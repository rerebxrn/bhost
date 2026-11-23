//! Prouver qu'on connaît le code d'une machine dédiée, sans jamais le faire voyager.
//!
//! - Le relais ne voit qu'une **empreinte** du code (scrypt) : elle sert à retrouver la machine,
//!   mais elle est trop lente à calculer pour retrouver le code en essayant tous les codes.
//! - Dans le tunnel chiffré (TLS, chiffre.rs), le PC et la machine font un échange SPAKE2 avec le code :
//!   chacun obtient la même clé seulement si les deux codes sont les mêmes. Puis chacun prouve qu'il a la clé
//!   (HMAC, attaché à cette session TLS). Quelqu'un qui écoute, ou un faux relais, n'apprend rien du code ;
//!   un faux interlocuteur ne peut essayer qu'un code par connexion (et les essais sont comptés).
//!
//! Dans le tunnel :
//!   PC      → « PAKE <message> »
//!   machine → « PAKE <message> <preuve> »   (ou « NON bloque »)
//!   PC      → « CONFIRME <preuve> »
//!   machine → « OK <id> »                  (ou « NON code »)

use crate::chiffre::Canal;
use crate::tunnel::protocole;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::collections::HashMap;
use std::io::{self, BufRead, Read};
use std::sync::Mutex;

const PC: &[u8] = b"bhost-pc";
const MACHINE: &[u8] = b"bhost-machine";

/// L'empreinte du code que voit le relais (64 caractères hexadécimaux). Gardée en mémoire : le calcul est lent exprès.
pub fn empreinte_code(code: &str) -> String {
    static DEJA: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
    if let Some(e) = DEJA.lock().unwrap().get_or_insert_with(HashMap::new).get(code) {
        return e.clone();
    }
    // 16 Mo et environ 50 ms par calcul : rien pour BHOST, des siècles pour essayer les 10¹² codes.
    let reglages = scrypt::Params::new(14, 8, 1, 32).expect("réglages scrypt");
    let mut sortie = [0u8; 32];
    scrypt::scrypt(code.as_bytes(), b"bhost-relais-code-v2", &reglages, &mut sortie).expect("scrypt");
    let e = protocole::hexa(&sortie);
    DEJA.lock().unwrap().get_or_insert_with(HashMap::new).insert(code.to_string(), e.clone());
    e
}

/// La preuve d'avoir la clé, pour `qui`, attachée à cette session TLS.
fn preuve(cle: &[u8], qui: &str, session: &[u8; 32]) -> [u8; 32] {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(cle).expect("clé HMAC");
    m.update(b"bhost-appairage-v1|");
    m.update(qui.as_bytes());
    m.update(b"|");
    m.update(session);
    m.finalize().into_bytes().into()
}

fn egal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn lire_ligne(lecteur: &mut impl BufRead) -> io::Result<String> {
    let mut ligne = String::new();
    if lecteur.by_ref().take(300).read_line(&mut ligne)? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(ligne.trim().to_string())
}

/// Pourquoi le PC n'a pas été accepté.
#[derive(Debug, PartialEq)]
pub enum Refus {
    /// Pas le même code (ou pas la vraie machine).
    Code,
    /// Trop de codes faux : attendre.
    Bloque,
    /// La connexion est tombée, ou la machine a mal répondu.
    Muette,
}

/// Côté PC : prouve le code à la machine `id` et vérifie qu'elle le connaît aussi.
pub fn cote_pc(canal: &Canal, lecteur: &mut impl BufRead, code: &str, id: &str) -> Result<(), Refus> {
    let session = canal.empreinte_session().map_err(|_| Refus::Muette)?;
    let (etat, message) = Spake2::<Ed25519Group>::start_a(&Password::new(code.as_bytes()), &Identity::new(PC), &Identity::new(MACHINE));
    canal.ecrire_ligne(&format!("PAKE {}", protocole::hexa(&message))).map_err(|_| Refus::Muette)?;
    let ligne = lire_ligne(lecteur).map_err(|_| Refus::Muette)?;
    let morceaux: Vec<&str> = ligne.split(' ').collect();
    let (leur_message, leur_preuve) = match morceaux[..] {
        ["PAKE", m, p] => (protocole::depuis_hexa(m).ok_or(Refus::Muette)?, protocole::depuis_hexa(p).ok_or(Refus::Muette)?),
        ["NON", "bloque"] => return Err(Refus::Bloque),
        ["NON", "code"] => return Err(Refus::Code),
        _ => return Err(Refus::Muette),
    };
    let cle = etat.finish(&leur_message).map_err(|_| Refus::Code)?;
    if !egal(&leur_preuve, &preuve(&cle, "machine", &session)) {
        return Err(Refus::Code);
    }
    canal.ecrire_ligne(&format!("CONFIRME {}", protocole::hexa(&preuve(&cle, "pc", &session)))).map_err(|_| Refus::Muette)?;
    match lire_ligne(lecteur).map_err(|_| Refus::Muette)?.split(' ').collect::<Vec<_>>()[..] {
        ["OK", vu] if vu == id => Ok(()),
        ["NON", "code"] => Err(Refus::Code),
        ["NON", "bloque"] => Err(Refus::Bloque),
        _ => Err(Refus::Muette),
    }
}

/// Côté machine : vérifie que le PC connaît le code. `bloque` : cette adresse a trop essayé.
/// Rend `true` si le PC est accepté (« OK <id> » est déjà parti).
#[cfg_attr(feature = "bureau", allow(dead_code))]
pub fn cote_machine(canal: &Canal, lecteur: &mut impl BufRead, code: &str, id: &str, bloque: bool) -> io::Result<bool> {
    let session = canal.empreinte_session()?;
    let ligne = lire_ligne(lecteur)?;
    let Some(leur_message) = ligne.strip_prefix("PAKE ").and_then(protocole::depuis_hexa) else {
        canal.ecrire_ligne("NON code")?;
        return Ok(false);
    };
    if bloque {
        canal.ecrire_ligne("NON bloque")?;
        return Ok(false);
    }
    let (etat, message) = Spake2::<Ed25519Group>::start_b(&Password::new(code.as_bytes()), &Identity::new(PC), &Identity::new(MACHINE));
    let Ok(cle) = etat.finish(&leur_message) else {
        canal.ecrire_ligne("NON code")?;
        return Ok(false);
    };
    canal.ecrire_ligne(&format!("PAKE {} {}", protocole::hexa(&message), protocole::hexa(&preuve(&cle, "machine", &session))))?;
    let ligne = lire_ligne(lecteur)?;
    let bonne = ligne.strip_prefix("CONFIRME ").and_then(protocole::depuis_hexa).is_some_and(|p| egal(&p, &preuve(&cle, "pc", &session)));
    if bonne {
        canal.ecrire_ligne(&format!("OK {id}"))?;
    } else {
        canal.ecrire_ligne("NON code")?;
    }
    Ok(bonne)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::BufReader;
    use std::net::TcpListener;

    /// Une vraie machine (TLS + preuve du code) et un PC qui donne `code_pc`.
    fn essai(code_machine: &'static str, code_pc: &str) -> Result<(), Refus> {
        let secret = [7u8; 32];
        let publique = ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key().to_bytes();
        let id = protocole::id_machine(&protocole::hexa(&publique));
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = ecoute.local_addr().unwrap();
        let config = crate::chiffre::config_machine(&secret).unwrap();
        let id_m = id.clone();
        std::thread::spawn(move || {
            let prise = ecoute.incoming().next().unwrap().unwrap();
            let c = crate::chiffre::accepter(prise, config).unwrap();
            let mut l = BufReader::new(c.lecteur);
            let _ = cote_machine(&c.canal, &mut l, code_machine, &id_m, false);
        });
        let prise = std::net::TcpStream::connect(adresse).unwrap();
        let (c, _) = crate::chiffre::connecter(prise, &id, None).unwrap();
        let mut l = BufReader::new(c.lecteur);
        cote_pc(&c.canal, &mut l, code_pc, &id)
    }

    #[test]
    fn bon_code_accepte_mauvais_refuse() {
        assert_eq!(essai("K7M2Q9XR", "K7M2Q9XR"), Ok(()));
        assert_eq!(essai("K7M2Q9XR", "K7M2Q9XS"), Err(Refus::Code));
    }

    #[test]
    fn empreinte_stable_et_differente() {
        let a = empreinte_code("K7M2Q9XR");
        assert_eq!(a.len(), 64);
        assert_eq!(a, empreinte_code("K7M2Q9XR"));
        assert_ne!(a, empreinte_code("K7M2Q9XS"));
    }
}
