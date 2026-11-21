// Les problèmes d'un serveur, montrés en cartes sur l'Accueil (maquette : « États d'erreur ») :
//   Java introuvable, port déjà pris, plantage, manque de mémoire.
// Et la connexion Internet, pour les serveurs ouverts aux amis.
//
// BHOST lit la console pendant que le serveur tourne (lire_indice) ; quand il s'arrête tout seul,
// on choisit la carte d'après ce qu'on a vu. La carte disparaît au démarrage suivant.

use crate::{serveurs, Etat};
use serde::Serialize;
use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use crate::hote::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "genre", rename_all = "camelCase")]
pub enum Probleme {
    /// Java est introuvable : BHOST propose de l'installer.
    Java { version: u32 },
    /// Le port est pris. `libre` : le premier port libre après lui. `serveur` : un autre serveur de BHOST qui l'a.
    Port { port: u16, libre: Option<u16>, serveur: Option<String> },
    /// Il s'est arrêté tout seul, sans raison connue.
    Crash,
    /// Plus de mémoire. `demarrage` : le PC n'avait pas assez de mémoire libre pour le lancer.
    Memoire {
        go: f64,
        #[serde(rename = "libreGo")]
        libre_go: f64,
        demarrage: bool,
    },
    /// Machine dédiée : son stockage max est atteint, BHOST l'a arrêté.
    Stockage { go: f64 },
}

/// Ce que la console a montré pendant ce lancement.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Indices {
    pub port: bool,
    pub memoire: bool,
    pub reserve: bool,
}

#[derive(Serialize, Clone)]
struct NouveauProbleme<'a> {
    id: &'a str,
    probleme: Option<&'a Probleme>,
}

static PROBLEMES: LazyLock<Mutex<HashMap<String, Probleme>>> = LazyLock::new(Default::default);
static INDICES: LazyLock<Mutex<HashMap<String, Indices>>> = LazyLock::new(Default::default);
/// Faux quand on vient de voir que la connexion Internet est coupée.
static INTERNET: AtomicBool = AtomicBool::new(true);

/// Une ligne de console : un indice de problème ?
/// Les lignes INFO sont ignorées : un joueur ne peut pas inventer une panne dans le chat.
pub fn lire_indice(ligne: &str) -> Option<Indices> {
    let tete: String = ligne.chars().take(48).collect();
    if tete.contains("INFO]") || tete.split_whitespace().nth(1) == Some("INFO") {
        return None;
    }
    if ligne.contains("FAILED TO BIND TO PORT") || ligne.contains("Address already in use") {
        return Some(Indices { port: true, ..Default::default() });
    }
    if ligne.contains("java.lang.OutOfMemoryError") {
        return Some(Indices { memoire: true, ..Default::default() });
    }
    if ligne.contains("Could not reserve enough space for") || ligne.contains("Insufficient memory for the Java Runtime") {
        return Some(Indices { reserve: true, ..Default::default() });
    }
    None
}

/// Le premier port libre après `depart` (on en essaie 50).
pub fn port_libre(depart: u16, pris: &dyn Fn(u16) -> bool) -> Option<u16> {
    (1..=50u16).filter_map(|i| depart.checked_add(i)).find(|p| !pris(*p))
}

fn prevenir(app: &AppHandle, id: &str, probleme: Option<&Probleme>) {
    let _ = app.emit("probleme", NouveauProbleme { id, probleme });
}

/// Une ligne de la console du serveur.
pub fn suivre(id: &str, ligne: &str) {
    if let Some(vu) = lire_indice(ligne) {
        let mut tous = INDICES.lock().unwrap();
        let i = tous.entry(id.to_string()).or_default();
        i.port |= vu.port;
        i.memoire |= vu.memoire;
        i.reserve |= vu.reserve;
    }
}

/// Le serveur (re)démarre : on oublie le problème d'avant.
/// `garder_java` : pendant le téléchargement de Java, la carte Java reste (elle montre l'avancement).
pub fn effacer(app: &AppHandle, id: &str, garder_java: bool) {
    INDICES.lock().unwrap().remove(id);
    let retire = {
        let mut tous = PROBLEMES.lock().unwrap();
        match tous.get(id) {
            Some(Probleme::Java { .. }) if garder_java => false,
            Some(_) => tous.remove(id).is_some(),
            None => false,
        }
    };
    if retire {
        prevenir(app, id, None);
    }
}

/// La phrase de la notification Windows.
pub fn phrase(probleme: &Probleme) -> String {
    match probleme {
        Probleme::Java { version } => format!("Java {version} est introuvable."),
        Probleme::Port { port, .. } => format!("Le port {port} est déjà pris."),
        Probleme::Crash => "Le serveur a planté.".into(),
        Probleme::Memoire { demarrage: true, .. } => "Pas assez de mémoire libre pour le lancer.".into(),
        Probleme::Memoire { .. } => "Le serveur a manqué de mémoire.".into(),
        Probleme::Stockage { .. } => "Stockage max atteint : le serveur s'est arrêté.".into(),
    }
}

fn poser(app: &AppHandle, id: &str, probleme: Probleme) {
    PROBLEMES.lock().unwrap().insert(id.to_string(), probleme.clone());
    prevenir(app, id, Some(&probleme));
    // BHOST est en arrière-plan : une notification Windows, pour ne pas le rater.
    let nom = app.state::<Etat>().liste.lock().unwrap().iter().find(|s| s.id == id).map(|s| s.nom.clone()).unwrap_or_default();
    crate::hote::notifier(app, &nom, &phrase(&probleme), false);
}

/// Le stockage max (en Mo) est atteint : BHOST arrête le serveur.
pub fn stockage(app: &AppHandle, id: &str, max_mo: u64) {
    poser(app, id, Probleme::Stockage { go: (max_mo as f64 / 1024.0 * 10.0).round() / 10.0 });
}

/// Java est introuvable : on propose la version qu'il lui faut.
pub fn java(app: &AppHandle, id: &str, version: u32) {
    poser(app, id, Probleme::Java { version });
}

/// Mémoire libre du PC, en Go (une décimale).
fn memoire_libre_go() -> f64 {
    let octets = sysinfo::System::new().map(|mut s| {
        s.refresh_memory();
        s.available_memory()
    });
    (octets.unwrap_or(0) as f64 / 1_073_741_824.0 * 10.0).round() / 10.0
}

/// Le serveur s'est arrêté sans qu'on le demande : quelle carte montrer ?
pub fn plante(app: &AppHandle, id: &str) {
    let indices = INDICES.lock().unwrap().remove(id).unwrap_or_default();
    let etat = app.state::<Etat>();
    let Some(serveur) = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned() else { return };
    let probleme = if indices.port {
        let port = serveurs::port(Path::new(&serveur.dossier));
        // Un autre serveur de BHOST, allumé sur ce port ?
        let autres: Vec<(String, String, u16)> = etat
            .liste
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.id != id)
            .map(|s| (s.id.clone(), s.nom.clone(), serveurs::port(Path::new(&s.dossier))))
            .collect();
        let vivants = etat.vivants.lock().unwrap();
        let serveur_pris = autres.iter().find(|(i, _, p)| *p == port && vivants.contains_key(i)).map(|(_, n, _)| n.clone());
        let ports_bhost: Vec<u16> = autres.iter().map(|(_, _, p)| *p).collect();
        drop(vivants);
        let libre = port_libre(port, &|p| ports_bhost.contains(&p) || TcpListener::bind(("0.0.0.0", p)).is_err());
        Probleme::Port { port, libre, serveur: serveur_pris }
    } else if indices.memoire || indices.reserve {
        Probleme::Memoire { go: f64::from(serveur.ram_mo) / 1024.0, libre_go: memoire_libre_go(), demarrage: indices.reserve && !indices.memoire }
    } else {
        Probleme::Crash
    };
    poser(app, id, probleme);
}

/// Le serveur a été retiré de BHOST.
pub fn oublier(id: &str) {
    PROBLEMES.lock().unwrap().remove(id);
    INDICES.lock().unwrap().remove(id);
}

/// Tous les problèmes en cours (au lancement de l'interface).
#[cfg_attr(feature = "bureau", tauri::command)]
pub fn problemes_serveurs() -> HashMap<String, Probleme> {
    PROBLEMES.lock().unwrap().clone()
}

/// On arrive sur Internet ? (deux adresses connues, 3 s chacune)
fn sonder() -> bool {
    let adresses: [SocketAddr; 2] = [([1, 1, 1, 1], 443).into(), ([8, 8, 8, 8], 53).into()];
    adresses.iter().any(|a| TcpStream::connect_timeout(a, Duration::from_secs(3)).is_ok())
}

fn noter_internet(app: &AppHandle, ok: bool) {
    if INTERNET.swap(ok, Ordering::SeqCst) != ok {
        let _ = app.emit("internet", ok);
    }
}

/// Un serveur ouvert aux amis est allumé ? Alors on regarde la connexion toutes les 20 s.
pub fn surveiller_internet(app: AppHandle) {
    std::thread::spawn(move || loop {
        let a_surveiller = {
            let etat = app.state::<Etat>();
            let ouverts: Vec<String> = etat.liste.lock().unwrap().iter().filter(|s| s.ouvert_amis).map(|s| s.id.clone()).collect();
            let vivants = etat.vivants.lock().unwrap();
            ouverts.iter().any(|id| vivants.contains_key(id))
        };
        noter_internet(&app, !a_surveiller || sonder());
        std::thread::sleep(Duration::from_secs(20));
    });
}

#[cfg_attr(feature = "bureau", tauri::command)]
pub fn etat_internet() -> bool {
    INTERNET.load(Ordering::SeqCst)
}

/// Bouton « Réessayer maintenant ».
#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn verifier_internet(app: AppHandle) -> bool {
    let ok = crate::hote::async_runtime::spawn_blocking(sonder).await.unwrap_or(false);
    noter_internet(&app, ok);
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_pris_vu_dans_la_console() {
        assert!(lire_indice("[10:42:01 WARN]: **** FAILED TO BIND TO PORT!").unwrap().port);
        assert!(lire_indice("[10:42:01] [Server thread/WARN]: **** FAILED TO BIND TO PORT!").unwrap().port);
    }

    #[test]
    fn memoire_vue_dans_la_console() {
        assert!(lire_indice("Exception in thread \"Server thread\" java.lang.OutOfMemoryError: Java heap space").unwrap().memoire);
        assert!(lire_indice("Error occurred during initialization of VM\nCould not reserve enough space for 8388608KB object heap").unwrap().reserve);
    }

    #[test]
    fn le_chat_ne_compte_pas() {
        assert_eq!(lire_indice("[10:42:01 INFO]: <Bob> FAILED TO BIND TO PORT"), None);
        assert_eq!(lire_indice("[10:42:01 INFO]: <Bob> java.lang.OutOfMemoryError"), None);
        assert_eq!(lire_indice("[10:42:01 INFO]: Done (3.2s)!"), None);
    }

    #[test]
    fn premier_port_libre() {
        assert_eq!(port_libre(25565, &|p| p == 25566), Some(25567));
        assert_eq!(port_libre(25565, &|_| false), Some(25566));
        assert_eq!(port_libre(65535, &|_| false), None);
    }

    #[test]
    fn forme_pour_l_interface() {
        let texte = serde_json::to_string(&Probleme::Memoire { go: 4.0, libre_go: 2.5, demarrage: false }).unwrap();
        assert_eq!(texte, r#"{"genre":"memoire","go":4.0,"libreGo":2.5,"demarrage":false}"#);
        assert_eq!(serde_json::to_string(&Probleme::Crash).unwrap(), r#"{"genre":"crash"}"#);
    }
}
