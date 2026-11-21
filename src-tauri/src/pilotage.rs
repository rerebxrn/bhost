//! BHOST OS : la machine reste joignable par le relais, et les PC qui ont son code la pilotent.
//!
//! 1. La machine ouvre une connexion de contrôle : « MACHINE <clé> <code> », signe le défi, puis PING toutes les 20 s.
//! 2. Un PC envoie « PILOTER <code> » au relais. Le relais dit à la machine « PILOTE <n> <jeton> ».
//! 3. La machine ouvre « DONNEES <n> <jeton> » : c'est la session de ce PC. Les commandes y arrivent
//!    en JSON (une ligne chacune, voir agent.rs) ; les réponses et toutes les nouvelles y repartent.
//!
//! Pas de port à ouvrir sur la box : c'est toujours la machine qui appelle le relais.
//! Sur le réseau de la maison (ou sur un VPS), le PC peut aussi venir en direct, sur le port 4446.
//! Dans les deux cas, la session est chiffrée (TLS 1.3, voir chiffre.rs) et commence par le code de la machine.

use crate::hote::AppHandle;
use crate::tunnel::{self, protocole};
use crate::tunnel::protocole::{DeBhost, DuRelais};
use ed25519_dalek::Signer;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PING: Duration = Duration::from_secs(20);
/// En direct : 5 codes faux depuis une adresse, et elle attend 15 minutes (comme sur le relais).
const ESSAIS_MAX: u32 = 5;
const ATTENTE_ESSAIS: Duration = Duration::from_secs(15 * 60);
/// En direct : pas plus de PC à la fois.
const DIRECTS_MAX: usize = 16;
const SILENCE: Duration = Duration::from_secs(70);
/// Une ligne de commande ne dépasse pas ça (un fichier de configuration écrit depuis BHOST, par exemple).
const LIGNE_MAX: u64 = 32 * 1024 * 1024;

/// Les PC qui pilotent la machine en ce moment : chaque nouvelle leur est envoyée à tous.
#[derive(Default)]
pub struct Sessions(Mutex<Vec<Sender<String>>>);

impl Sessions {
    pub fn diffuser(&self, ligne: &str) {
        // Une session fermée est oubliée au passage.
        self.0.lock().unwrap().retain(|s| s.send(ligne.to_string()).is_ok());
    }
    fn ajouter(&self, s: Sender<String>) {
        self.0.lock().unwrap().push(s);
    }
    pub fn nombre(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

/// Les connexions ouvertes avec le relais (contrôle et sessions des PC) : « Changer le code » les coupe toutes.
static PRISES: Mutex<Vec<(u64, TcpStream)>> = Mutex::new(Vec::new());
static NUMERO: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Tant qu'elle existe, la connexion peut être coupée par « Changer le code » ; ensuite, elle est oubliée.
struct Retenue(u64);

impl Drop for Retenue {
    fn drop(&mut self) {
        PRISES.lock().unwrap().retain(|(n, _)| *n != self.0);
    }
}

fn retenir(flux: &TcpStream) -> Retenue {
    let n = NUMERO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(copie) = flux.try_clone() {
        PRISES.lock().unwrap().push((n, copie));
    }
    Retenue(n)
}

/// « Changer le code » (page Machine de BHOST) : nouveau code, puis, une seconde plus tard (le temps de
/// répondre au PC qui l'a demandé), toutes les connexions sont coupées. La machine revient sur le relais
/// avec son nouveau code ; les PC qui n'ont que l'ancien ne peuvent plus la piloter.
pub fn changer_code(dossier: &Path) -> io::Result<String> {
    let code = nouveau_code(dossier)?;
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(1));
        for (_, prise) in PRISES.lock().unwrap().drain(..) {
            let _ = prise.shutdown(std::net::Shutdown::Both);
        }
    });
    Ok(code)
}

/// Le code de la machine (8 caractères), tiré au hasard la première fois et gardé dans machine-code.txt.
pub fn code(dossier: &Path) -> io::Result<String> {
    let fichier = dossier.join("machine-code.txt");
    if let Ok(t) = std::fs::read_to_string(&fichier) {
        let t = t.trim().to_string();
        if protocole::code_valide(&t) {
            return Ok(t);
        }
    }
    nouveau_code(dossier)
}

/// Tire un nouveau code (Régénérer le code, ou code déjà pris par une autre machine).
pub fn nouveau_code(dossier: &Path) -> io::Result<String> {
    let alphabet: Vec<char> = protocole::ALPHABET_CODE.chars().collect();
    let mut hasard = [0u8; 8];
    getrandom::fill(&mut hasard).map_err(|_| io::Error::other("pas de hasard"))?;
    // 32 lettres : un octet % 32 ne favorise aucune lettre.
    let code: String = hasard.iter().map(|o| alphabet[*o as usize % alphabet.len()]).collect();
    std::fs::create_dir_all(dossier)?;
    crate::garde::ecrire_atomique(&dossier.join("machine-code.txt"), code.as_bytes()).map_err(io::Error::other)?;
    Ok(code)
}

/// L'id de la machine (les 16 premiers caractères de sa clé publique), comme le relais le calcule.
pub fn id(dossier: &Path) -> io::Result<String> {
    let cle = tunnel::cle(dossier)?;
    Ok(protocole::id_machine(&protocole::hexa(cle.verifying_key().as_bytes())))
}

/// Ce que l'écran de la machine montre : « En ligne » ou non.
#[derive(Default)]
pub struct EtatRelais {
    pub en_ligne: std::sync::atomic::AtomicBool,
}

/// Reste en ligne pour toujours (reconnexion après une coupure, de plus en plus espacée jusqu'à 1 min).
pub fn servir(dossier: PathBuf, app: AppHandle, sessions: Arc<Sessions>, rt: tokio::runtime::Handle, etat: Arc<EtatRelais>) {
    std::thread::spawn(move || {
        let mut attente = Duration::from_secs(2);
        loop {
            let debut = std::time::Instant::now();
            let resultat = une_connexion(&dossier, &app, &sessions, &rt, &etat);
            etat.en_ligne.store(false, std::sync::atomic::Ordering::Relaxed);
            if let Err(e) = resultat {
                eprintln!("[bhost-agent] relais : {e}");
            }
            // Restée longtemps en ligne : la coupure est nouvelle, on réessaie vite.
            if debut.elapsed() > Duration::from_secs(120) {
                attente = Duration::from_secs(2);
            }
            std::thread::sleep(attente);
            attente = (attente * 2).min(Duration::from_secs(60));
        }
    });
}

fn lire_ligne(lecteur: &mut impl BufRead) -> io::Result<String> {
    let mut ligne = String::new();
    if lecteur.take(300).read_line(&mut ligne)? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(ligne)
}

fn une_connexion(dossier: &Path, app: &AppHandle, sessions: &Arc<Sessions>, rt: &tokio::runtime::Handle, etat: &EtatRelais) -> io::Result<()> {
    let cle = tunnel::cle(dossier)?;
    let code = code(dossier)?;
    let adresse = tunnel::adresse_relais()?;
    let mut flux = tunnel::connecter(adresse)?;
    let _retenue = retenir(&flux);
    flux.set_read_timeout(Some(Duration::from_secs(10)))?;
    let bonjour = protocole::ecrire_de_bhost(&DeBhost::Machine { cle: protocole::hexa(cle.verifying_key().as_bytes()), code: code.clone() });
    flux.write_all(format!("BHOST {}\n{bonjour}", protocole::VERSION).as_bytes())?;
    let mut lecteur = BufReader::new(flux.try_clone()?);
    let Some(DuRelais::Defi { defi }) = protocole::lire_du_relais(&lire_ligne(&mut lecteur)?) else {
        return Err(io::Error::other("pas de défi"));
    };
    let signature = protocole::hexa(&cle.sign(protocole::a_signer_machine(&code, &defi).as_bytes()).to_bytes());
    flux.write_all(protocole::ecrire_de_bhost(&DeBhost::Signe { signature }).as_bytes())?;
    match protocole::lire_du_relais(&lire_ligne(&mut lecteur)?) {
        Some(DuRelais::Ok { .. }) => {}
        // Une autre machine a déjà ce code (très rare) : on en tire un autre.
        Some(DuRelais::Non { raison }) if raison == "pris" => {
            nouveau_code(dossier)?;
            return Err(io::Error::other("code déjà pris : nouveau code"));
        }
        autre => return Err(io::Error::other(format!("refusée : {autre:?}"))),
    }
    etat.en_ligne.store(true, std::sync::atomic::Ordering::Relaxed);
    eprintln!("[bhost-agent] en ligne sur le relais");

    // PING toutes les 20 s, par un fil à part (il s'arrête quand la connexion tombe).
    let mut ping = flux.try_clone()?;
    std::thread::spawn(move || {
        let ligne = protocole::ecrire_de_bhost(&DeBhost::Ping);
        loop {
            std::thread::sleep(PING);
            if ping.write_all(ligne.as_bytes()).is_err() {
                return;
            }
        }
    });
    // Le délai se règle sur la prise qui lit (sous Windows, une copie a ses propres réglages).
    lecteur.get_ref().set_read_timeout(Some(SILENCE))?;
    loop {
        let ligne = lire_ligne(&mut lecteur)?;
        match protocole::lire_du_relais(&ligne) {
            Some(DuRelais::Pong) => {}
            Some(DuRelais::Pilote { id, jeton }) => {
                let (app, sessions, rt, dossier) = (app.clone(), sessions.clone(), rt.clone(), dossier.to_path_buf());
                std::thread::spawn(move || {
                    if let Err(e) = session(id, &jeton, &dossier, app, sessions, rt) {
                        eprintln!("[bhost-agent] session : {e}");
                    }
                });
            }
            _ => return Err(io::Error::other("ligne inattendue du relais")),
        }
    }
}

/// La configuration TLS de la machine (faite une fois, avec sa clé).
fn config_tls(dossier: &Path) -> io::Result<Arc<rustls::ServerConfig>> {
    static CONFIG: Mutex<Option<Arc<rustls::ServerConfig>>> = Mutex::new(None);
    let mut c = CONFIG.lock().unwrap();
    if let Some(c) = c.as_ref() {
        return Ok(c.clone());
    }
    let cle = tunnel::cle(dossier)?;
    let config = crate::chiffre::config_machine(&cle.to_bytes()).map_err(io::Error::other)?;
    *c = Some(config.clone());
    Ok(config)
}

/// Les codes faux venus en direct, par adresse (un bloc /64 en IPv6).
static ESSAIS: std::sync::LazyLock<Mutex<std::collections::HashMap<std::net::IpAddr, (std::time::Instant, u32)>>> = std::sync::LazyLock::new(Default::default);

fn bloc(ip: std::net::IpAddr) -> std::net::IpAddr {
    match ip {
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            std::net::IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

fn bloquee(ip: std::net::IpAddr) -> bool {
    let mut essais = ESSAIS.lock().unwrap();
    essais.retain(|_, (depuis, _)| depuis.elapsed() < ATTENTE_ESSAIS);
    essais.get(&bloc(ip)).is_some_and(|(_, n)| *n >= ESSAIS_MAX)
}

fn noter_code_faux(ip: std::net::IpAddr) {
    let mut essais = ESSAIS.lock().unwrap();
    let (depuis, n) = essais.entry(bloc(ip)).or_insert((std::time::Instant::now(), 0));
    *n += 1;
    *depuis = std::time::Instant::now();
}

/// Compare deux codes sans que le temps de réponse dise où ils diffèrent.
fn meme_code(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// La session d'un PC venu par le relais : la machine ouvre « DONNEES », puis le PC chiffre la connexion.
fn session(n: u64, jeton: &str, dossier: &Path, app: AppHandle, sessions: Arc<Sessions>, rt: tokio::runtime::Handle) -> io::Result<()> {
    let mut flux = tunnel::connecter(tunnel::adresse_relais()?)?;
    let _retenue = retenir(&flux);
    let donnees = protocole::ecrire_de_bhost(&DeBhost::Donnees { id: n, jeton: jeton.to_string() });
    flux.write_all(format!("BHOST {}\n{donnees}", protocole::VERSION).as_bytes())?;
    flux.set_read_timeout(Some(Duration::from_secs(20)))?;
    let chiffree = crate::chiffre::accepter(flux, config_tls(dossier)?)?;
    servir_pc(chiffree, dossier, None, app, sessions, rt)
}

/// En direct (réseau de la maison, ou VPS) : la machine attend les PC sur le port 4446.
pub fn ecouter_direct(dossier: PathBuf, app: AppHandle, sessions: Arc<Sessions>, rt: tokio::runtime::Handle) {
    // Essais : deux agents sur le même ordinateur (BHOST_PORT_DIRECT=0 : pas d'écoute).
    let port = crate::chiffre::port_direct();
    if port == 0 {
        return;
    }
    std::thread::spawn(move || {
        let ecoute = loop {
            match std::net::TcpListener::bind(("0.0.0.0", port)) {
                Ok(e) => break e,
                Err(e) => {
                    eprintln!("[bhost-agent] port {port} : {e}");
                    std::thread::sleep(Duration::from_secs(30));
                }
            }
        };
        let ouvertes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for prise in ecoute.incoming().flatten() {
            let Ok(ip) = prise.peer_addr().map(|a| a.ip()) else { continue };
            if bloquee(ip) || ouvertes.load(std::sync::atomic::Ordering::Relaxed) >= DIRECTS_MAX {
                continue;
            }
            ouvertes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let (dossier, app, sessions, rt, ouvertes) = (dossier.clone(), app.clone(), sessions.clone(), rt.clone(), ouvertes.clone());
            std::thread::spawn(move || {
                let _retenue = retenir(&prise);
                let resultat = prise
                    .set_read_timeout(Some(Duration::from_secs(15)))
                    .and_then(|_| config_tls(&dossier))
                    .and_then(|config| crate::chiffre::accepter(prise, config))
                    .and_then(|c| servir_pc(c, &dossier, Some(ip), app, sessions, rt));
                if let Err(e) = resultat {
                    eprintln!("[bhost-agent] direct ({ip}) : {e}");
                }
                ouvertes.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            });
        }
    });
}

/// La session d'un PC, une fois chiffrée : d'abord son code, puis ses commandes ; les réponses et les nouvelles repartent.
/// `ip` : venu en direct (les codes faux comptent) ; `None` : par le relais (qui a déjà vérifié le code).
fn servir_pc(c: crate::chiffre::Chiffree, dossier: &Path, ip: Option<std::net::IpAddr>, app: AppHandle, sessions: Arc<Sessions>, rt: tokio::runtime::Handle) -> io::Result<()> {
    let crate::chiffre::Chiffree { canal, lecteur } = c;
    let mut lecteur = BufReader::new(lecteur);
    let ligne = lire_ligne(&mut lecteur)?;
    let donne = ligne.trim().strip_prefix("CODE ").unwrap_or_default().to_string();
    if ip.is_some_and(bloquee) {
        canal.ecrire_ligne("NON bloque")?;
        return Ok(());
    }
    if !meme_code(&donne, &code(dossier)?) {
        if let Some(ip) = ip {
            noter_code_faux(ip);
        }
        canal.ecrire_ligne("NON code")?;
        // Laisse partir la réponse avant de couper.
        std::thread::sleep(Duration::from_millis(200));
        return Ok(());
    }
    canal.ecrire_ligne(&format!("OK {}", id(dossier)?))?;
    // Les commandes peuvent se suivre de loin (le PC reste branché) : plus de délai de lecture.
    lecteur.get_ref().delai(None)?;

    // Un seul fil écrit : réponses et nouvelles passent par ce canal.
    let (envoi, reception) = mpsc::channel::<String>();
    let ecriture = canal.clone();
    std::thread::spawn(move || {
        for ligne in reception {
            if ecriture.ecrire_ligne(&ligne).is_err() {
                break;
            }
        }
        ecriture.fermer();
    });
    sessions.ajouter(envoi.clone());

    loop {
        let mut ligne = String::new();
        if (&mut lecteur).take(LIGNE_MAX).read_line(&mut ligne)? == 0 {
            canal.fermer();
            return Ok(());
        }
        if ligne.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<crate::agent::Demande>(&ligne) {
            Ok(demande) => {
                let (app, envoi) = (app.clone(), envoi.clone());
                // Chaque commande à part : une création de serveur ne bloque pas les autres.
                rt.spawn(async move {
                    let _ = envoi.send(crate::agent::repondre(&app, demande).await);
                });
            }
            Err(_) => {
                canal.fermer();
                return Err(io::Error::other("ligne illisible"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_garde_et_valide() {
        let d = std::env::temp_dir().join(format!("bhost-pilotage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let a = code(&d).unwrap();
        assert!(protocole::code_valide(&a));
        assert_eq!(code(&d).unwrap(), a, "le même code à chaque démarrage");
        let b = nouveau_code(&d).unwrap();
        assert!(protocole::code_valide(&b));
        assert_eq!(code(&d).unwrap(), b);
        assert_eq!(id(&d).unwrap().len(), 16);
        let _ = std::fs::remove_dir_all(&d);
    }
}
