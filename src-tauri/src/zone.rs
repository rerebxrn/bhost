// BHOST dans la zone de notification de Windows (en bas à droite, près de l'heure).
// Réglages › Garder les serveurs allumés : fermer la fenêtre range BHOST ici, les serveurs continuent.
//
// Le menu (clic droit sur l'icône), d'après la maquette :
//   BHOST · 2 serveurs allumés · 3 joueurs
//   ✓ Survie                 En ligne · 3 joueurs     ← clic : arrêter proprement / démarrer
//     Créatif                Arrêté
//   ─────────
//   Ouvrir BHOST
//   Quitter BHOST (arrête les serveurs proprement)
//
// Le menu est refait par un fil à part : jamais par le fil qui prévient d'un changement
// (le fil principal de Windows peut être occupé à attendre la fin des serveurs, et le menu en a besoin).

use crate::Etat;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const ID: &str = "bhost";

/// La sonnette du fil qui refait le menu.
static SONNETTE: OnceLock<Mutex<Sender<()>>> = OnceLock::new();

/// Une ligne du menu pour un serveur : (id, nom, statut, joueurs connectés).
type Ligne = (String, String, String, usize);

/// « 0 joueur », « 1 joueur », « 3 joueurs ».
fn joueurs(n: usize) -> String {
    if n <= 1 { format!("{n} joueur") } else { format!("{n} joueurs") }
}

/// Le nom d'un statut, comme dans la barre latérale.
fn nom_statut(statut: &str) -> &'static str {
    match statut {
        "en-ligne" => "En ligne",
        "demarrage" => "Démarrage…",
        "arret" => "Arrêt…",
        "plante" => "Planté",
        _ => "Arrêté",
    }
}

fn allume(statut: &str) -> bool {
    matches!(statut, "en-ligne" | "demarrage")
}

/// « BHOST · 2 serveurs allumés · 3 joueurs ».
pub fn resume(lignes: &[Ligne]) -> String {
    let presents: usize = lignes.iter().filter(|(_, _, s, _)| allume(s)).map(|l| l.3).sum();
    let fin = if presents > 0 { format!(" · {}", joueurs(presents)) } else { String::new() };
    match lignes.iter().filter(|(_, _, s, _)| allume(s)).count() {
        0 => "BHOST · Aucun serveur allumé".into(),
        1 => format!("BHOST · 1 serveur allumé{fin}"),
        n => format!("BHOST · {n} serveurs allumés{fin}"),
    }
}

/// Le texte d'une ligne : le nom, puis le statut calé à droite (la tabulation de Windows).
/// Un « & » dans le nom est doublé : seul, Windows le prendrait pour un raccourci clavier.
pub fn texte_ligne(nom: &str, statut: &str, presents: usize) -> String {
    let detail = if statut == "en-ligne" { format!(" · {}", joueurs(presents)) } else { String::new() };
    format!("{}\t{}{detail}", nom.replace('&', "&&"), nom_statut(statut))
}

/// Les serveurs et leur statut, lus d'un coup (les verrous sont rendus tout de suite).
fn lignes(app: &AppHandle) -> Vec<Ligne> {
    let etat = app.state::<Etat>();
    let liste: Vec<(String, String)> = etat.liste.lock().unwrap().iter().map(|s| (s.id.clone(), s.nom.clone())).collect();
    let statuts = etat.statuts.lock().unwrap().clone();
    let presents: std::collections::HashMap<String, usize> = etat.joueurs.lock().unwrap().iter().map(|(id, l)| (id.clone(), l.len())).collect();
    liste.into_iter().map(|(id, nom)| {
        let statut = statuts.get(&id).cloned().unwrap_or_else(|| "arrete".into());
        let n = presents.get(&id).copied().unwrap_or(0);
        (id, nom, statut, n)
    }).collect()
}

fn construire(app: &AppHandle, lignes: &[Ligne]) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "titre", resume(lignes), false, None::<&str>)?)?;
    if !lignes.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    for (id, nom, statut, presents) in lignes {
        // Coché = allumé. Pendant l'arrêt, on ne peut pas cliquer.
        let ligne = CheckMenuItem::with_id(app, format!("srv:{id}"), texte_ligne(nom, statut, *presents), statut != "arret", allume(statut), None::<&str>)?;
        menu.append(&ligne)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "ouvrir", "Ouvrir BHOST", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "quitter", "Quitter BHOST", true, None::<&str>)?)?;
    Ok(menu)
}

/// Réaffiche la fenêtre de BHOST (au premier plan).
pub fn montrer(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window("main") {
        let _ = fenetre.show();
        let _ = fenetre.unminimize();
        let _ = fenetre.set_focus();
    }
}

/// Clic sur un serveur du menu : arrêt propre s'il tourne, sinon démarrage.
fn basculer(app: &AppHandle, id: &str) {
    let statut = app.state::<Etat>().statuts.lock().unwrap().get(id).cloned().unwrap_or_default();
    let fait = if allume(&statut) { crate::arreter(app, id) } else if statut == "arret" { Ok(()) } else { crate::lancer_serveur(app, id) };
    if let Err(erreur) = fait {
        crate::dire_ligne(app, id, &format!("[BHOST!] {erreur}"));
    }
    // Windows a déjà coché ou décoché la ligne : on remet le vrai état.
    rafraichir();
}

/// L'icône de la zone : celle du thème (« azur-clair »…), en 32 px.
fn image(theme: &str) -> Option<Image<'static>> {
    let png = crate::icone::image(crate::icone::fichier(theme), 32)?;
    Image::from_bytes(png).ok().map(|i| i.to_owned())
}

/// Pose l'icône dans la zone de notification et lance le fil qui tient le menu à jour.
pub fn installer(app: &AppHandle) -> tauri::Result<()> {
    let depart = lignes(app);
    let mut icone = TrayIconBuilder::with_id(ID)
        .tooltip(resume(&depart))
        .menu(&construire(app, &depart)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, evenement| match evenement.id().as_ref() {
            "ouvrir" => montrer(app),
            // Quitter : RunEvent::Exit arrête les serveurs proprement (la fenêtre part d'abord : elle ne répondrait plus).
            "quitter" => {
                if let Some(fenetre) = app.get_webview_window("main") {
                    let _ = fenetre.hide();
                }
                app.exit(0)
            }
            autre => {
                if let Some(id) = autre.strip_prefix("srv:") {
                    let (app, id) = (app.clone(), id.to_string());
                    // Démarrer peut prendre un moment (Java…) : pas sur le fil de Windows.
                    std::thread::spawn(move || basculer(&app, &id));
                }
            }
        })
        .on_tray_icon_event(|icone, evenement| match evenement {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } | TrayIconEvent::DoubleClick { .. } => {
                montrer(icone.app_handle())
            }
            _ => {}
        });
    if let Some(i) = image("azur-clair").or_else(|| app.default_window_icon().cloned()) {
        icone = icone.icon(i);
    }
    icone.build(app)?;

    let (envoi, reception) = channel::<()>();
    let _ = SONNETTE.set(Mutex::new(envoi));
    let app = app.clone();
    std::thread::spawn(move || {
        let mut vu = depart;
        loop {
            // Un changement annoncé, ou un coup d'œil toutes les 3 s (nouveau serveur, nom changé…).
            match reception.recv_timeout(Duration::from_secs(3)) {
                Ok(()) => {
                    // Plusieurs changements d'un coup : un seul nouveau menu.
                    std::thread::sleep(Duration::from_millis(120));
                    while reception.try_recv().is_ok() {}
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let maintenant = lignes(&app);
            if maintenant == vu {
                continue;
            }
            let Some(zone) = app.tray_by_id(ID) else { continue };
            if let Ok(menu) = construire(&app, &maintenant) {
                let _ = zone.set_menu(Some(menu));
                let _ = zone.set_tooltip(Some(resume(&maintenant)));
                vu = maintenant;
            }
        }
    });
    Ok(())
}

/// Demande un nouveau menu (un serveur a changé d'état). Ne bloque jamais.
pub fn rafraichir() {
    if let Some(sonnette) = SONNETTE.get() {
        let _ = sonnette.lock().unwrap().send(());
    }
}

/// Le thème change : l'icône de la zone suit.
pub fn changer_icone(app: &AppHandle, theme: &str) {
    if let (Some(zone), Some(i)) = (app.tray_by_id(ID), image(theme)) {
        let _ = zone.set_icon(Some(i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_et_lignes() {
        let l = |s: &str, n: usize| ("a".to_string(), "Survie".to_string(), s.to_string(), n);
        assert_eq!(resume(&[]), "BHOST · Aucun serveur allumé");
        assert_eq!(resume(&[l("en-ligne", 0), l("arrete", 0)]), "BHOST · 1 serveur allumé");
        assert_eq!(resume(&[l("en-ligne", 1), l("arrete", 0)]), "BHOST · 1 serveur allumé · 1 joueur");
        assert_eq!(resume(&[l("en-ligne", 2), l("demarrage", 1), l("plante", 0)]), "BHOST · 2 serveurs allumés · 3 joueurs");
        assert_eq!(texte_ligne("Survie", "en-ligne", 2), "Survie\tEn ligne · 2 joueurs");
        assert_eq!(texte_ligne("Survie", "en-ligne", 0), "Survie\tEn ligne · 0 joueur");
        assert_eq!(texte_ligne("Toi & moi", "plante", 0), "Toi && moi\tPlanté");
        assert_eq!(texte_ligne("X", "?", 0), "X\tArrêté");
    }

    #[test]
    fn chaque_theme_donne_une_icone() {
        for theme in ["azur-clair", "azur-sombre", "ambre-clair", "ambre-sombre"] {
            let i = image(theme).unwrap();
            assert_eq!((i.width(), i.height()), (32, 32));
        }
    }
}
