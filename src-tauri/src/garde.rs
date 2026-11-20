// Garde-fous partagés : archives .zip venues d'ailleurs, liens et jonctions, dossiers trop profonds.
// Une archive ou un dossier piégé ne doit ni remplir le disque, ni faire planter BHOST, ni écrire hors du serveur.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Au-delà, une archive est refusée : trop d'éléments.
pub const ENTREES_MAX: usize = 500_000;
/// Au-delà, un dossier est trop profond pour être parcouru (une archive normale reste sous 20 niveaux).
pub const PROFONDEUR_MAX: usize = 64;
/// On laisse toujours au moins 2 Go libres sur le disque pendant une extraction.
const MARGE_DISQUE: u64 = 2 * 1024 * 1024 * 1024;

/// La place libre sur le disque qui contient `chemin` (None si on ne sait pas).
fn place_libre(chemin: &Path) -> Option<u64> {
    let vrai = chemin.ancestors().find_map(|p| p.canonicalize().ok())?;
    let disques = sysinfo::Disks::new_with_refreshed_list().ok()?;
    disques
        .iter()
        .filter(|d| vrai.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// Combien d'octets une extraction vers `destination` peut écrire au plus.
pub fn budget_extraction(destination: &Path) -> u64 {
    place_libre(destination).map(|libre| libre.saturating_sub(MARGE_DISQUE)).unwrap_or(u64::MAX)
}

/// Vérifie le nombre d'éléments d'une archive.
pub fn verifier_archive<R: Read + std::io::Seek>(zip: &zip::ZipArchive<R>) -> Result<(), String> {
    if zip.len() > ENTREES_MAX {
        return Err("Archive trop grosse : trop de fichiers.".into());
    }
    Ok(())
}

/// Vérifie un chemin lu dans une archive (déjà passé par `enclosed_name`).
pub fn verifier_chemin(relatif: &Path) -> Result<(), String> {
    if relatif.components().count() > PROFONDEUR_MAX {
        return Err("Archive suspecte : dossiers trop profonds.".into());
    }
    Ok(())
}

/// Copie une entrée d'archive vers un fichier sans dépasser le `budget` restant (diminué de ce qui est écrit).
pub fn copier_limite(entree: &mut dyn Read, sortie: &mut dyn Write, budget: &mut u64) -> Result<(), String> {
    let n = std::io::copy(&mut entree.take(budget.saturating_add(1)), sortie).map_err(|e| format!("Archive abîmée ({e})."))?;
    if n > *budget {
        return Err("Archive trop grosse une fois ouverte : pas assez de place sur le disque.".into());
    }
    *budget -= n;
    Ok(())
}

/// Vrai si `chemin` est un lien symbolique ou une jonction.
/// (Sous Windows, Rust compte aussi les jonctions comme des liens, mais pas les fichiers OneDrive.)
pub fn est_un_lien(chemin: &Path) -> bool {
    fs::symlink_metadata(chemin).is_ok_and(|m| m.file_type().is_symlink())
}

/// Refuse d'écrire à travers un lien ou une jonction.
pub fn pas_un_lien(chemin: &Path) -> Result<(), String> {
    if est_un_lien(chemin) { Err(format!("Lien refusé : {}.", chemin.file_name().unwrap_or_default().to_string_lossy())) } else { Ok(()) }
}

/// Vérifie que `chemin` (ou son dossier parent, s'il n'existe pas encore) est vraiment dans `racine`,
/// liens et jonctions résolus.
pub fn dans(racine: &Path, chemin: &Path) -> Result<(), String> {
    let Ok(racine) = racine.canonicalize() else { return Ok(()) };
    // Le plus proche dossier existant du chemin.
    let existant: Option<PathBuf> = chemin.ancestors().find_map(|p| p.canonicalize().ok());
    match existant {
        Some(vrai) if vrai.starts_with(&racine) => Ok(()),
        Some(_) => Err("Chemin refusé : il sort du dossier du serveur.".into()),
        None => Ok(()),
    }
}

/// Écrit un fichier sans jamais le laisser à moitié écrit (fichier provisoire, puis renommage).
/// Le renommage remplace un éventuel lien au lieu de le suivre.
pub fn ecrire_atomique(chemin: &Path, octets: &[u8]) -> Result<(), String> {
    let mut nom = chemin.file_name().unwrap_or_default().to_os_string();
    nom.push(".bhost-tmp");
    let provisoire = chemin.with_file_name(nom);
    fs::write(&provisoire, octets).map_err(|e| format!("Impossible d'enregistrer ({e})."))?;
    fs::rename(&provisoire, chemin).map_err(|e| {
        let _ = fs::remove_file(&provisoire);
        format!("Impossible d'enregistrer ({e}).")
    })
}

/// Lit au plus `max` octets d'un fichier (le début), pour ne pas charger un fichier énorme en mémoire.
pub fn lire_debut(chemin: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let mut octets = Vec::new();
    fs::File::open(chemin)?.take(max).read_to_end(&mut octets)?;
    Ok(octets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-garde-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copie_limitee() {
        let mut budget = 10;
        let mut sortie = Vec::new();
        copier_limite(&mut &b"12345"[..], &mut sortie, &mut budget).unwrap();
        assert_eq!(budget, 5);
        assert!(copier_limite(&mut &b"123456"[..], &mut sortie, &mut budget).is_err());
    }

    #[test]
    fn profondeur() {
        assert!(verifier_chemin(Path::new("world/region/r.0.0.mca")).is_ok());
        let profond: PathBuf = (0..100).map(|_| "a").collect();
        assert!(verifier_chemin(&profond).is_err());
    }

    #[test]
    fn liens_et_sortie() {
        let d = dossier("liens");
        let dehors = dossier("dehors");
        fs::write(d.join("vrai.txt"), "x").unwrap();
        assert!(!est_un_lien(&d.join("vrai.txt")));
        assert!(dans(&d, &d.join("vrai.txt")).is_ok());
        assert!(dans(&d, &d.join("nouveau/fichier.txt")).is_ok());
        assert!(dans(&d, &dehors.join("x")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&dehors, d.join("lien")).unwrap();
            assert!(est_un_lien(&d.join("lien")));
            assert!(pas_un_lien(&d.join("lien")).is_err());
            assert!(dans(&d, &d.join("lien").join("x.txt")).is_err());
        }
        ecrire_atomique(&d.join("a.txt"), b"bonjour").unwrap();
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"bonjour");
        assert_eq!(lire_debut(&d.join("a.txt"), 3).unwrap(), b"bon");
        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&dehors);
    }
}
