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

    /// Coupe le serveur tout de suite, sans sauvegarder : Java et tout ce qu'il a lancé.
    pub fn tuer(&self) {
        self.groupe.couper();
        let _ = self.enfant.lock().unwrap().kill();
    }
}

/// Le groupe de programmes d'un serveur (Windows : Job Object). Ailleurs : rien.
mod groupe {
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

    #[cfg(not(windows))]
    pub struct Groupe;

    #[cfg(not(windows))]
    impl Groupe {
        pub fn ranger(_enfant: &std::process::Child) -> Groupe {
            Groupe
        }
        pub fn couper(&self) {}
    }
}

/// Enlève les codes de couleur du terminal et les retours chariot d'une ligne.
pub fn nettoyer(octets: &[u8]) -> String {
    // UTF-8 si possible ; sinon l'ancien encodage de Windows (un octet = une lettre).
    let texte = match std::str::from_utf8(octets) {
        Ok(t) => t.to_string(),
        Err(_) => octets.iter().map(|&o| o as char).collect(),
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
            format!("Le serveur n'a pas pu démarrer ({e}).")
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
