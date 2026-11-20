// Réglages › Arrêter proprement : quand Windows s'éteint (ou redémarre), BHOST arrête d'abord
// les serveurs avec « stop » : le monde est enregistré, rien n'est perdu.
//
// Comment : Windows prévient chaque fenêtre (WM_QUERYENDSESSION puis WM_ENDSESSION).
// BHOST écoute ces messages sur sa fenêtre (même cachée dans la zone de notification),
// affiche une raison sur l'écran d'extinction (« BHOST enregistre tes mondes… »),
// attend la fin des serveurs, puis laisse Windows continuer tout seul.
// BHOST demande aussi à être prévenu avant les autres programmes (et donc avant Java).

#![cfg(windows)]

use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{WM_ENDSESSION, WM_QUERYENDSESSION};

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Faut-il retenir Windows ? Seulement si le réglage est actif et qu'un serveur tourne.
fn a_retenir() -> bool {
    let Some(app) = APP.get() else { return false };
    crate::PROPREMENT.load(Ordering::Relaxed) && !app.state::<crate::Etat>().vivants.lock().unwrap().is_empty()
}

fn raison() -> Vec<u16> {
    "BHOST enregistre tes mondes…".encode_utf16().chain(Some(0)).collect()
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _donnee: usize) -> LRESULT {
    match message {
        WM_QUERYENDSESSION => {
            if a_retenir() {
                let texte = raison();
                // SAFETY : `hwnd` est notre fenêtre ; `texte` finit par un zéro et vit pendant l'appel.
                unsafe { ShutdownBlockReasonCreate(hwnd, texte.as_ptr()) };
            }
            // Oui, Windows peut s'éteindre : on fera le travail dans WM_ENDSESSION (Windows attend).
            1
        }
        WM_ENDSESSION => {
            if wparam != 0 {
                if let Some(app) = APP.get() {
                    if a_retenir() {
                        crate::automatique::FERMETURE.store(true, Ordering::Relaxed);
                        crate::tout_arreter(app);
                    }
                    app.state::<crate::Etat>().acces.tout_fermer();
                }
            }
            // SAFETY : `hwnd` est notre fenêtre (sans raison posée, l'appel ne fait rien).
            unsafe { ShutdownBlockReasonDestroy(hwnd) };
            0
        }
        // SAFETY : les autres messages suivent leur chemin habituel.
        _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
    }
}

/// À appeler une fois, au démarrage, sur le fil principal (celui de la fenêtre).
pub fn surveiller(app: &AppHandle) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let _ = APP.set(app.clone());
    // Prévenu parmi les premiers programmes quand Windows s'éteint (avant Java, resté au niveau normal).
    // SAFETY : simple réglage de notre propre programme (0x3FF : le plus tôt permis aux applications).
    unsafe { windows_sys::Win32::System::Threading::SetProcessShutdownParameters(0x3FF, 0) };
    let Some(fenetre) = app.get_webview_window("main") else { return };
    let Ok(poignee) = fenetre.window_handle() else { return };
    let RawWindowHandle::Win32(win32) = poignee.as_raw() else { return };
    // SAFETY : la fenêtre est vivante et appartient à ce fil ; `procedure` vit autant que le programme.
    unsafe { SetWindowSubclass(win32.hwnd.get() as HWND, Some(procedure), 0xB405, 0) };
}
