//! Mettre le dossier d'un serveur à la Corbeille de Windows : on peut encore le récupérer.
//! Sur BHOST OS (pas de Corbeille) : supprimé pour de bon.
use std::path::Path;

/// Met un dossier à la Corbeille. Si Windows ne peut pas (disque sans Corbeille, clé USB…),
/// il demande lui-même avant d'effacer pour de bon.
#[cfg(windows)]
pub fn jeter(dossier: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, FO_DELETE,
        SHFILEOPSTRUCTW,
    };
    // Windows veut une liste de chemins finie par deux zéros.
    let mut chemin: Vec<u16> = dossier.as_os_str().encode_wide().collect();
    if chemin.contains(&0) {
        return Err("Ce dossier a un nom bizarre.".into());
    }
    chemin.extend([0, 0]);
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: chemin.as_ptr(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOF_WANTNUKEWARNING) as u16,
        ..Default::default()
    };
    // SAFETY : `operation` et `chemin` vivent jusqu'à la fin de l'appel.
    let resultat = unsafe { SHFileOperationW(&mut operation) };
    if operation.fAnyOperationsAborted != 0 {
        return Err("Suppression annulée.".into());
    }
    if resultat != 0 {
        return Err("Le dossier n’a pas pu partir. Un fichier est peut-être ouvert.".into());
    }
    Ok(())
}

/// BHOST OS (pas de Corbeille) : effacé pour de bon. Un lien symbolique est retiré lui-même (jamais ce qu'il vise).
#[cfg(not(windows))]
pub fn jeter(dossier: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(dossier).map_err(|_| "Introuvable.".to_string())?;
    let resultat = if meta.is_dir() { std::fs::remove_dir_all(dossier) } else { std::fs::remove_file(dossier) };
    resultat.map_err(|e| format!("Impossible de supprimer ({e})."))
}
