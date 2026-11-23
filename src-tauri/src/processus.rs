// Lancer un serveur Minecraft et lui parler.
// Le serveur est un programme Java : on lit ce qu'il écrit (sa console)
// et on lui envoie des commandes comme si on les tapait au clavier.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Ce que le serveur nous apprend pendant qu'il tourne.
#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    /// Une ligne de sa console.
    Ligne(String),
    /// Il a fini de démarrer : les joueurs peuvent entrer.
    Pret,
    /// Il s'est arrêté. `propre` est faux s'il a planté.
    Fini { propre: bool },
}

pub type Ecoute = Arc<dyn Fn(Signal) + Send + Sync>;

/// Paramètres › Performances › Priorité : qui passe d'abord quand le PC est occupé (un jeu, un navigateur…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priorite {
    /// Le serveur laisse la place aux autres programmes.
    Basse,
    #[default]
    Normale,
    /// Le serveur passe avant les autres programmes.
    Haute,
}

impl Priorite {
    pub fn est_normale(&self) -> bool {
        *self == Priorite::Normale
    }
}

/// Le « bridage » de Windows 11 pour les programmes sans fenêtre (EcoQoS), sous la forme (ControlMask, StateMask).
/// Deux freins, tous deux coupés pour un serveur.
///
/// Frein « vitesse » (1) : Windows met le programme sur les petits cœurs, à basse fréquence, quand un jeu est au premier plan.
///
/// Frein « minuterie » (4) : Windows ignore la demande de précision de Java. Ses pauses durent alors 15,6 ms au lieu de 1 ms :
/// un tick de 50 ms passe à 62,5 ms, soit exactement 16 TPS au lieu de 20.
///
/// En priorité Basse, on laisse Windows décider de la vitesse, mais la minuterie reste précise.
/// StateMask à 0 : « jamais bridé » pour chaque frein choisi dans ControlMask.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn freins_coupes(priorite: Priorite) -> (u32, u32) {
    const VITESSE: u32 = 1; // PROCESS_POWER_THROTTLING_EXECUTION_SPEED
    const MINUTERIE: u32 = 4; // PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION
    match priorite {
        Priorite::Basse => (MINUTERIE, 0),
        _ => (VITESSE | MINUTERIE, 0),
    }
}

/// Un serveur en train de tourner.
pub struct Processus {
    /// Numéro du programme chez Windows (pour mesurer sa mémoire et son processeur).
    pub pid: u32,
    clavier: Mutex<Option<ChildStdin>>,
    enfant: Arc<Mutex<Child>>,
    /// Vrai si c'est nous qui avons demandé l'arrêt (sinon, c'est un plantage).
    pub arret_demande: AtomicBool,
    /// Vrai s'il faut le relancer dès qu'il s'est arrêté.
    pub relancer: AtomicBool,
    /// Windows : le « groupe » (Job Object) du serveur. Forcer l'arrêt coupe tout le groupe :
    /// Java et tout ce qu'il a lancé. Et si BHOST se ferme d'un coup (plantage), Windows coupe le groupe :
    /// aucun serveur ne reste allumé tout seul, sans console, avec son port bloqué.
    groupe: groupe::Groupe,
}

impl Processus {
    /// Envoie une commande au serveur, comme tapée dans sa console.
    pub fn ecrire(&self, commande: &str) -> Result<(), String> {
        // Une seule commande par ligne : un retour à la ligne glissé dans un texte n'ajoute jamais une 2e commande.
        if commande.contains(['\n', '\r']) {
            return Err("Une seule commande à la fois.".into());
        }
        let mut clavier = self.clavier.lock().unwrap();
        let Some(tuyau) = clavier.as_mut() else {
            return Err("Le serveur ne répond plus.".into());
        };
        writeln!(tuyau, "{commande}")
            .and_then(|_| tuyau.flush())
            .map_err(|_| "Le serveur ne répond plus.".to_string())
    }

    /// Donne au serveur sa priorité, et coupe le bridage de Windows (voir `freins_coupes`).
    /// Sous Linux (BHOST OS) : sa part du processeur face aux autres serveurs (groupe du serveur).
    /// Marche aussi pendant qu'il tourne.
    pub fn regler_priorite(&self, priorite: Priorite) -> Result<(), String> {
        self.groupe.priorite(priorite);
        let enfant = self.enfant.lock().unwrap();
        vitesse::regler(&enfant, priorite)
    }

    /// BHOST OS : la mémoire que le serveur ne peut pas dépasser (Java et tout ce qu'il lance).
    /// Au-delà, c'est lui qui s'arrête, pas la machine ni les autres serveurs. Ailleurs : rien.
    pub fn limiter_memoire(&self, mo: u64) {
        self.groupe.memoire_max(mo);
    }

    /// Coupe le serveur tout de suite, sans sauvegarder : Java et tout ce qu'il a lancé.
    pub fn tuer(&self) {
        self.groupe.couper();
        let _ = self.enfant.lock().unwrap().kill();
    }
}

/// Priorité et bridage (Windows). Ailleurs : rien.
mod vitesse {
    use super::Priorite;

    #[cfg(windows)]
    pub fn regler(enfant: &std::process::Child, priorite: Priorite) -> Result<(), String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Threading::{
            ProcessPowerThrottling, SetPriorityClass, SetProcessInformation, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
            NORMAL_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_STATE,
        };
        let classe = match priorite {
            Priorite::Basse => BELOW_NORMAL_PRIORITY_CLASS,
            Priorite::Normale => NORMAL_PRIORITY_CLASS,
            Priorite::Haute => HIGH_PRIORITY_CLASS,
        };
        let poignee = enfant.as_raw_handle() as _;
        let (controle, etat) = super::freins_coupes(priorite);
        let freins = |controle: u32| {
            let infos = PROCESS_POWER_THROTTLING_STATE { Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION, ControlMask: controle, StateMask: etat };
            // SAFETY : `poignee` est celle du programme que nous avons lancé (vivant tant que `enfant` est verrouillé),
            // `infos` vit pendant l'appel et sa taille est la bonne.
            unsafe {
                SetProcessInformation(
                    poignee,
                    ProcessPowerThrottling,
                    &infos as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
                ) != 0
            }
        };
        // Windows 10 ne connaît pas le frein « minuterie » : on réessaie sans lui.
        if !freins(controle) {
            let _ = freins(controle & !4);
        }
        // SAFETY : même poignée, appel simple.
        if unsafe { SetPriorityClass(poignee, classe) } == 0 {
            return Err("Windows a refusé de changer la priorité.".into());
        }
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn regler(_enfant: &std::process::Child, _priorite: Priorite) -> Result<(), String> {
        Ok(())
    }
}

/// Le groupe de programmes d'un serveur (Windows : Job Object). Ailleurs : rien.
pub(crate) mod groupe {
    #[cfg(windows)]
    pub struct Groupe(Option<isize>);

    #[cfg(windows)]
    impl Groupe {
        /// Crée un groupe et y range le programme `enfant` (ses futurs programmes y entrent tout seuls).
        pub fn ranger(enfant: &std::process::Child) -> Groupe {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            };
            // SAFETY : appels Windows simples ; `infos` vit pendant l'appel, sa taille est la bonne,
            // et le programme `enfant` est vivant (on vient de le lancer, son numéro est encore à nous).
            unsafe {
                let groupe = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if groupe.is_null() {
                    return Groupe(None);
                }
                let mut infos: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                infos.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let regle = SetInformationJobObject(
                    groupe,
                    JobObjectExtendedLimitInformation,
                    &infos as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if regle == 0 || AssignProcessToJobObject(groupe, enfant.as_raw_handle() as _) == 0 {
                    CloseHandle(groupe);
                    return Groupe(None);
                }
                Groupe(Some(groupe as isize))
            }
        }

        /// Coupe tous les programmes du groupe.
        pub fn couper(&self) {
            if let Some(groupe) = self.0 {
                // SAFETY : `groupe` est notre groupe, ouvert tant que ce Groupe existe.
                unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(groupe as _, 1) };
            }
        }
    }

    #[cfg(windows)]
    impl Drop for Groupe {
        fn drop(&mut self) {
            if let Some(groupe) = self.0 {
                // SAFETY : on rend notre groupe à Windows une seule fois (le serveur est déjà arrêté à ce moment-là).
                unsafe { windows_sys::Win32::Foundation::CloseHandle(groupe as _) };
            }
        }
    }

    #[cfg(windows)]
    impl Groupe {
        pub fn priorite(&self, _priorite: super::Priorite) {}
        pub fn memoire_max(&self, _mo: u64) {}
    }

    // Linux (BHOST OS) : un groupe de contrôle (cgroup v2) par serveur, sous celui de l'agent
    // (systemd le lui confie : Delegate= dans bhost-agent.service). Voir `preparer`.
    #[cfg(target_os = "linux")]
    pub struct Groupe(Option<std::path::PathBuf>);

    #[cfg(target_os = "linux")]
    impl Groupe {
        pub fn ranger(enfant: &std::process::Child) -> Groupe {
            let Some(base) = super::cgroupes::base() else { return Groupe(None) };
            let dossier = base.join(format!("serveur-{}", enfant.id()));
            let pret = std::fs::create_dir(&dossier).is_ok() && std::fs::write(dossier.join("cgroup.procs"), enfant.id().to_string()).is_ok();
            if !pret {
                let _ = std::fs::remove_dir(&dossier);
                return Groupe(None);
            }
            Groupe(Some(dossier))
        }
        /// Coupe tous les programmes du groupe d'un coup (Java et ce qu'il a lancé).
        pub fn couper(&self) {
            if let Some(d) = &self.0 {
                let _ = std::fs::write(d.join("cgroup.kill"), "1");
            }
        }
        pub fn priorite(&self, priorite: super::Priorite) {
            if let Some(d) = &self.0 {
                let _ = std::fs::write(d.join("cpu.weight"), super::cgroupes::poids(priorite).to_string());
            }
        }
        pub fn memoire_max(&self, mo: u64) {
            if let Some(d) = &self.0 {
                let _ = std::fs::write(d.join("memory.max"), (mo * 1024 * 1024).to_string());
            }
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for Groupe {
        fn drop(&mut self) {
            // Le serveur est arrêté : le groupe est vide, on le retire (sinon `preparer` le fera au prochain démarrage).
            if let Some(d) = &self.0 {
                let _ = std::fs::remove_dir(d);
            }
        }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub struct Groupe;

    #[cfg(not(any(windows, target_os = "linux")))]
    impl Groupe {
        pub fn ranger(_enfant: &std::process::Child) -> Groupe {
            Groupe
        }
        pub fn couper(&self) {}
        pub fn priorite(&self, _priorite: super::Priorite) {}
        pub fn memoire_max(&self, _mo: u64) {}
    }
}

/// BHOST OS : les groupes de contrôle (cgroup v2) des serveurs.
/// systemd confie à l'agent son propre groupe (Delegate=). Une règle du noyau interdit d'y mettre à la fois
/// des programmes et des sous-groupes réglés : l'agent passe donc dans le sous-groupe « agent », et chaque
/// serveur a le sien (« serveur-<numéro> ») avec sa mémoire max et sa part de processeur.
pub mod cgroupes {
    use super::Priorite;
    #[cfg(target_os = "linux")]
    use std::path::{Path, PathBuf};
    #[cfg(target_os = "linux")]
    use std::sync::OnceLock;

    /// Part du processeur quand plusieurs serveurs le veulent (100 = normal pour le noyau).
    pub fn poids(priorite: Priorite) -> u32 {
        match priorite {
            Priorite::Basse => 25,
            Priorite::Normale => 100,
            Priorite::Haute => 400,
        }
    }

    #[cfg(target_os = "linux")]
    static BASE: OnceLock<Option<PathBuf>> = OnceLock::new();

    /// Le groupe confié à l'agent, prêt pour les serveurs (ou rien : pas BHOST OS, pas de cgroup v2…).
    #[cfg(target_os = "linux")]
    pub fn base() -> Option<PathBuf> {
        BASE.get_or_init(|| preparer_dans(Path::new("/sys/fs/cgroup"), &std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default())).clone()
    }

    /// Vrai si les serveurs ont leur groupe (BHOST OS).
    pub fn actives() -> bool {
        #[cfg(target_os = "linux")]
        {
            BASE.get().is_some_and(|b| b.is_some())
        }
        #[cfg(not(target_os = "linux"))]
        false
    }

    /// À appeler au démarrage de l'agent, avant de lancer quoi que ce soit.
    pub fn preparer() {
        #[cfg(target_os = "linux")]
        {
            let _ = base();
        }
    }

    /// `racine` : où est monté cgroup2 ; `moi` : le contenu de /proc/self/cgroup (« 0::/chemin »).
    #[cfg(target_os = "linux")]
    pub fn preparer_dans(racine: &Path, moi: &str) -> Option<PathBuf> {
        let chemin = moi.lines().find_map(|l| l.strip_prefix("0::"))?.trim();
        let chemin = chemin.trim_start_matches('/');
        let mut base = racine.join(chemin);
        // Déjà dans « agent » (l'agent a redémarré sans que systemd recrée son groupe) : on remonte.
        if base.file_name().is_some_and(|n| n == "agent") {
            base.pop();
        }
        if chemin.is_empty() || !base.join("cgroup.subtree_control").exists() {
            return None;
        }
        let agent = base.join("agent");
        std::fs::create_dir_all(&agent).ok()?;
        std::fs::write(agent.join("cgroup.procs"), std::process::id().to_string()).ok()?;
        // Les groupes de serveurs vides laissés par un arrêt brutal.
        if let Ok(liste) = std::fs::read_dir(&base) {
            for e in liste.flatten() {
                if e.file_name().to_string_lossy().starts_with("serveur-") {
                    let _ = std::fs::remove_dir(e.path());
                }
            }
        }
        std::fs::write(base.join("cgroup.subtree_control"), "+memory +cpu").ok()?;
        Some(base)
    }
}

/// Un octet de l'ancien encodage de Windows (CP1252) : comme Latin-1, sauf 0x80–0x9F (’ œ € … « »…).
fn windows_1252(o: u8) -> char {
    const HAUT: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™',
        'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    if (0x80..0xA0).contains(&o) { HAUT[(o - 0x80) as usize] } else { o as char }
}

/// Enlève les codes de couleur du terminal et les retours chariot d'une ligne.
pub fn nettoyer(octets: &[u8]) -> String {
    // UTF-8 si possible ; sinon l'ancien encodage de Windows (un octet = une lettre).
    let texte = match std::str::from_utf8(octets) {
        Ok(t) => t.to_string(),
        Err(_) => octets.iter().map(|&o| windows_1252(o)).collect(),
    };
    let mut propre = String::with_capacity(texte.len());
    let mut lettres = texte.chars().peekable();
    while let Some(c) = lettres.next() {
        if c == '\u{1b}' {
            // Code du genre ESC [ 0 ; 3 2 m : on saute jusqu'à la lettre finale.
            if lettres.peek() == Some(&'[') {
                lettres.next();
                for suivant in lettres.by_ref() {
                    if suivant.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c != '\r' && c != '\n' {
            propre.push(c);
        }
    }
    // Certains serveurs redessinent une invite « > » en début de ligne.
    let propre = propre.trim_end();
    propre.strip_prefix('>').map(str::trim_start).unwrap_or(propre).to_string()
}

/// Lit un flux ligne par ligne et prévient à chaque ligne.
/// `en_cours` compte les lecteurs qui n'ont pas encore fini.
fn lire(flux: impl Read + Send + 'static, ecoute: Ecoute, en_cours: Arc<AtomicUsize>) {
    en_cours.fetch_add(1, Ordering::SeqCst);
    thread::spawn(move || {
        let mut lecteur = BufReader::new(flux);
        let mut octets = Vec::new();
        loop {
            octets.clear();
            // Une ligne de 64 Ko au plus : au-delà, la suite arrive comme une autre ligne (pas de mémoire sans fin).
            match (&mut lecteur).take(64 * 1024).read_until(b'\n', &mut octets) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let ligne = nettoyer(&octets);
                    if ligne.is_empty() {
                        continue;
                    }
                    let pret = ligne.contains("Done (") && ligne.contains("For help, type");
                    ecoute(Signal::Ligne(ligne));
                    if pret {
                        ecoute(Signal::Pret);
                    }
                }
            }
        }
        en_cours.fetch_sub(1, Ordering::SeqCst);
    });
}

/// Lance `programme` avec `arguments` dans `dossier`, et prévient `ecoute` de tout ce qui se passe.
/// `variables` : des variables d'environnement en plus pour le serveur (GEYSER_BEDROCK_PORT…).
pub fn lancer(dossier: &Path, programme: &str, arguments: &[String], variables: &[(String, String)], ecoute: Ecoute) -> Result<Arc<Processus>, String> {
    let mut commande = Command::new(programme);
    // BHOST OS : le serveur ne voit que son dossier, Java et le système (pas les clés de la machine ni les autres serveurs).
    #[cfg(all(target_os = "linux", feature = "agent"))]
    let _cloison = cloison::enfermer(&mut commande, dossier, Path::new(programme));
    commande
        .args(arguments)
        .envs(variables.iter().map(|(k, v)| (k, v)))
        .current_dir(dossier)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Sur Windows : ne pas ouvrir de fenêtre noire à côté de BHOST.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        commande.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut enfant = commande.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Java est introuvable sur ce PC.".to_string()
        } else {
            format!("Le serveur n’a pas pu démarrer ({e}).")
        }
    })?;

    let groupe = groupe::Groupe::ranger(&enfant);
    let clavier = enfant.stdin.take();
    let lecteurs = Arc::new(AtomicUsize::new(0));
    if let Some(sortie) = enfant.stdout.take() {
        lire(sortie, ecoute.clone(), lecteurs.clone());
    }
    if let Some(erreurs) = enfant.stderr.take() {
        lire(erreurs, ecoute.clone(), lecteurs.clone());
    }
    let processus = Arc::new(Processus {
        pid: enfant.id(),
        clavier: Mutex::new(clavier),
        enfant: Arc::new(Mutex::new(enfant)),
        arret_demande: AtomicBool::new(false),
        relancer: AtomicBool::new(false),
        groupe,
    });

    // Un fil qui attend la fin du serveur, puis prévient.
    let suivi = processus.clone();
    thread::spawn(move || {
        let code = loop {
            if let Ok(Some(code)) = suivi.enfant.lock().unwrap().try_wait() {
                break Some(code);
            }
            thread::sleep(Duration::from_millis(150));
        };
        // On laisse les lecteurs finir, pour ne perdre aucune ligne (2 secondes au plus).
        for _ in 0..40 {
            if lecteurs.load(Ordering::SeqCst) == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        suivi.clavier.lock().unwrap().take();
        let propre = suivi.arret_demande.load(Ordering::SeqCst) || code.is_some_and(|c| c.success());
        ecoute(Signal::Fini { propre });
    });

    Ok(processus)
}

/// BHOST OS : un serveur Minecraft (et ses plugins) est enfermé avec Landlock (noyau Linux ≥ 5.13).
/// Il peut écrire dans son dossier, /tmp et /dev ; lire Java et le système. Il ne peut ni lire les clés de la machine
/// (relais-cle.txt, machine-code.txt), ni toucher aux autres serveurs. Sans Landlock (vieux noyau), rien ne change.
#[cfg(target_os = "linux")]
#[cfg_attr(not(feature = "agent"), allow(dead_code))]
pub mod cloison {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::process::Command;

    const EXECUTE: u64 = 1 << 0;
    const READ_FILE: u64 = 1 << 2;
    const READ_DIR: u64 = 1 << 3;
    /// ABI 1 : les 13 premiers droits sur les fichiers.
    const ABI1: u64 = (1 << 13) - 1;
    const REFER: u64 = 1 << 13;
    const TRUNCATE: u64 = 1 << 14;
    const IOCTL_DEV: u64 = 1 << 15;
    const LECTURE: u64 = EXECUTE | READ_FILE | READ_DIR;

    #[repr(C)]
    struct Regles {
        fs: u64,
    }
    #[repr(C, packed)]
    struct Sous {
        permis: u64,
        dossier: i32,
    }

    fn droits() -> Option<u64> {
        // SAFETY : demande seulement la version de Landlock (pointeur nul, taille 0).
        let abi = unsafe { libc::syscall(libc::SYS_landlock_create_ruleset, std::ptr::null::<Regles>(), 0usize, 1u32) };
        if abi < 1 {
            return None;
        }
        let mut d = ABI1;
        if abi >= 2 {
            d |= REFER;
        }
        if abi >= 3 {
            d |= TRUNCATE;
        }
        if abi >= 5 {
            d |= IOCTL_DEV;
        }
        Some(d)
    }

    fn permettre(regles: &OwnedFd, chemin: &Path, permis: u64) {
        let Ok(c) = CString::new(chemin.as_os_str().as_bytes()) else { return };
        // SAFETY : open avec O_PATH ne lit rien ; le descripteur est fermé à la fin.
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if fd < 0 {
            return;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        // Un fichier (pas un dossier) n'accepte que les droits d'un fichier.
        let est_dossier = std::fs::metadata(chemin).is_ok_and(|m| m.is_dir());
        let permis = if est_dossier { permis } else { permis & (EXECUTE | READ_FILE | (1 << 1) | TRUNCATE | IOCTL_DEV) };
        let regle = Sous { permis, dossier: fd.as_raw_fd() };
        // SAFETY : la règle est une structure valide pendant l'appel.
        unsafe { libc::syscall(libc::SYS_landlock_add_rule, regles.as_raw_fd(), 1u32, &regle as *const Sous, 0u32) };
    }

    /// Prépare la cloison et la pose sur `commande` (appliquée dans le processus du serveur, juste avant Java).
    /// Le descripteur rendu doit vivre jusqu'au lancement.
    pub fn enfermer(commande: &mut Command, dossier: &Path, programme: &Path) -> Option<OwnedFd> {
        let tout = droits()?;
        let r = Regles { fs: tout };
        // SAFETY : la structure vit pendant l'appel ; le noyau rend un descripteur (O_CLOEXEC).
        let fd = unsafe { libc::syscall(libc::SYS_landlock_create_ruleset, &r as *const Regles, std::mem::size_of::<Regles>(), 0u32) };
        if fd < 0 {
            return None;
        }
        let regles = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        for systeme in ["/usr", "/bin", "/sbin", "/lib", "/lib32", "/lib64", "/etc", "/opt", "/proc", "/sys", "/run"] {
            permettre(&regles, Path::new(systeme), LECTURE);
        }
        for ecriture in ["/tmp", "/var/tmp", "/dev"] {
            permettre(&regles, Path::new(ecriture), tout);
        }
        // Java : le dossier de son installation (…/java/21 pour …/java/21/bin/java).
        if let Ok(vrai) = std::fs::canonicalize(programme)
            && let Some(maison) = vrai.parent().and_then(Path::parent)
        {
            permettre(&regles, maison, LECTURE);
        }
        permettre(&regles, dossier, tout);
        // Les réglages que Java et les plugins rangent dans « ~ » vont dans le dossier du serveur.
        commande.env("HOME", dossier);
        let brut = regles.as_raw_fd();
        // SAFETY : entre fork et exec, seulement deux appels système (pas d'allocation).
        unsafe {
            commande.pre_exec(move || {
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::syscall(libc::SYS_landlock_restrict_self, brut, 0u32) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Some(regles)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn le_serveur_ne_voit_que_son_dossier() {
            let racine = std::env::temp_dir().join(format!("bhost-cloison-{}", std::process::id()));
            let serveur = racine.join("serveurs").join("s1");
            std::fs::create_dir_all(&serveur).unwrap();
            std::fs::write(racine.join("machine-code.txt"), "SECRET").unwrap();
            // Dans /var/lib (pas dans /tmp, qui est permis) : on copie la racine ailleurs si possible.
            let racine2 = std::path::PathBuf::from("/var/lib/bhost-essai-cloison");
            if std::fs::create_dir_all(racine2.join("s1")).is_err() || droits().is_none() {
                return; // pas les droits, ou pas de Landlock ici
            }
            std::fs::write(racine2.join("machine-code.txt"), "SECRET").unwrap();
            let lancer = |script: &str| {
                let mut c = Command::new("/bin/sh");
                c.args(["-c", script]).current_dir(racine2.join("s1"));
                let _g = enfermer(&mut c, &racine2.join("s1"), Path::new("/bin/sh"));
                c.status().unwrap().success()
            };
            assert!(lancer("echo ok > ./monde.txt && cat ./monde.txt"));
            assert!(!lancer("cat ../machine-code.txt"));
            assert!(!lancer("echo x > ../autre.txt"));
            assert!(lancer("ls /usr/bin > /dev/null && echo x > /tmp/bhost-essai-cloison"));
            // Java démarre et écrit dans son dossier (si Java est installé ici).
            if Path::new("/usr/bin/java").exists() {
                std::fs::write(racine2.join("s1").join("Essai.java"), "public class Essai { public static void main(String[] a) throws Exception { java.nio.file.Files.writeString(java.nio.file.Path.of(\"monde.dat\"), \"ok\"); System.out.println(Runtime.getRuntime().availableProcessors()); } }").unwrap();
                let mut c = Command::new("/usr/bin/java");
                c.args(["-Xmx64m", "Essai.java"]).current_dir(racine2.join("s1"));
                let _g = enfermer(&mut c, &racine2.join("s1"), Path::new("/usr/bin/java"));
                let sortie = c.output().unwrap();
                assert!(sortie.status.success(), "{}", String::from_utf8_lossy(&sortie.stderr));
                assert_eq!(std::fs::read_to_string(racine2.join("s1").join("monde.dat")).unwrap(), "ok");
            }
            let _ = std::fs::remove_dir_all(&racine2);
            let _ = std::fs::remove_dir_all(&racine);
        }
    }
}

#[cfg(test)]
mod tests_priorite {
    #[cfg(target_os = "linux")]
    #[test]
    fn groupes_de_bhost_os() {
        let d = std::env::temp_dir().join(format!("bhost-cgroupes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let service = d.join("system.slice/bhost-agent.service");
        std::fs::create_dir_all(service.join("serveur-12")).unwrap();
        std::fs::write(service.join("cgroup.subtree_control"), "").unwrap();
        let base = cgroupes::preparer_dans(&d, "0::/system.slice/bhost-agent.service\n").unwrap();
        assert_eq!(base, service);
        assert_eq!(std::fs::read_to_string(service.join("agent/cgroup.procs")).unwrap(), std::process::id().to_string());
        assert_eq!(std::fs::read_to_string(service.join("cgroup.subtree_control")).unwrap(), "+memory +cpu");
        assert!(!service.join("serveur-12").exists(), "un ancien groupe vide est retiré");
        // L'agent a redémarré dans « agent » : même base.
        assert_eq!(cgroupes::preparer_dans(&d, "0::/system.slice/bhost-agent.service/agent\n").unwrap(), service);
        // Pas de groupe confié (pas BHOST OS) : rien.
        assert!(cgroupes::preparer_dans(&d, "0::/\n").is_none());
        assert!(cgroupes::preparer_dans(&d, "0::/user.slice/ailleurs\n").is_none());
        assert_eq!(cgroupes::poids(Priorite::Haute), 400);
        let _ = std::fs::remove_dir_all(&d);
    }

    use super::*;

    #[test]
    fn freins() {
        // Normale et Haute : vitesse et minuterie jamais bridées ; Basse : seulement la minuterie.
        assert_eq!(freins_coupes(Priorite::Normale), (5, 0));
        assert_eq!(freins_coupes(Priorite::Haute), (5, 0));
        assert_eq!(freins_coupes(Priorite::Basse), (4, 0));
    }

    #[test]
    fn priorite_dans_le_fichier() {
        assert_eq!(serde_json::to_string(&Priorite::Haute).unwrap(), "\"haute\"");
        assert_eq!(serde_json::from_str::<Priorite>("\"basse\"").unwrap(), Priorite::Basse);
        assert_eq!(Priorite::default(), Priorite::Normale);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Un faux serveur : un petit script qui parle comme Minecraft.
    fn faux_serveur(script: &str) -> (Arc<Processus>, mpsc::Receiver<Signal>) {
        let (envoi, reception) = mpsc::channel();
        let envoi = Mutex::new(envoi);
        let ecoute: Ecoute = Arc::new(move |s| {
            let _ = envoi.lock().unwrap().send(s);
        });
        let p = lancer(&std::env::temp_dir(), "sh", &["-c".into(), script.into()], &[], ecoute).unwrap();
        (p, reception)
    }

    fn tout(reception: &mpsc::Receiver<Signal>) -> Vec<Signal> {
        let mut signaux = Vec::new();
        while let Ok(s) = reception.recv_timeout(Duration::from_secs(5)) {
            let fini = matches!(s, Signal::Fini { .. });
            signaux.push(s);
            if fini {
                break;
            }
        }
        signaux
    }

    const SCRIPT: &str = r#"
        echo '[10:00:06 INFO]: Starting minecraft server version 1.8.8'
        echo '[10:00:08 INFO]: Done (2,1s)! For help, type "help" or "?"'
        while read ligne; do
          if [ "$ligne" = "stop" ]; then echo '[10:00:09 INFO]: Stopping server'; exit 0; fi
          echo "[10:00:09 INFO]: recu $ligne"
        done
    "#;

    #[test]
    fn donne_les_variables_au_serveur() {
        let (envoi, reception) = mpsc::channel();
        let envoi = Mutex::new(envoi);
        let ecoute: Ecoute = Arc::new(move |s| {
            let _ = envoi.lock().unwrap().send(s);
        });
        let variables = [("GEYSER_BEDROCK_PORT".to_string(), "19133".to_string())];
        let _p = lancer(&std::env::temp_dir(), "sh", &["-c".into(), "echo port=$GEYSER_BEDROCK_PORT".into()], &variables, ecoute).unwrap();
        assert_eq!(reception.recv_timeout(Duration::from_secs(5)).unwrap(), Signal::Ligne("port=19133".into()));
    }

    #[test]
    fn demarre_ecoute_une_commande_et_s_arrete_proprement() {
        let (p, reception) = faux_serveur(SCRIPT);
        assert_eq!(reception.recv_timeout(Duration::from_secs(5)).unwrap(), Signal::Ligne("[10:00:06 INFO]: Starting minecraft server version 1.8.8".into()));
        assert!(matches!(reception.recv_timeout(Duration::from_secs(5)).unwrap(), Signal::Ligne(_)));
        assert_eq!(reception.recv_timeout(Duration::from_secs(5)).unwrap(), Signal::Pret);
        p.ecrire("say Bonjour").unwrap();
        assert_eq!(reception.recv_timeout(Duration::from_secs(5)).unwrap(), Signal::Ligne("[10:00:09 INFO]: recu say Bonjour".into()));
        p.arret_demande.store(true, Ordering::SeqCst);
        p.ecrire("stop").unwrap();
        let fin = tout(&reception);
        assert_eq!(fin.last(), Some(&Signal::Fini { propre: true }));
        assert!(p.ecrire("list").is_err());
    }

    #[test]
    fn un_plantage_n_est_pas_un_arret_propre() {
        let (_p, reception) = faux_serveur("echo 'java.lang.OutOfMemoryError' >&2; exit 1");
        let signaux = tout(&reception);
        assert_eq!(signaux, vec![Signal::Ligne("java.lang.OutOfMemoryError".into()), Signal::Fini { propre: false }]);
    }

    #[test]
    fn forcer_l_arret_coupe_tout_de_suite() {
        let (p, reception) = faux_serveur("echo debut; exec sleep 60");
        assert_eq!(reception.recv_timeout(Duration::from_secs(5)).unwrap(), Signal::Ligne("debut".into()));
        p.arret_demande.store(true, Ordering::SeqCst);
        p.tuer();
        assert_eq!(tout(&reception).last(), Some(&Signal::Fini { propre: true }));
    }

    #[test]
    fn java_introuvable_donne_une_phrase_claire() {
        let ecoute: Ecoute = Arc::new(|_| {});
        let erreur = lancer(&std::env::temp_dir(), "programme-qui-n-existe-pas", &[], &[], ecoute).err().unwrap();
        assert_eq!(erreur, "Java est introuvable sur ce PC.");
    }

    #[test]
    fn nettoie_les_couleurs_les_invites_et_le_latin1() {
        assert_eq!(nettoyer(b"\x1b[0;32m[10:00:06 INFO]: Bonjour\x1b[m\r\n"), "[10:00:06 INFO]: Bonjour");
        assert_eq!(nettoyer(b">\r[10:00:06 INFO]: Salut\n"), "[10:00:06 INFO]: Salut");
        assert_eq!(nettoyer(b"caract\xe8re\n"), "caractère");
        assert_eq!(nettoyer(b">\r\n"), "");
    }
}
