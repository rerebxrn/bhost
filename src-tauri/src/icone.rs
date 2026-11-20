// Hors Windows, ce fichier ne sert qu'aux tests.
#![cfg_attr(not(windows), allow(dead_code))]

// L'icône de la fenêtre dans Windows (barre des tâches, Alt+Tab) suit le thème : Azur ou Ambre, clair ou sombre.
// L'icône du programme lui-même (raccourci, explorateur de fichiers) est fixée à la fabrication : elle reste en Azur clair.
// Les dessins viennent de docs/logo/icone.py.

/// Les icônes des quatre thèmes, rangées dans le programme.
const AZUR_CLAIR: &[u8] = include_bytes!("../icons/icon.ico");
const AZUR_SOMBRE: &[u8] = include_bytes!("../icons/azur-sombre.ico");
const AMBRE_CLAIR: &[u8] = include_bytes!("../icons/ambre-clair.ico");
const AMBRE_SOMBRE: &[u8] = include_bytes!("../icons/ambre-sombre.ico");

/// Le fichier d'icône d'un thème, « azur-clair », « ambre-sombre »… (Azur clair si le nom est inconnu).
/// On pose toujours l'icône nous-mêmes, Azur clair compris : Tauri ne pose que la petite icône,
/// et sans grande icône, Windows garderait l'ancienne dans la barre des tâches.
pub fn fichier(theme: &str) -> &'static [u8] {
    match theme {
        "azur-sombre" => AZUR_SOMBRE,
        "ambre-clair" => AMBRE_CLAIR,
        "ambre-sombre" => AMBRE_SOMBRE,
        _ => AZUR_CLAIR,
    }
}

/// Un fichier .ico contient le même dessin à plusieurs tailles.
/// Renvoie l'image la plus petite qui fait au moins `taille` pixels (ou la plus grande, à défaut).
pub fn image(ico: &[u8], taille: u32) -> Option<&[u8]> {
    let nombre = u16::from_le_bytes(ico.get(4..6)?.try_into().ok()?) as usize;
    let mut choix: Option<(u32, &[u8])> = None;
    for n in 0..nombre {
        // Chaque image est décrite sur 16 octets : largeur, …, taille des données, position des données.
        let fiche = ico.get(6 + 16 * n..22 + 16 * n)?;
        let largeur = if fiche[0] == 0 { 256 } else { fiche[0] as u32 };
        let longueur = u32::from_le_bytes(fiche[8..12].try_into().ok()?) as usize;
        let debut = u32::from_le_bytes(fiche[12..16].try_into().ok()?) as usize;
        let donnees = ico.get(debut..debut.checked_add(longueur)?)?;
        let mieux = match choix {
            None => true,
            Some((gardee, _)) if gardee < taille => largeur > gardee,
            Some((gardee, _)) => largeur >= taille && largeur < gardee,
        };
        if mieux {
            choix = Some((largeur, donnees));
        }
    }
    choix.map(|(_, donnees)| donnees)
}

/// Les icônes posées par BHOST, à rendre à Windows quand on en change.
#[derive(Default)]
pub struct Icones {
    creees: Vec<isize>,
}

/// Change l'icône de la fenêtre pour celle du thème.
#[cfg(windows)]
pub fn appliquer(fenetre: &tauri::WebviewWindow, theme: &str, icones: &mut Icones) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateIconFromResourceEx, DestroyIcon, GetSystemMetrics, SendMessageW, HICON, ICON_BIG, ICON_SMALL, LR_DEFAULTCOLOR, SM_CXICON,
        SM_CXSMICON, WM_SETICON,
    };

    let poignee = fenetre.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(win32) = poignee.as_raw() else { return Ok(()) };
    let hwnd = win32.hwnd.get() as HWND;

    // Pose une icône (petite ou grande) sur la fenêtre.
    let poser = |quelle: u32, icone: isize| {
        // SAFETY : `hwnd` est la fenêtre de BHOST, bien vivante ; `icone` est une icône valide créée juste avant.
        unsafe { SendMessageW(hwnd, WM_SETICON, quelle as usize, icone) };
    };

    let ico = fichier(theme);
    // Windows nous dit quelles tailles il veut (elles dépendent de l'échelle de l'écran).
    let creer = |mesure: i32| -> Result<isize, String> {
        // SAFETY : simple lecture d'un réglage de Windows.
        let taille = unsafe { GetSystemMetrics(mesure) }.max(16);
        let donnees = image(ico, taille as u32).ok_or("Icône illisible.")?;
        // SAFETY : `donnees` est une image PNG complète tirée de notre propre fichier .ico ;
        // le pointeur et la longueur viennent de la même tranche.
        let icone = unsafe { CreateIconFromResourceEx(donnees.as_ptr(), donnees.len() as u32, 1, 0x0003_0000, taille, taille, LR_DEFAULTCOLOR) };
        if icone.is_null() { Err("Windows a refusé l'icône.".to_string()) } else { Ok(icone as isize) }
    };
    let petite = creer(SM_CXSMICON)?;
    let grande = match creer(SM_CXICON) {
        Ok(grande) => grande,
        Err(erreur) => {
            // SAFETY : la petite icône vient d'être créée et n'est posée nulle part.
            unsafe { DestroyIcon(petite as HICON) };
            return Err(erreur);
        }
    };
    poser(ICON_SMALL, petite);
    poser(ICON_BIG, grande);
    let nouvelles = vec![petite, grande];
    // On rend à Windows les icônes qu'on avait créées et qui ne servent plus.
    // (Celle de Tauri, posée au départ, reste à Tauri : on n'y touche pas.)
    for ancienne in std::mem::replace(&mut icones.creees, nouvelles) {
        // SAFETY : cette icône a été créée par BHOST et n'est plus posée sur aucune fenêtre.
        unsafe { DestroyIcon(ancienne as HICON) };
    }
    Ok(())
}

/// Ailleurs que sur Windows, il n'y a rien à faire.
#[cfg(not(windows))]
pub fn appliquer(_fenetre: &tauri::WebviewWindow, _theme: &str, _icones: &mut Icones) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La largeur d'une image PNG, lue dans son en-tête.
    fn largeur(png: &[u8]) -> u32 {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        u32::from_be_bytes(png[16..20].try_into().unwrap())
    }

    #[test]
    fn choisit_la_bonne_taille_dans_le_fichier() {
        for ico in [AZUR_CLAIR, AZUR_SOMBRE, AMBRE_CLAIR, AMBRE_SOMBRE] {
            assert_eq!(largeur(image(ico, 16).unwrap()), 16);
            assert_eq!(largeur(image(ico, 20).unwrap()), 20);
            assert_eq!(largeur(image(ico, 32).unwrap()), 32);
            assert_eq!(largeur(image(ico, 36).unwrap()), 40); // pas de 36 : la taille juste au-dessus
            assert_eq!(largeur(image(ico, 48).unwrap()), 48);
            assert_eq!(largeur(image(ico, 100).unwrap()), 128);
            assert_eq!(largeur(image(ico, 256).unwrap()), 256);
            assert_eq!(largeur(image(ico, 999).unwrap()), 256); // trop grand : la plus grande
        }
    }

    #[test]
    fn ne_plante_pas_sur_un_fichier_abime() {
        assert_eq!(image(b"", 32), None);
        assert_eq!(image(&AMBRE_CLAIR[..40], 32), None);
        assert_eq!(image(&[0, 0, 1, 0, 0, 0], 32), None);
    }

    #[test]
    fn chaque_theme_a_son_icone() {
        assert_eq!(fichier("azur-clair"), AZUR_CLAIR);
        assert_eq!(fichier("azur-sombre"), AZUR_SOMBRE);
        assert_eq!(fichier("ambre-clair"), AMBRE_CLAIR);
        assert_eq!(fichier("ambre-sombre"), AMBRE_SOMBRE);
        assert_eq!(fichier("n'importe quoi"), AZUR_CLAIR);
        // Quatre dessins différents.
        assert_ne!(AZUR_CLAIR, AZUR_SOMBRE);
        assert_ne!(AMBRE_CLAIR, AMBRE_SOMBRE);
        assert_ne!(AZUR_CLAIR, AMBRE_CLAIR);
    }
}
