// Copie d'un dossier de serveur (clic droit › Dupliquer).
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use std::fs;
use std::path::Path;

/// Ce qu'on ne copie pas : les journaux, et les verrous des mondes (le serveur les recrée).
fn a_sauter(nom: &str) -> bool {
    nom.eq_ignore_ascii_case("logs") || nom.eq_ignore_ascii_case("session.lock")
}

/// Le poids total des fichiers à copier, en octets.
fn poids(dossier: &Path, profondeur: usize) -> u64 {
    if profondeur > crate::garde::PROFONDEUR_MAX {
        return 0;
    }
    let Ok(entrees) = fs::read_dir(dossier) else { return 0 };
    entrees
        .flatten()
        .filter(|e| !a_sauter(&e.file_name().to_string_lossy()))
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => poids(&e.path(), profondeur + 1),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0, // liens : ignorés
        })
        .sum()
}

/// Copie le contenu de `source` dans `cible` (qui existe déjà et est vide).
/// `avance` reçoit le pourcentage copié (0 à 100).
pub fn copier(source: &Path, cible: &Path, avance: &mut dyn FnMut(u8)) -> Result<(), String> {
    let total = poids(source, 0).max(1);
    let mut fait = 0u64;
    let mut dernier = u8::MAX;
    copier_dans(source, cible, 0, &mut |octets| {
        fait += octets;
        let p = (fait * 100 / total).min(100) as u8;
        if p != dernier {
            dernier = p;
            avance(p);
        }
    })
}

fn copier_dans(source: &Path, cible: &Path, profondeur: usize, compte: &mut dyn FnMut(u64)) -> Result<(), String> {
    let erreur = |e: std::io::Error| format!("Copie impossible ({e}).");
    if profondeur > crate::garde::PROFONDEUR_MAX {
        return Err("Copie impossible : dossiers trop profonds.".into());
    }
    for entree in fs::read_dir(source).map_err(erreur)? {
        let entree = entree.map_err(erreur)?;
        let nom = entree.file_name();
        if a_sauter(&nom.to_string_lossy()) {
            continue;
        }
        let (de, vers) = (entree.path(), cible.join(&nom));
        let genre = entree.file_type().map_err(erreur)?;
        if genre.is_dir() {
            fs::create_dir(&vers).map_err(erreur)?;
            copier_dans(&de, &vers, profondeur + 1, compte)?;
        } else if genre.is_file() {
            compte(fs::copy(&de, &vers).map_err(erreur)?);
        }
    }
    Ok(())
}

/// Le nom de la copie : « Copie de Survie » (40 lettres au plus).
pub fn nom_de_copie(nom: &str) -> String {
    format!("Copie de {nom}").chars().take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copie_tout_sauf_les_journaux_et_les_verrous() {
        let racine = std::env::temp_dir().join(format!("bhost-test-copie-{}", std::process::id()));
        let _ = fs::remove_dir_all(&racine);
        let (a, b) = (racine.join("a"), racine.join("b"));
        fs::create_dir_all(a.join("world/region")).unwrap();
        fs::create_dir_all(a.join("logs")).unwrap();
        fs::write(a.join("server.jar"), vec![0u8; 300]).unwrap();
        fs::write(a.join("world/region/r.0.0.mca"), vec![1u8; 700]).unwrap();
        fs::write(a.join("world/session.lock"), b"x").unwrap();
        fs::write(a.join("logs/latest.log"), b"log").unwrap();
        fs::create_dir(&b).unwrap();
        let mut vus = Vec::new();
        copier(&a, &b, &mut |p| vus.push(p)).unwrap();
        assert_eq!(fs::read(b.join("world/region/r.0.0.mca")).unwrap().len(), 700);
        assert!(b.join("server.jar").is_file());
        assert!(!b.join("logs").exists() && !b.join("world/session.lock").exists());
        assert_eq!(vus.last(), Some(&100));
        assert_eq!(nom_de_copie("Survie"), "Copie de Survie");
        assert_eq!(nom_de_copie(&"x".repeat(40)).chars().count(), 40);
        let _ = fs::remove_dir_all(&racine);
    }
}
