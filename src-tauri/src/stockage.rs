// Machine dédiée : le stockage max d'un serveur (Créer un serveur › Stockage max, Paramètres).
// BHOST mesure la place prise (le dossier du serveur et ses sauvegardes) toutes les 2 minutes.
// Au-delà du max : les sauvegardes automatiques les plus anciennes partent (la plus récente reste).
// Si ça ne suffit pas : le serveur s'arrête proprement, et ne redémarre pas tant que la place manque.

use crate::hote::{AppHandle, Manager};
use crate::serveurs::Serveur;
use crate::{problemes, sauvegarde, Etat};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// La dernière mesure de chaque serveur, en Mo.
static TAILLES: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(Default::default);

const ENTRE_DEUX: Duration = Duration::from_secs(120);

/// La place prise par un dossier, en octets (les liens ne sont pas suivis).
pub fn taille(dossier: &Path) -> u64 {
    let Ok(entrees) = std::fs::read_dir(dossier) else { return 0 };
    entrees
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => taille(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn mo(octets: u64) -> u64 {
    octets.div_ceil(1024 * 1024)
}

/// La place prise par le serveur (son dossier et ses sauvegardes), en Mo. Notée pour l'interface.
pub fn mesurer(etat: &Etat, serveur: &Serveur) -> u64 {
    let pris = mo(taille(Path::new(&serveur.dossier)) + taille(&crate::dossier_sauvegardes(etat, serveur)));
    TAILLES.lock().unwrap().insert(serveur.id.clone(), pris);
    pris
}

/// La dernière mesure, si BHOST en a une.
pub fn connue(id: &str) -> Option<u64> {
    TAILLES.lock().unwrap().get(id).copied()
}

pub fn oublier(id: &str) {
    TAILLES.lock().unwrap().remove(id);
}

/// Au-delà du max : les sauvegardes automatiques en trop partent (la plus récente reste).
/// Renvoie la place prise ensuite, en Mo.
pub fn faire_de_la_place(etat: &Etat, serveur: &Serveur, max: u64) -> u64 {
    let pris = mesurer(etat, serveur);
    if pris < max {
        return pris;
    }
    let rangement = crate::dossier_sauvegardes(etat, serveur);
    let en_trop = sauvegarde::en_trop(&sauvegarde::lister(&rangement, &serveur.epinglees), 1);
    if en_trop.is_empty() {
        return pris;
    }
    for fichier in en_trop {
        let _ = std::fs::remove_file(rangement.join(fichier));
    }
    mesurer(etat, serveur)
}

/// Refus de démarrer ou d'importer : la phrase.
pub fn phrase_plein(max: u64) -> String {
    format!("Stockage max atteint ({} Go). Libère de la place ou monte-le dans Paramètres.", go(max))
}

/// « 20 », « 2,5 ».
fn go(mo: u64) -> String {
    let g = (mo as f64 / 1024.0 * 10.0).round() / 10.0;
    if g.fract() == 0.0 { format!("{g}") } else { format!("{g}").replace('.', ",") }
}

/// Avant de démarrer : la place manque-t-elle ?
pub fn verifier_demarrage(etat: &Etat, serveur: &Serveur) -> Result<(), String> {
    match serveur.disque_max_mo {
        Some(max) if faire_de_la_place(etat, serveur, max) >= max => Err(phrase_plein(max)),
        _ => Ok(()),
    }
}

/// Avant d'importer `ajout` octets : la place manque-t-elle ?
#[cfg_attr(feature = "bureau", allow(dead_code))]
pub fn verifier_ajout(etat: &Etat, serveur: &Serveur, ajout: u64) -> Result<(), String> {
    match serveur.disque_max_mo {
        Some(max) if faire_de_la_place(etat, serveur, max) + mo(ajout) > max => Err(phrase_plein(max)),
        _ => Ok(()),
    }
}

/// Le fil qui mesure : un serveur allumé qui dépasse son max s'arrête proprement, avec une carte.
pub fn surveiller(app: AppHandle) {
    std::thread::spawn(move || loop {
        let etat = app.state::<Etat>();
        let limites: Vec<Serveur> = etat.liste.lock().unwrap().iter().filter(|s| s.disque_max_mo.is_some()).cloned().collect();
        for serveur in limites {
            let max = serveur.disque_max_mo.unwrap_or(u64::MAX);
            let allume = etat.vivants.lock().unwrap().contains_key(&serveur.id);
            if faire_de_la_place(&etat, &serveur, max) >= max && allume {
                problemes::stockage(&app, &serveur.id, max);
                let _ = crate::arreter(&app, &serveur.id);
            }
        }
        std::thread::sleep(ENTRE_DEUX);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesure_un_dossier() {
        let d = std::env::temp_dir().join(format!("bhost-stockage-{}", std::process::id()));
        std::fs::create_dir_all(d.join("world/region")).unwrap();
        std::fs::write(d.join("server.jar"), vec![0u8; 3000]).unwrap();
        std::fs::write(d.join("world/region/r.0.0.mca"), vec![0u8; 5000]).unwrap();
        assert_eq!(taille(&d), 8000);
        assert_eq!(mo(8000), 1);
        assert_eq!(mo(0), 0);
        assert_eq!(taille(&d.join("absent")), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn phrase_en_go() {
        assert_eq!(phrase_plein(20 * 1024), "Stockage max atteint (20 Go). Libère de la place ou monte-le dans Paramètres.");
        assert_eq!(go(2560), "2,5");
    }
}
