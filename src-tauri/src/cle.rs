//! Machine dédiée, étape « Clé USB » : BHOST écrit BHOST OS sur une clé, avec le Wi-Fi de la maison.
//!
//! 1. L'ISO : téléchargée (dernière version publiée) ou choisie sur le PC.
//! 2. Le Wi-Fi : l'ISO contient une zone de taille fixe (bhost/wifi.txt, repérée par sa 1re ligne) ;
//!    BHOST la remplit au moment d'écrire la clé, sans refaire l'ISO. L'installeur de BHOST OS la lit.
//! 3. L'écriture : il faut les droits d'administrateur (écrire un disque entier). BHOST se relance donc
//!    lui-même, en administrateur (Windows demande la permission), avec « --ecrire-cle <travail.json> ».
//!    Cette copie vérifie que le disque est bien une clé USB, l'efface (diskpart), puis écrit l'ISO.
//!    Elle note son avancement dans un petit fichier que BHOST lit pour la barre de progression.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// La dernière version publiée de BHOST OS, et son empreinte SHA-256 (« <hexa>  bhost-os.iso »).
pub const ISO_URL: &str = "https://github.com/rerebxrn/bhost/releases/latest/download/bhost-os.iso";
pub const EMPREINTE_URL: &str = "https://github.com/rerebxrn/bhost/releases/latest/download/bhost-os.iso.sha256";

/// La 1re ligne de la zone du Wi-Fi dans l'ISO (voir os/construire.sh), et sa taille.
pub const REPERE_WIFI: &[u8] = b"# BHOST-WIFI v1\n";
pub const TAILLE_WIFI: usize = 4096;

/// Option de la copie administrateur de BHOST.
pub const OPTION: &str = "--ecrire-cle";

/// Ce que la copie administrateur doit faire (écrit dans un fichier JSON par BHOST).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Travail {
    /// L'ISO de BHOST OS sur ce PC.
    pub iso: PathBuf,
    /// La clé : son numéro de disque chez Windows (Get-Disk).
    pub disque: u32,
    /// Où commence la zone du Wi-Fi dans l'ISO (en octets), et ce qu'il faut y mettre.
    pub wifi_position: Option<u64>,
    pub wifi: Option<String>,
    /// L'avancement (« 42 ») puis le résultat (« ok » ou une phrase d'erreur).
    pub progres: PathBuf,
}

// ---------- Le Wi-Fi ----------

/// La zone du Wi-Fi remplie : « nom= » et « motdepasse= », complétée par des retours à la ligne.
pub fn bloc_wifi(nom: &str, motdepasse: &str) -> Result<Vec<u8>, String> {
    let propre = |t: &str| !t.contains(['\n', '\r', '\0']);
    if !propre(nom) || !propre(motdepasse) {
        return Err("Le nom ou le mot de passe du Wi-Fi contient un caractère interdit.".into());
    }
    if nom.len() > 32 {
        return Err("Le nom du Wi-Fi est trop long (32 caractères au plus).".into());
    }
    if !motdepasse.is_empty() && !(8..=63).contains(&motdepasse.len()) {
        return Err("Le mot de passe du Wi-Fi doit avoir entre 8 et 63 caractères.".into());
    }
    let mut bloc = Vec::with_capacity(TAILLE_WIFI);
    bloc.extend_from_slice(REPERE_WIFI);
    bloc.extend_from_slice(format!("nom={nom}\nmotdepasse={motdepasse}\n").as_bytes());
    bloc.resize(TAILLE_WIFI, b'\n');
    Ok(bloc)
}

/// Cherche la zone du Wi-Fi dans l'ISO (lecture par morceaux : l'ISO fait plusieurs centaines de Mo).
pub fn trouver_zone_wifi(iso: &Path) -> io::Result<Option<u64>> {
    let mut f = File::open(iso)?;
    let mut tampon = vec![0u8; 4 << 20];
    let mut garde: Vec<u8> = Vec::new(); // la fin du morceau précédent (le repère peut être à cheval)
    let mut debut_garde = 0u64;
    loop {
        let n = f.read(&mut tampon)?;
        if n == 0 {
            return Ok(None);
        }
        let mut fenetre = std::mem::take(&mut garde);
        fenetre.extend_from_slice(&tampon[..n]);
        if let Some(i) = fenetre.windows(REPERE_WIFI.len()).position(|w| w == REPERE_WIFI) {
            return Ok(Some(debut_garde + i as u64));
        }
        let reste = REPERE_WIFI.len() - 1;
        let coupe = fenetre.len().saturating_sub(reste);
        debut_garde += coupe as u64;
        garde = fenetre[coupe..].to_vec();
    }
}

/// Le Wi-Fi de ce PC (pour pré-remplir) : son nom, et son mot de passe si Windows le donne.
#[derive(Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WifiDuPc {
    pub nom: String,
    pub motdepasse: String,
}

/// « SSID : Maison » dans la sortie de « netsh wlan show interfaces » (le mot SSID ne se traduit pas).
pub fn ssid_de_netsh(sortie: &str) -> Option<String> {
    sortie.lines().find_map(|l| {
        let (cle, valeur) = l.split_once(':')?;
        (cle.trim() == "SSID").then(|| valeur.trim().to_string()).filter(|v| !v.is_empty())
    })
}

/// Le mot de passe dans le profil exporté par « netsh wlan export profile … key=clear » (XML).
pub fn cle_du_profil(xml: &str) -> Option<String> {
    let debut = xml.find("<keyMaterial>")? + "<keyMaterial>".len();
    let fin = xml[debut..].find("</keyMaterial>")? + debut;
    let cle = xml[debut..fin].replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'");
    Some(cle).filter(|c| !c.is_empty())
}

fn commande(programme: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(programme);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn wifi_du_pc() -> WifiDuPc {
    crate::hote::async_runtime::spawn_blocking(wifi_du_pc_maintenant).await.unwrap_or_default()
}

fn wifi_du_pc_maintenant() -> WifiDuPc {
    if !cfg!(windows) {
        return WifiDuPc::default();
    }
    let Ok(sortie) = commande("netsh").args(["wlan", "show", "interfaces"]).output() else { return WifiDuPc::default() };
    let Some(nom) = ssid_de_netsh(&String::from_utf8_lossy(&sortie.stdout)) else { return WifiDuPc::default() };
    // Le mot de passe : Windows le donne pour les profils de cet utilisateur (sinon, on le demande).
    let dossier = std::env::temp_dir().join(format!("bhost-wifi-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dossier);
    let _ = commande("netsh")
        .args(["wlan", "export", "profile"])
        .arg(format!("name={nom}"))
        .arg("key=clear")
        .arg(format!("folder={}", dossier.display()))
        .output();
    let motdepasse = std::fs::read_dir(&dossier)
        .ok()
        .and_then(|l| l.flatten().find_map(|e| std::fs::read_to_string(e.path()).ok().and_then(|x| cle_du_profil(&x))))
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dossier);
    WifiDuPc { nom, motdepasse }
}

// ---------- La copie administrateur : écrire l'ISO sur la clé ----------

/// Le début de la clé (table des partitions comprise) est écrit en dernier : tant qu'il n'est pas écrit,
/// Windows ne voit aucune partition sur la clé, donc ne monte rien, ne bloque rien et ne propose pas de la formater.
pub const DEBUT_EN_DERNIER: u64 = 1 << 20;
/// Nouveaux essais après une écriture ratée.
const ESSAIS_ECRITURE: u32 = 3;

/// Relit la clé et la compare à l'ISO (zone du Wi-Fi comprise). Faux : la clé ne contient pas l'ISO.
pub fn verifier(iso: &Path, disque: &mut (impl Read + Seek), wifi: Option<(u64, &[u8])>, mut progres: impl FnMut(u8)) -> io::Result<bool> {
    let mut source = File::open(iso)?;
    let total = source.metadata()?.len().max(1);
    disque.seek(SeekFrom::Start(0))?;
    let (mut attendu, mut lu) = (vec![0u8; 4 << 20], vec![0u8; 4 << 20]);
    let mut position = 0u64;
    let mut dernier = 255u8;
    while position < total {
        let voulu = ((total - position) as usize).min(attendu.len());
        source.read_exact(&mut attendu[..voulu])?;
        if let Some((zone, bloc)) = wifi {
            let (x, y) = (zone.max(position), (zone + bloc.len() as u64).min(position + voulu as u64));
            if x < y {
                attendu[(x - position) as usize..(y - position) as usize].copy_from_slice(&bloc[(x - zone) as usize..(y - zone) as usize]);
            }
        }
        // Les disques se lisent par secteurs entiers.
        let longueur = voulu.div_ceil(512) * 512;
        disque.read_exact(&mut lu[..longueur])?;
        if lu[..voulu] != attendu[..voulu] {
            return Ok(false);
        }
        position += voulu as u64;
        let pct = ((position * 100) / total).min(100) as u8;
        if pct != dernier {
            dernier = pct;
            progres(pct);
        }
    }
    Ok(true)
}

/// Écrit l'ISO sur `disque` (un fichier ou \\.\PhysicalDriveN), en remplaçant la zone du Wi-Fi.
/// `progres` reçoit le pourcentage. Chaque écriture est un multiple de 512 octets (secteurs).
pub fn copier(iso: &Path, disque: &mut (impl Write + Seek), wifi: Option<(u64, &[u8])>, mut progres: impl FnMut(u8)) -> io::Result<()> {
    let mut source = File::open(iso)?;
    let total = source.metadata()?.len().max(1);
    let debut = DEBUT_EN_DERNIER.min(total);
    let mut fait = 0u64;
    let mut dernier = 255u8;
    let mut avancer = |n: u64| {
        fait += n;
        let pct = ((fait * 100) / total).min(100) as u8;
        if pct != dernier {
            dernier = pct;
            progres(pct);
        }
    };
    // 1. Tout sauf le début ; 2. le début.
    for (de, a) in [(debut, total), (0, debut)] {
        source.seek(SeekFrom::Start(de))?;
        disque.seek(SeekFrom::Start(de))?;
        let mut position = de;
        let mut tampon = vec![0u8; 4 << 20];
        while position < a {
            let voulu = ((a - position) as usize).min(tampon.len());
            source.read_exact(&mut tampon[..voulu])?;
            // La zone du Wi-Fi, si elle tombe (en partie) dans ce morceau.
            if let Some((zone, bloc)) = wifi {
                let (x, y) = (zone.max(position), (zone + bloc.len() as u64).min(position + voulu as u64));
                if x < y {
                    tampon[(x - position) as usize..(y - position) as usize].copy_from_slice(&bloc[(x - zone) as usize..(y - zone) as usize]);
                }
            }
            let mut longueur = voulu;
            if longueur % 512 != 0 {
                let complet = longueur.div_ceil(512) * 512;
                tampon[longueur..complet].fill(0);
                longueur = complet;
            }
            // Comme Rufus : une écriture ratée est retentée (une clé USB a parfois un raté passager).
            let mut essai = 0;
            loop {
                match disque.seek(SeekFrom::Start(position)).and_then(|_| disque.write_all(&tampon[..longueur])) {
                    Ok(()) => break,
                    Err(e) if essai >= ESSAIS_ECRITURE => return Err(e),
                    Err(_) => {
                        essai += 1;
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            }
            position += voulu as u64;
            avancer(voulu as u64);
        }
    }
    disque.flush()
}

/// Les renseignements de Windows sur le disque de la lettre `lecteur` : numéro, type de bus, disque système ?
#[derive(Debug, PartialEq)]
pub struct Disque {
    pub numero: u32,
    pub bus: String,
    pub systeme: bool,
}

/// Lit « numéro|bus|démarrage|système » (voir `disque_de`).
pub fn lire_disque(sortie: &str) -> Option<Disque> {
    let ligne = sortie.lines().map(str::trim).find(|l| l.contains('|'))?;
    let p: Vec<&str> = ligne.split('|').map(str::trim).collect();
    let [numero, bus, demarrage, systeme] = p.as_slice() else { return None };
    Some(Disque { numero: numero.parse().ok()?, bus: bus.to_string(), systeme: demarrage.eq_ignore_ascii_case("true") || systeme.eq_ignore_ascii_case("true") })
}

/// Les clés USB (et cartes SD) branchées, vues par Windows comme des disques (même illisibles, sans lettre).
pub fn cles_windows() -> Vec<crate::machine::CleUsb> {
    let script = "Get-Disk | Where-Object { @('USB','SD','MMC') -contains $_.BusType.ToString() -and -not $_.IsBoot -and -not $_.IsSystem } | \
         ForEach-Object { $l = (($_ | Get-Partition -ErrorAction SilentlyContinue | Where-Object DriveLetter | ForEach-Object { \"$($_.DriveLetter):\" }) -join ', '); \
         \"$($_.Number)|$($_.Size)|$($_.FriendlyName)|$l\" }";
    let Ok(sortie) = commande("powershell").args(["-NoProfile", "-NonInteractive", "-Command", script]).output() else { return Vec::new() };
    lire_cles(&String::from_utf8_lossy(&sortie.stdout))
}

/// Lit « numéro|taille|nom|lettres » (une clé par ligne).
pub fn lire_cles(sortie: &str) -> Vec<crate::machine::CleUsb> {
    let go = |octets: u64| (octets as f64 / 1_000_000_000.0 * 10.0).round() / 10.0;
    sortie
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.trim().splitn(4, '|').collect();
            let [numero, taille, nom, lettres] = p.as_slice() else { return None };
            let taille: u64 = taille.parse().ok().filter(|t| *t > 0)?;
            Some(crate::machine::CleUsb {
                disque: numero.parse().ok()?,
                lecteur: lettres.trim().to_string(),
                nom: if nom.trim().is_empty() { "Clé USB".into() } else { nom.trim().to_string() },
                total_go: go(taille),
            })
        })
        .collect()
}

fn disque_de(numero: u32) -> Result<Disque, String> {
    let script = format!("$d = Get-Disk -Number {numero} -ErrorAction Stop; \"$($d.Number)|$($d.BusType)|$($d.IsBoot)|$($d.IsSystem)\"");
    let sortie = commande("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).output().map_err(|e| e.to_string())?;
    lire_disque(&String::from_utf8_lossy(&sortie.stdout))
        .filter(|d| d.numero == numero)
        .ok_or_else(|| "Clé introuvable. Débranche-la, rebranche-la, puis réessaie.".to_string())
}

/// Ce que fait la copie administrateur (`bhost.exe --ecrire-cle travail.json`). Renvoie le code de sortie.
pub fn ecrire_depuis_commande(fichier: &Path) -> i32 {
    let travail: Travail = match std::fs::read(fichier).ok().and_then(|o| serde_json::from_slice(&o).ok()) {
        Some(t) => t,
        None => return 2,
    };
    let noter = |texte: &str| {
        let _ = std::fs::write(&travail.progres, texte);
    };
    match ecrire(&travail, &noter) {
        Ok(()) => {
            noter("ok");
            0
        }
        Err(e) => {
            noter(&e);
            1
        }
    }
}

fn ecrire(travail: &Travail, noter: &dyn Fn(&str)) -> Result<(), String> {
    let disque = disque_de(travail.disque)?;
    // Jamais le disque de Windows, ni un disque interne : seulement une clé (USB) ou une carte SD.
    if disque.systeme || !["USB", "SD", "MMC"].iter().any(|b| disque.bus.eq_ignore_ascii_case(b)) {
        return Err("Ce lecteur n’est pas une clé USB : BHOST ne l’efface pas.".into());
    }
    let bloc = travail.wifi.as_deref().map(|w| w.as_bytes().to_vec());
    if let Some(b) = &bloc {
        if b.len() != TAILLE_WIFI {
            return Err("Zone du Wi-Fi invalide.".into());
        }
    }
    noter("0");
    // Effacer la clé : plus de partition, donc plus de volume que Windows protège en écriture.
    let script = std::env::temp_dir().join(format!("bhost-diskpart-{}.txt", std::process::id()));
    std::fs::write(&script, format!("select disk {}\nclean\nrescan\n", disque.numero)).map_err(|e| e.to_string())?;
    let efface = commande("diskpart").arg("/s").arg(&script).output();
    let _ = std::fs::remove_file(&script);
    if !efface.is_ok_and(|s| s.status.success()) {
        return Err("Impossible d’effacer la clé. Ferme les fenêtres qui l’utilisent, puis réessaie.".into());
    }
    let chemin = format!(r"\\.\PhysicalDrive{}", disque.numero);
    let mut cle = std::fs::OpenOptions::new().read(true).write(true).open(&chemin).map_err(|e| format!("Impossible d’ouvrir la clé ({e})."))?;
    let wifi = travail.wifi_position.zip(bloc.as_deref());
    copier(&travail.iso, &mut cle, wifi, |p| noter(&p.to_string())).map_err(|e| format!("L’écriture s’est arrêtée ({e}). Essaie une autre clé."))?;
    // Vider les tampons de Windows : certaines clés ne le permettent pas (l'écriture est déjà faite).
    let _ = cle.sync_all();
    drop(cle);
    // Relire toute la clé, comme balenaEtcher : une clé fatiguée peut « accepter » sans rien garder.
    let mut relue = std::fs::File::open(&chemin).map_err(|e| format!("Impossible de relire la clé ({e})."))?;
    let bonne = verifier(&travail.iso, &mut relue, wifi, |p| noter(&format!("v{p}"))).map_err(|e| format!("Impossible de relire la clé ({e})."))?;
    drop(relue);
    if !bonne {
        return Err("La clé ne garde pas ce qu’on écrit dessus. Essaie une autre clé.".into());
    }
    // Windows relit la clé (ses partitions BHOST OS sont de type Linux : il ne lui donne pas de lettre).
    let script = std::env::temp_dir().join(format!("bhost-diskpart-{}.txt", std::process::id()));
    if std::fs::write(&script, format!("select disk {}\nrescan\n", disque.numero)).is_ok() {
        let _ = commande("diskpart").arg("/s").arg(&script).output();
        let _ = std::fs::remove_file(&script);
    }
    Ok(())
}

// ---------- Côté BHOST (sans droits) ----------

/// Où garder l'ISO téléchargée.
fn iso_telechargee(dossier: &Path) -> PathBuf {
    dossier.join("bhost-os.iso")
}

/// Télécharge la dernière ISO (et vérifie son empreinte). `progres` reçoit le pourcentage.
fn telecharger(agent: &ureq::Agent, dossier: &Path, progres: &dyn Fn(u8)) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    let hors_ligne = "BHOST OS n’est pas encore en ligne. Choisis un fichier ISO.".to_string();
    let attendue = crate::reseau::telecharger(agent, EMPREINTE_URL).ok().flatten().ok_or_else(|| hors_ligne.clone())?;
    let attendue = String::from_utf8_lossy(&attendue).split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
    let destination = iso_telechargee(dossier);
    // Déjà téléchargée et identique : rien à faire.
    if empreinte(&destination).is_ok_and(|e| e == attendue) {
        return Ok(destination);
    }
    let mut reponse = agent
        .get(ISO_URL)
        .config()
        .timeout_global(None)
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_per_read(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .map_err(|_| hors_ligne.clone())?;
    let total: u64 = reponse.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    let provisoire = destination.with_extension("iso.part");
    let mut fichier = File::create(&provisoire).map_err(|e| e.to_string())?;
    let mut lecteur = reponse.body_mut().with_config().limit(2 << 30).reader();
    let mut calcul = Sha256::new();
    let mut tampon = vec![0u8; 1 << 20];
    let mut recu = 0u64;
    loop {
        let n = lecteur.read(&mut tampon).map_err(|_| "Le téléchargement s’est arrêté. Réessaie.".to_string())?;
        if n == 0 {
            break;
        }
        fichier.write_all(&tampon[..n]).map_err(|e| e.to_string())?;
        calcul.update(&tampon[..n]);
        recu += n as u64;
        if total > 0 {
            progres(((recu * 100) / total).min(100) as u8);
        }
    }
    drop(fichier);
    let obtenue: String = calcul.finalize().iter().map(|o| format!("{o:02x}")).collect();
    if obtenue != attendue {
        let _ = std::fs::remove_file(&provisoire);
        return Err("Le téléchargement est abîmé. Réessaie.".into());
    }
    std::fs::rename(&provisoire, &destination).map_err(|e| e.to_string())?;
    Ok(destination)
}

fn empreinte(fichier: &Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = File::open(fichier)?;
    let mut calcul = Sha256::new();
    io::copy(&mut f, &mut calcul)?;
    Ok(calcul.finalize().iter().map(|o| format!("{o:02x}")).collect())
}

/// L'avancement envoyé à l'interface (événement « cle »).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Avancement {
    etape: &'static str, // « telechargement », « autorisation », « ecriture », « verification »
    pct: u8,
}

/// Écrit BHOST OS sur la clé `lecteur`. `iso` : un fichier du PC, sinon la dernière version est téléchargée.
/// Wi-Fi vide : câble réseau (ou choix du Wi-Fi sur la machine).
#[cfg_attr(feature = "bureau", tauri::command)]
pub(crate) async fn ecrire_cle(
    app: crate::hote::AppHandle,
    disque: u32,
    iso: Option<String>,
    wifi_nom: String,
    wifi_motdepasse: String,
    etat: crate::hote::State<'_, crate::Etat>,
) -> Result<(), String> {
    let (dossier, agent) = (etat.dossier.clone(), etat.reseau.clone());
    crate::hote::async_runtime::spawn_blocking(move || ecrire_cle_maintenant(&app, &agent, &dossier, disque, iso, &wifi_nom, &wifi_motdepasse))
        .await
        .map_err(|e| e.to_string())?
}

fn ecrire_cle_maintenant(app: &crate::hote::AppHandle, agent: &ureq::Agent, dossier: &Path, disque: u32, iso: Option<String>, wifi_nom: &str, wifi_motdepasse: &str) -> Result<(), String> {
    use crate::hote::Emitter;
    let dire = |etape: &'static str, pct: u8| {
        let _ = app.emit("cle", Avancement { etape, pct });
    };
    if !cfg!(windows) {
        return Err("Seulement sous Windows pour l’instant.".into());
    }
    let wifi_nom = wifi_nom.trim();
    let bloc = if wifi_nom.is_empty() { None } else { Some(bloc_wifi(wifi_nom, wifi_motdepasse)?) };
    let iso = match iso.filter(|i| !i.trim().is_empty()) {
        Some(i) => PathBuf::from(i),
        None => telecharger(agent, dossier, &|p| dire("telechargement", p))?,
    };
    let position = trouver_zone_wifi(&iso).map_err(|e| format!("ISO illisible ({e})."))?;
    if bloc.is_some() && position.is_none() {
        return Err("Cette ISO est trop ancienne pour le Wi-Fi. Prends la dernière version.".into());
    }
    let n = std::process::id();
    let travail = Travail {
        iso: iso.clone(),
        disque,
        wifi_position: position.filter(|_| bloc.is_some()),
        wifi: bloc.map(|b| String::from_utf8_lossy(&b).into_owned()),
        progres: std::env::temp_dir().join(format!("bhost-cle-{n}.txt")),
    };
    let fichier = std::env::temp_dir().join(format!("bhost-cle-{n}.json"));
    std::fs::write(&fichier, serde_json::to_vec(&travail).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&travail.progres);

    // BHOST se relance en administrateur (Windows affiche sa fenêtre de permission).
    dire("autorisation", 0);
    let moi = std::env::current_exe().map_err(|e| e.to_string())?;
    let echapper = |t: &str| t.replace('\'', "''");
    let script = format!(
        "$p = Start-Process -FilePath '{}' -ArgumentList '{OPTION}','\"{}\"' -Verb RunAs -WindowStyle Hidden -PassThru -ErrorAction Stop; $p.WaitForExit(); exit $p.ExitCode",
        echapper(&moi.to_string_lossy()),
        echapper(&fichier.to_string_lossy())
    );
    let mut enfant = commande("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).spawn().map_err(|e| e.to_string())?;
    let fin = loop {
        if let Some(statut) = enfant.try_wait().map_err(|e| e.to_string())? {
            break statut;
        }
        if let Ok(t) = std::fs::read_to_string(&travail.progres) {
            let t = t.trim();
            if let Ok(p) = t.parse::<u8>() {
                dire("ecriture", p);
            } else if let Some(Ok(p)) = t.strip_prefix('v').map(str::parse::<u8>) {
                dire("verification", p);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    };
    let resultat = std::fs::read_to_string(&travail.progres).unwrap_or_default();
    let _ = std::fs::remove_file(&fichier);
    let _ = std::fs::remove_file(&travail.progres);
    if resultat.trim() == "ok" {
        return Ok(());
    }
    let r = resultat.trim();
    if r.is_empty() || r.trim_start_matches('v').parse::<u8>().is_ok() {
        // Permission refusée (fenêtre de Windows), ou copie arrêtée en route.
        return Err(if fin.success() { "L’écriture ne s’est pas terminée. Réessaie.".into() } else { "Il faut accepter la fenêtre de Windows pour écrire la clé.".into() });
    }
    Err(resultat.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_du_wifi() {
        let b = bloc_wifi("Maison", "motdepasse123").unwrap();
        assert_eq!(b.len(), TAILLE_WIFI);
        assert!(b.starts_with(b"# BHOST-WIFI v1\nnom=Maison\nmotdepasse=motdepasse123\n\n"));
        assert!(bloc_wifi("Mai\nson", "motdepasse123").is_err());
        assert!(bloc_wifi("Maison", "court").is_err());
        assert!(bloc_wifi("Maison", "").is_ok(), "réseau ouvert");
        assert!(bloc_wifi(&"x".repeat(33), "").is_err());
    }

    #[test]
    fn retrouve_la_zone_et_ecrit_la_cle() {
        let d = std::env::temp_dir().join(format!("bhost-cle-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let iso = d.join("x.iso");
        // Le repère à cheval sur deux morceaux de 4 Mo.
        let position = (4u64 << 20) - 5;
        let mut contenu = vec![7u8; (9 << 20) + 2048];
        let vide = bloc_wifi("", "").unwrap(); // « nom= » vide, comme dans l'ISO
        contenu[position as usize..position as usize + TAILLE_WIFI].copy_from_slice(&vide);
        std::fs::write(&iso, &contenu).unwrap();
        assert_eq!(trouver_zone_wifi(&iso).unwrap(), Some(position));

        let bloc = bloc_wifi("Box", "12345678").unwrap();
        let mut cle = std::io::Cursor::new(Vec::new());
        let mut vus = Vec::new();
        copier(&iso, &mut cle, Some((position, &bloc)), |p| vus.push(p)).unwrap();
        let ecrit = cle.into_inner();
        assert_eq!(ecrit.len(), contenu.len());
        assert_eq!(&ecrit[position as usize..position as usize + TAILLE_WIFI], &bloc[..]);
        assert_eq!(&ecrit[..position as usize], &contenu[..position as usize]);
        assert_eq!(*vus.last().unwrap(), 100);

        // Le début (table des partitions) est écrit en tout dernier.
        struct Journal(std::io::Cursor<Vec<u8>>, Vec<u64>);
        impl Write for Journal {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                self.1.push(self.0.position());
                self.0.write(b)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        impl Seek for Journal {
            fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
                self.0.seek(p)
            }
        }
        let mut j = Journal(std::io::Cursor::new(Vec::new()), Vec::new());
        copier(&iso, &mut j, None, |_| {}).unwrap();
        assert_eq!(j.1.first(), Some(&DEBUT_EN_DERNIER));
        assert_eq!(j.1.last(), Some(&0));
        assert_eq!(j.0.into_inner(), contenu);

        // Relecture : bonne avec le même Wi-Fi, mauvaise si un octet a changé.
        let mut relue = std::io::Cursor::new(ecrit.clone());
        assert!(verifier(&iso, &mut relue, Some((position, &bloc)), |_| {}).unwrap());
        let mut abimee = ecrit.clone();
        abimee[9 << 20] ^= 1;
        assert!(!verifier(&iso, &mut std::io::Cursor::new(abimee), Some((position, &bloc)), |_| {}).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn lit_windows() {
        let sortie = "Il existe 1 interface sur le système :\r\n\r\n    Nom                    : Wi-Fi\r\n    BSSID                  : aa:bb\r\n    SSID                   : Livebox-12AB\r\n";
        assert_eq!(ssid_de_netsh(sortie).as_deref(), Some("Livebox-12AB"));
        assert_eq!(ssid_de_netsh("    État : déconnecté\r\n"), None);
        assert_eq!(cle_du_profil("<sharedKey><keyMaterial>a&amp;b&lt;c</keyMaterial></sharedKey>").as_deref(), Some("a&b<c"));
        assert_eq!(cle_du_profil("<sharedKey></sharedKey>"), None);
        assert_eq!(lire_disque("2|USB|False|False\r\n"), Some(Disque { numero: 2, bus: "USB".into(), systeme: false }));
        assert!(lire_disque("0|NVMe|True|True").unwrap().systeme);
        assert_eq!(lire_disque("rien"), None);
        let cles = lire_cles("2|15665725440|SanDisk Cruzer|F:, G:\r\n3|8000000000||\r\n4|0|Lecteur vide|\r\nbizarre\r\n");
        assert_eq!(cles.len(), 2);
        assert_eq!((cles[0].disque, cles[0].lecteur.as_str(), cles[0].nom.as_str(), cles[0].total_go), (2, "F:, G:", "SanDisk Cruzer", 15.7));
        assert_eq!((cles[1].disque, cles[1].lecteur.as_str(), cles[1].nom.as_str()), (3, "", "Clé USB"));
    }
}
