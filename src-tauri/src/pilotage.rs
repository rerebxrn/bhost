//! BHOST OS : la machine reste joignable par le relais, et les PC qui ont son code la pilotent.
//!
//! 1. La machine ouvre une connexion de contrôle : « MACHINE <clé> <code> », signe le défi, puis PING toutes les 20 s.
//! 2. Un PC envoie « PILOTER <code> » au relais. Le relais dit à la machine « PILOTE <n> <jeton> ».
//! 3. La machine ouvre « DONNEES <n> <jeton> » : c'est la session de ce PC. Les commandes y arrivent
//!    en JSON (une ligne chacune, voir agent.rs) ; les réponses et toutes les nouvelles y repartent.
//!
//! Pas de port à ouvrir sur la box : c'est toujours la machine qui appelle le relais.

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
                let (app, sessions, rt) = (app.clone(), sessions.clone(), rt.clone());
                std::thread::spawn(move || {
                    if let Err(e) = session(id, &jeton, app, sessions, rt) {
                        eprintln!("[bhost-agent] session : {e}");
                    }
                });
            }
            _ => return Err(io::Error::other("ligne inattendue du relais")),
        }
    }
}

/// La session d'un PC : ses commandes arrivent ici, les réponses et les nouvelles repartent.
fn session(n: u64, jeton: &str, app: AppHandle, sessions: Arc<Sessions>, rt: tokio::runtime::Handle) -> io::Result<()> {
    let mut flux = tunnel::connecter(tunnel::adresse_relais()?)?;
    let donnees = protocole::ecrire_de_bhost(&DeBhost::Donnees { id: n, jeton: jeton.to_string() });
    flux.write_all(format!("BHOST {}\n{donnees}", protocole::VERSION).as_bytes())?;

    // Un seul fil écrit sur la connexion : réponses et nouvelles passent par ce canal.
    let (envoi, reception) = mpsc::channel::<String>();
    let mut ecriture = flux.try_clone()?;
    std::thread::spawn(move || {
        for ligne in reception {
            if ecriture.write_all(ligne.as_bytes()).and_then(|_| ecriture.write_all(b"\n")).is_err() {
                break;
            }
        }
        let _ = ecriture.shutdown(std::net::Shutdown::Both);
    });
    sessions.ajouter(envoi.clone());

    let mut lecteur = BufReader::new(flux);
    loop {
        let mut ligne = String::new();
        if (&mut lecteur).take(LIGNE_MAX).read_line(&mut ligne)? == 0 {
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
            Err(_) => return Err(io::Error::other("ligne illisible")),
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
