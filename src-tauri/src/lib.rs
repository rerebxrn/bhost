// Point d'entrée de la partie Rust de BHOST.
// Ici : les commandes que l'interface peut appeler (avec invoke),
// et les nouvelles que Rust lui envoie (events « statut », « console », « joueurs », « mesure », « tps », « automatisations », « demandes »).

mod acces;
mod automatique;
mod bedrock;
mod automatisations;
mod catalogue;
mod copie;
mod creation;
mod corbeille;
mod demandes;
#[cfg(windows)]
mod extinction;
mod extensions;
mod fiche;
mod fichiers;
mod garde;
mod icone;
mod icone_jeu;
mod installeur;
mod java;
mod machine;
mod joueurs;
mod listes;
mod mise_a_jour;
mod mondes;
mod nbt;
mod problemes;
mod processus;
mod regles;
mod reseau;
mod ressources;
mod reveil;
mod sauvegarde;
mod serveurs;
mod spigot;
mod tetes;
mod tps;
mod tunnel;
mod zone;

use processus::{Processus, Signal};
use serde::{Deserialize, Serialize};
use serveurs::Serveur;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

/// Réglages › Notifications : BHOST peut envoyer des notifications Windows.
pub(crate) static NOTIFICATIONS: AtomicBool = AtomicBool::new(true);
/// Réglages › Empêcher la veille : le PC reste éveillé tant qu'un serveur tourne.
static VEILLE: AtomicBool = AtomicBool::new(true);
/// Réglages › Garder les serveurs allumés : fermer la fenêtre range BHOST dans la zone de notification.
static GARDER: AtomicBool = AtomicBool::new(true);
/// Réglages › Arrêter proprement : quand Windows s'éteint, BHOST arrête d'abord les serveurs (extinction.rs).
pub(crate) static PROPREMENT: AtomicBool = AtomicBool::new(true);
/// La fenêtre a déjà été rangée une fois (la notification « BHOST continue » n'apparaît qu'une fois).
static DEJA_RANGE: AtomicBool = AtomicBool::new(false);
/// L'option ajoutée au lancement avec Windows : BHOST démarre rangé, sans fenêtre.
const ARRIERE_PLAN: &str = "--arriere-plan";

/// Nombre de lignes de console gardées en mémoire par serveur.
const LIGNES_MAX: usize = 2000;

/// Ce que BHOST garde en mémoire tant qu'il est ouvert.
struct Etat {
    /// Le dossier des données de BHOST : %APPDATA%\BHOST
    dossier: PathBuf,
    /// De quoi aller sur Internet (têtes des joueurs…).
    reseau: ureq::Agent,
    /// Les icônes posées sur la fenêtre (elles suivent le thème).
    icones: Mutex<icone::Icones>,
    /// Le fichier où la liste est enregistrée : %APPDATA%\BHOST\serveurs.json
    fichier: PathBuf,
    liste: Mutex<Vec<Serveur>>,
    /// Les serveurs qui tournent en ce moment.
    vivants: Mutex<HashMap<String, Arc<Processus>>>,
    /// Les serveurs qui attendent leur Java (téléchargement en cours) avant de démarrer.
    preparation: Mutex<HashSet<String>>,
    /// Les serveurs en train d'être copiés (clic droit › Dupliquer).
    copies: Mutex<HashSet<String>>,
    /// Les serveurs occupés par un long travail (sauvegarde, changement de version, import d'un monde…).
    travaux: Mutex<HashSet<String>>,
    /// Les règles du jeu envoyées à un serveur allumé, pas encore écrites dans son monde (oubliées à l'arrêt).
    regles_envoyees: Mutex<HashMap<String, BTreeMap<String, i64>>>,
    /// Un seul téléchargement de Java à la fois (deux serveurs peuvent vouloir le même).
    installation_java: Mutex<()>,
    /// Une seule fabrication de Spigot à la fois (BuildTools partage son dossier de travail).
    buildtools: Mutex<()>,
    /// L'état de chaque serveur : arrete, demarrage, en-ligne, arret, plante.
    statuts: Mutex<HashMap<String, String>>,
    /// Les dernières lignes de console de chaque serveur.
    journaux: Mutex<HashMap<String, Journal>>,
    /// Depuis quand chaque serveur est en ligne (heure en millisecondes).
    depuis: Mutex<HashMap<String, u64>>,
    /// Les joueurs connectés à chaque serveur.
    joueurs: Mutex<HashMap<String, Vec<String>>>,
    /// L'heure d'arrivée (millisecondes) de chaque joueur connecté, par serveur.
    arrivees: Mutex<HashMap<String, HashMap<String, u64>>>,
    /// L'identifiant du compte de chaque joueur vu (pseudo en minuscules → identifiant), lu dans les consoles.
    identifiants: Mutex<HashMap<String, String>>,
    /// Le serveur dont l'accueil est affiché : c'est le seul à qui BHOST demande ses ticks par seconde.
    tps_regarde: Mutex<Option<String>>,
    /// Où en sont les demandes de ticks par seconde, pour chaque serveur allumé.
    tps: Mutex<HashMap<String, tps::Suivi>>,
    /// Onglet Automatisations : historique, seuils, réveil.
    auto: automatique::EtatAuto,
    /// Paramètres › Ouvert à tes amis : les ports ouverts sur la box.
    acces: Arc<acces::EtatAcces>,
    /// Joueurs › Demandes : les joueurs refusés par la liste blanche (gardés dans demandes.json).
    demandes: Mutex<demandes::Registre>,
    /// Onglet Plugins / Mods / Datapacks.
    catalogue: catalogue::EtatCatalogue,
}

/// La console d'un serveur, gardée en mémoire.
#[derive(Default)]
struct Journal {
    /// Combien de lignes ont été écrites depuis l'ouverture de BHOST.
    total: u64,
    /// Les dernières lignes (LIGNES_MAX au plus).
    lignes: Vec<String>,
}

/// Les lignes gardées, pour l'ouverture de la page Console.
#[derive(Serialize)]
struct ConsoleVue {
    /// Numéro de la dernière ligne de la liste.
    dernier: u64,
    lignes: Vec<String>,
}

/// Un serveur avec son état du moment, pour l'interface.
#[derive(Serialize, Clone)]
struct ServeurVu {
    #[serde(flatten)]
    serveur: Serveur,
    statut: String,
    /// Port du serveur, lu dans server.properties.
    port: u16,
    /// Depuis quand il est en ligne (millisecondes), s'il l'est.
    depuis: Option<u64>,
    /// Les joueurs connectés, et le maximum autorisé (server.properties).
    joueurs: Vec<String>,
    #[serde(rename = "joueursMax")]
    joueurs_max: u32,
    /// Machine dédiée : la place prise par le serveur, en Mo.
    #[serde(rename = "disqueMo", skip_serializing_if = "Option::is_none")]
    disque_mo: Option<u64>,
}

fn vu(etat: &Etat, serveur: &Serveur) -> ServeurVu {
    ServeurVu {
        statut: etat.statuts.lock().unwrap().get(&serveur.id).cloned().unwrap_or_else(|| "arrete".into()),
        port: serveurs::port(Path::new(&serveur.dossier)),
        depuis: etat.depuis.lock().unwrap().get(&serveur.id).copied(),
        joueurs: etat.joueurs.lock().unwrap().get(&serveur.id).cloned().unwrap_or_default(),
        joueurs_max: serveurs::joueurs_max(Path::new(&serveur.dossier)),
        disque_mo: None,
        serveur: serveur.clone(),
    }
}

#[derive(Serialize, Clone)]
struct NouvelleStatut<'a> {
    id: &'a str,
    statut: &'a str,
    depuis: Option<u64>,
}

#[derive(Serialize, Clone)]
struct NouvelleLigne<'a> {
    id: &'a str,
    /// Numéro de la ligne : l'interface s'en sert pour ne rien afficher en double.
    n: u64,
    ligne: &'a str,
}

#[derive(Serialize, Clone)]
struct NouvelleJoueurs<'a> {
    id: &'a str,
    joueurs: &'a [String],
}

/// L'installation de Java pour un serveur : `pourcent` vaut None quand c'est fini.
#[derive(Serialize, Clone)]
struct NouvelleJava<'a> {
    id: &'a str,
    version: u32,
    pourcent: Option<u8>,
}

/// Mémoire et processeur d'un serveur, mesurés toutes les deux secondes.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NouvelleMesure<'a> {
    id: &'a str,
    /// Part du processeur du PC utilisée, de 0 à 100.
    processeur: f32,
    /// Mémoire utilisée, en Mo.
    memoire_mo: u64,
}

/// Les ticks par seconde d'un serveur (20 = tout va bien).
#[derive(Serialize, Clone)]
struct NouvelleTps<'a> {
    id: &'a str,
    tps: f32,
}

/// Regarde si une ligne de console donne les ticks par seconde, et prévient l'interface.
/// Renvoie vrai si la ligne répond à une demande de BHOST : elle ne va pas dans la console.
fn suivre_tps(app: &AppHandle, id: &str, ligne: &str) -> bool {
    let Some(valeur) = tps::lire(ligne) else { return false };
    let pour_bhost = app.state::<Etat>().tps.lock().unwrap().entry(id.to_string()).or_default().recevoir();
    let _ = app.emit("tps", NouvelleTps { id, tps: valeur });
    automatique::tps_recu(app, id, valeur);
    pour_bhost
}

/// Demande ses ticks par seconde au serveur dont l'accueil est affiché (toutes les dix secondes).
fn demander_tps(app: &AppHandle) {
    let id = app.state::<Etat>().tps_regarde.lock().unwrap().clone();
    if let Some(id) = id {
        demander_tps_de(app, &id);
    }
}

/// Demande ses ticks par seconde à un serveur en ligne (au plus toutes les dix secondes).
fn demander_tps_de(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    if etat.statuts.lock().unwrap().get(id).map(String::as_str) != Some("en-ligne") {
        return;
    }
    let logiciel = etat.liste.lock().unwrap().iter().find(|s| s.id == id).map(|s| s.logiciel.clone());
    let Some(commande) = logiciel.as_deref().and_then(tps::commande) else { return };
    let Some(p) = etat.vivants.lock().unwrap().get(id).cloned() else { return };
    if etat.tps.lock().unwrap().entry(id.to_string()).or_default().demander(Instant::now()) {
        let _ = p.ecrire(commande);
    }
}

/// Note l'identifiant d'un joueur (pseudo → identifiant). Au plus 5 000 : des robots
/// qui entrent avec des pseudos au hasard ne remplissent pas la mémoire.
fn noter_identifiant(etat: &Etat, nom: &str, uuid: String, remplacer: bool) {
    let mut identifiants = etat.identifiants.lock().unwrap();
    if identifiants.len() >= 5_000 {
        identifiants.clear();
    }
    let cle = nom.to_ascii_lowercase();
    if remplacer || !identifiants.contains_key(&cle) {
        identifiants.insert(cle, uuid);
    }
}

/// Regarde si une ligne de console annonce une arrivée ou un départ, et prévient l'interface.
fn suivre_joueurs(app: &AppHandle, id: &str, ligne: &str) {
    if let Some((nom, uuid)) = joueurs::lire_identifiant(ligne) {
        noter_identifiant(&app.state::<Etat>(), &nom, uuid, true);
        return;
    }
    let Some(passage) = joueurs::lire(ligne) else { return };
    let arrive = match &passage {
        joueurs::Passage::Arrivee(nom) => Some(nom.clone()),
        joueurs::Passage::Depart(_) => None,
    };
    let etat = app.state::<Etat>();
    {
        let mut arrivees = etat.arrivees.lock().unwrap();
        let du_serveur = arrivees.entry(id.to_string()).or_default();
        match &passage {
            joueurs::Passage::Arrivee(nom) => {
                du_serveur.entry(nom.clone()).or_insert_with(maintenant_ms);
            }
            joueurs::Passage::Depart(nom) => {
                du_serveur.remove(nom);
            }
        }
    }
    {
        let mut tous = etat.joueurs.lock().unwrap();
        let liste = tous.entry(id.to_string()).or_default();
        if joueurs::noter(liste, passage) {
            let _ = app.emit("joueurs", NouvelleJoueurs { id, joueurs: liste });
        }
    }
    if let Some(nom) = arrive {
        automatique::arrivee(app, id, &nom);
    }
}

/// Le fichier des demandes, dans %APPDATA%\BHOST.
const FICHIER_DEMANDES: &str = "demandes.json";

/// Les demandes d'un serveur ont changé : on prévient l'interface.
/// `nouvelle` : un joueur vient d'essayer d'entrer pour la première fois (BHOST le montre tout de suite).
#[derive(Serialize, Clone)]
struct NouvelleDemandes<'a> {
    id: &'a str,
    demandes: Vec<demandes::Demande>,
    nouvelle: Option<demandes::Demande>,
}

fn dire_demandes(app: &AppHandle, id: &str, nouvelle: Option<demandes::Demande>) {
    let etat = app.state::<Etat>();
    let demandes = {
        let registre = etat.demandes.lock().unwrap();
        registre.enregistrer(&etat.dossier.join(FICHIER_DEMANDES));
        registre.liste(id)
    };
    let _ = app.emit("demandes", NouvelleDemandes { id, demandes, nouvelle });
}

/// Regarde si une ligne de console dit qu'un joueur a été refusé par la liste blanche : c'est une demande.
fn suivre_refus(app: &AppHandle, id: &str, ligne: &str) {
    let Some((nom, uuid)) = demandes::lire_refus(ligne) else { return };
    let etat = app.state::<Etat>();
    // Son identifiant servira à l'ajouter à la liste blanche, même serveur arrêté.
    if let Some(u) = &uuid {
        noter_identifiant(&etat, &nom, u.clone(), true);
    }
    // Déjà sur la liste blanche (ajouté à l'instant) : rien à demander.
    let Some(serveur) = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned() else { return };
    if listes::inscrits(Path::new(&serveur.dossier), listes::BLANCHE).iter().any(|b| b.nom.eq_ignore_ascii_case(&nom)) {
        return;
    }
    let note = etat.demandes.lock().unwrap().noter(id, &nom, uuid, maintenant_ms());
    match note {
        demandes::Note::Rien => {}
        demandes::Note::Encore => dire_demandes(app, id, None),
        demandes::Note::Nouvelle => {
            let demande = etat.demandes.lock().unwrap().trouver(id, &nom).cloned();
            dire_demandes(app, id, demande);
            // BHOST est en arrière-plan : une notification Windows, pour ne pas la rater.
            let devant = app.get_webview_window("main").and_then(|f| f.is_focused().ok()).unwrap_or(false);
            if !devant && NOTIFICATIONS.load(Ordering::Relaxed) {
                use tauri_plugin_notification::NotificationExt;
                let _ = app.notification().builder().title(&serveur.nom).body(format!("{nom} veut entrer.")).show();
            }
        }
    }
}

/// Un fil qui mesure, toutes les deux secondes, la mémoire et le processeur des serveurs qui tournent.
/// Il demande aussi les ticks par seconde du serveur affiché.
fn mesurer_en_continu(app: AppHandle) {
    std::thread::spawn(move || {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let coeurs = std::thread::available_parallelism().map(|n| n.get() as f32).unwrap_or(1.0);
        // Si Windows refuse de donner ces mesures, on s'en passe : le reste de BHOST marche.
        let mut systeme = System::new().ok();
        let mut eveille = false;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            demander_tps(&app);
            // Réglages › Empêcher la veille : tant qu'un serveur tourne, Windows ne met pas le PC en veille.
            let garder = VEILLE.load(Ordering::Relaxed) && !app.state::<Etat>().vivants.lock().unwrap().is_empty();
            if garder != eveille {
                eveille = garder;
                empecher_veille(garder);
            }
            let Some(systeme) = systeme.as_mut() else { continue };
            let en_cours: Vec<(String, u32)> =
                app.state::<Etat>().vivants.lock().unwrap().iter().map(|(id, p)| (id.clone(), p.pid)).collect();
            if en_cours.is_empty() {
                continue;
            }
            let pids: Vec<Pid> = en_cours.iter().map(|(_, pid)| Pid::from_u32(*pid)).collect();
            systeme.refresh_processes_specifics(
                ProcessesToUpdate::Some(&pids),
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );
            for (id, pid) in &en_cours {
                if let Some(programme) = systeme.process(Pid::from_u32(*pid)) {
                    let mesure = NouvelleMesure {
                        id,
                        // sysinfo compte 100 % par cœur : on ramène à 100 % pour tout le PC.
                        processeur: (programme.cpu_usage() / coeurs).clamp(0.0, 100.0),
                        memoire_mo: programme.memory() / (1024 * 1024),
                    };
                    automatique::memoire_recue(&app, id, mesure.memoire_mo);
                    let _ = app.emit("mesure", mesure);
                }
            }
        }
    });
}

/// Dit à Windows de ne pas mettre le PC en veille (ou de nouveau le permettre).
/// Ce réglage vaut pour le fil qui l'appelle : c'est le fil des mesures, qui tourne tant que BHOST est ouvert.
/// L'écran, lui, peut toujours s'éteindre.
#[cfg(windows)]
fn empecher_veille(oui: bool) {
    use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
    // SAFETY : simple appel à Windows, sans pointeur.
    unsafe {
        SetThreadExecutionState(if oui { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS });
    }
}

#[cfg(not(windows))]
fn empecher_veille(_oui: bool) {}

/// L'heure qu'il est, en millisecondes depuis 1970.
fn maintenant_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Change l'état d'un serveur et prévient l'interface.
fn dire_statut(app: &AppHandle, id: &str, statut: &str) {
    let etat = app.state::<Etat>();
    etat.statuts.lock().unwrap().insert(id.to_string(), statut.to_string());
    // On note l'heure quand il passe en ligne, et on l'oublie quand il ne l'est plus.
    let depuis = {
        let mut depuis = etat.depuis.lock().unwrap();
        if statut == "en-ligne" {
            let maintenant = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
            Some(*depuis.entry(id.to_string()).or_insert(maintenant))
        } else {
            depuis.remove(id);
            None
        }
    };
    let _ = app.emit("statut", NouvelleStatut { id, statut, depuis });
    zone::rafraichir();
}

/// Ajoute une ligne à la console d'un serveur et prévient l'interface.
pub(crate) fn dire_ligne(app: &AppHandle, id: &str, ligne: &str) {
    let etat = app.state::<Etat>();
    // On garde le verrou pendant l'envoi : les lignes partent dans l'ordre de leur numéro.
    let mut journaux = etat.journaux.lock().unwrap();
    let journal = journaux.entry(id.to_string()).or_default();
    journal.total += 1;
    journal.lignes.push(ligne.to_string());
    if journal.lignes.len() > LIGNES_MAX {
        let trop = journal.lignes.len() - LIGNES_MAX;
        journal.lignes.drain(..trop);
    }
    let _ = app.emit("console", NouvelleLigne { id, n: journal.total, ligne });
}

/// Les options de démarrage « optimisées » d'Aikar (mcflags.emc.gs), sans réserver la mémoire d'avance
/// (pas de -Xms ni d'AlwaysPreTouch) et sans G1RSetUpdatingPauseTimePercent, retiré des Java récents.
const OPTIONS_OPTIMISEES: [&str; 17] = [
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=200",
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+DisableExplicitGC",
    "-XX:G1NewSizePercent=30",
    "-XX:G1MaxNewSizePercent=40",
    "-XX:G1HeapRegionSize=8M",
    "-XX:G1ReservePercent=20",
    "-XX:G1HeapWastePercent=5",
    "-XX:G1MixedGCCountTarget=4",
    "-XX:InitiatingHeapOccupancyPercent=15",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:SurvivorRatio=32",
    "-XX:+PerfDisableSharedMem",
    "-XX:MaxTenuringThreshold=1",
    "-Dusing.aikars.flags=https://mcflags.emc.gs",
];

/// Coupe « -Da=1 "-Dnom=mon serveur" » en morceaux, en gardant ensemble ce qui est entre guillemets.
fn decouper(texte: &str) -> Vec<String> {
    let (mut morceaux, mut courant, mut guillemets, mut vu) = (Vec::new(), String::new(), false, false);
    for c in texte.chars() {
        match c {
            '"' => {
                guillemets = !guillemets;
                vu = true;
            }
            c if c.is_whitespace() && !guillemets => {
                if vu {
                    morceaux.push(std::mem::take(&mut courant));
                    vu = false;
                }
            }
            c => {
                courant.push(c);
                vu = true;
            }
        }
    }
    if vu {
        morceaux.push(courant);
    }
    morceaux
}

/// Ce qu'on donne à Java pour lancer le serveur.
fn arguments_java(serveur: &Serveur) -> Vec<String> {
    // Le maximum de mémoire. On ne réserve rien d'avance (pas de -Xms) :
    // Java prend la mémoire au fur et à mesure, et en laisse au reste du PC.
    let mut arguments = vec![format!("-Xmx{}M", serveur.ram_mo)];
    if serveur.optimise {
        arguments.extend(OPTIONS_OPTIMISEES.iter().map(|o| o.to_string()));
    }
    arguments.extend(decouper(&serveur.arguments));
    // Pas de console « intelligente » : on veut du texte simple, ligne par ligne.
    arguments.push("-Djline.terminal=jline.UnsupportedTerminal".into());
    // Forge et NeoForge récents n'ont pas de .jar : on donne leur fichier d'arguments (« @libraries/…/win_args.txt »).
    if serveur.jar.starts_with('@') {
        arguments.push(serveur.jar.clone());
    } else {
        arguments.extend(["-jar".to_string(), serveur.jar.clone()]);
    }
    arguments.push("nogui".to_string());
    arguments
}

/// Le Java choisi à la main dans Paramètres › Avancé : un numéro (« 21 ») ou le chemin d'un java.exe.
enum ChoixJava {
    Auto,
    Version(u32),
    Fichier(String),
}

fn choix_java(serveur: &Serveur) -> ChoixJava {
    match serveur.java_choix.as_deref() {
        None => ChoixJava::Auto,
        Some(c) => c.parse().map(ChoixJava::Version).unwrap_or_else(|_| ChoixJava::Fichier(c.to_string())),
    }
}

/// Le Java que BHOST choisirait tout seul (selon la version de Minecraft), s'il le sait.
fn java_auto(serveur: &Serveur) -> Option<u32> {
    java::requise(&serveur.version).or(serveur.java)
}

/// Le Java avec lequel ce serveur tourne (`None` : un java.exe choisi à la main, version inconnue).
pub(crate) fn java_du_serveur(serveur: &Serveur) -> Option<u32> {
    match choix_java(serveur) {
        ChoixJava::Auto => java_auto(serveur),
        ChoixJava::Version(n) => Some(n),
        ChoixJava::Fichier(chemin) => java_d_un_programme(Path::new(&chemin)),
    }
}

/// La version d'un java.exe choisi à la main : lue dans le fichier « release » de son dossier
/// (…\jdk-21\bin\java.exe → …\jdk-21\release : JAVA_VERSION="21.0.6", ou "1.8.0_402" pour Java 8).
fn java_d_un_programme(programme: &Path) -> Option<u32> {
    let texte = std::fs::read_to_string(programme.parent()?.parent()?.join("release")).ok()?;
    let valeur = texte.lines().find_map(|l| l.strip_prefix("JAVA_VERSION="))?.trim().trim_matches('"').to_string();
    let mut morceaux = valeur.split(['.', '_', '-', '+']);
    match morceaux.next()?.parse::<u32>().ok()? {
        1 => morceaux.next()?.parse().ok(),
        n => Some(n),
    }
}

/// Démarre un serveur : avec le bon Java s'il est déjà là, sinon après l'avoir téléchargé.
pub(crate) fn lancer_serveur(app: &AppHandle, id: &str) -> Result<(), String> {
    let etat = app.state::<Etat>();
    let Some(serveur) = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned() else {
        return Err("Ce serveur n'existe plus.".into());
    };
    if etat.vivants.lock().unwrap().contains_key(id) {
        return Err("Ce serveur tourne déjà.".into());
    }
    if etat.travaux.lock().unwrap().contains(id) {
        return Err("Attends la fin de l'opération en cours.".into());
    }
    // La carte d'erreur d'avant disparaît (sauf « Java introuvable », qui montre le téléchargement).
    problemes::effacer(app, id, true);
    // Réveil à la connexion : BHOST libère le port avant que le serveur le prenne.
    let _garde = automatique::liberer_port(app, id);
    let version = match choix_java(&serveur) {
        ChoixJava::Fichier(programme) => {
            if !Path::new(&programme).is_file() {
                return Err("Le Java choisi n'existe plus. Change-le dans Paramètres.".into());
            }
            return demarrer_avec(app, id, &serveur, &programme);
        }
        ChoixJava::Version(v) => Some(v),
        ChoixJava::Auto => java_auto(&serveur),
    };
    // Version de Minecraft inconnue : on prend le Java du PC, comme avant.
    let Some(version) = version else {
        return demarrer_avec(app, id, &serveur, "java");
    };
    if let Some(programme) = java::installe(&etat.dossier.join("java"), version, false) {
        return demarrer_avec(app, id, &serveur, &programme.to_string_lossy());
    }
    // Il faut d'abord télécharger Java : ça prend du temps, on le fait à part.
    if !etat.preparation.lock().unwrap().insert(id.to_string()) {
        return Err("Ce serveur démarre déjà.".into());
    }
    dire_statut(app, id, "demarrage");
    let (app, id) = (app.clone(), id.to_string());
    std::thread::spawn(move || {
        let installe = installer_java(&app, &id, version);
        app.state::<Etat>().preparation.lock().unwrap().remove(&id);
        let _ = app.emit("java", NouvelleJava { id: &id, version, pourcent: None });
        let programme = match installe {
            Ok(programme) => {
                dire_ligne(&app, &id, &format!("[BHOST] Java {version} installé."));
                programme.to_string_lossy().into_owned()
            }
            Err(erreur) => {
                // Pas de Java de BHOST : on tente quand même celui du PC.
                dire_ligne(&app, &id, &format!("[BHOST!] Java {version} n'a pas pu être installé : {erreur}"));
                dire_ligne(&app, &id, "[BHOST] Essai avec le Java de ton PC.");
                "java".to_string()
            }
        };
        let _ = demarrer_avec(&app, &id, &serveur, &programme);
    });
    Ok(())
}

/// Télécharge chez Adoptium le Java demandé, et le range dans %APPDATA%\BHOST\java.
fn installer_java(app: &AppHandle, id: &str, version: u32) -> Result<PathBuf, String> {
    java_pret(app, version, false, &mut |pourcent| {
        if pourcent == 0 {
            dire_ligne(app, id, &format!("[BHOST] Ce serveur a besoin de Java {version}. Téléchargement…"));
        }
        let _ = app.emit("java", NouvelleJava { id, version, pourcent: Some(pourcent) });
    })
}

/// Le Java demandé, installé s'il le faut (un téléchargement à la fois).
/// `jdk` : le Java complet, qui sait compiler (pour BuildTools). Sinon le JRE, plus léger, pour les serveurs.
fn java_pret(app: &AppHandle, version: u32, jdk: bool, avancement: &mut dyn FnMut(u8)) -> Result<PathBuf, String> {
    let etat = app.state::<Etat>();
    let racine = etat.dossier.join("java");
    // Si un autre serveur télécharge déjà ce Java, on attend qu'il ait fini.
    let _un_a_la_fois = etat.installation_java.lock().unwrap();
    if let Some(programme) = java::installe(&racine, version, jdk) {
        return Ok(programme);
    }
    avancement(0);
    let agent = etat.reseau.clone();
    let lire_texte = |adresse: &str| reseau::telecharger(&agent, adresse)?.ok_or_else(|| "Adoptium ne répond pas.".to_string());
    let ouvrir = |adresse: &str| reseau::ouvrir(&agent, adresse);
    java::installer(&racine, version, jdk, &lire_texte, &ouvrir, avancement)
}

/// Lance le programme Java d'un serveur et branche sa console sur l'interface.
fn demarrer_avec(app: &AppHandle, id: &str, serveur: &Serveur, programme: &str) -> Result<(), String> {
    let etat = app.state::<Etat>();
    // On garde ce verrou jusqu'au bout : si le serveur meurt tout de suite,
    // sa fin attendra qu'il soit bien noté comme vivant.
    let mut vivants = etat.vivants.lock().unwrap();
    if vivants.contains_key(id) {
        return Err("Ce serveur tourne déjà.".into());
    }

    let arguments = arguments_java(serveur);
    // L'icône dans Minecraft suit celle de BHOST (serveurs d'avant, image retirée…), sauf image de l'utilisateur.
    let icone = serveur.couleur.clone().filter(|c| serveurs::icone_valide(c)).unwrap_or_else(|| icone_jeu::par_defaut(&serveur.logiciel).to_string());
    let _ = icone_jeu::poser(Path::new(&serveur.dossier), &icone, false);
    // Le serveur va charger ses extensions telles qu'elles sont : plus rien à redémarrer.
    catalogue::demarre(app, id);
    // Joueurs Bedrock : le port de Geyser (dans sa config, et en variable si la config n'existe pas encore).
    let dossier = Path::new(&serveur.dossier);
    let variables: Vec<(String, String)> =
        bedrock::preparer(dossier, &serveur.logiciel, &serveur.version, serveur.bedrock_port).map(|p| vec![("GEYSER_BEDROCK_PORT".to_string(), p.to_string())]).unwrap_or_default();

    let (app2, id2) = (app.clone(), id.to_string());
    let ecoute: processus::Ecoute = Arc::new(move |signal| match signal {
        Signal::Ligne(ligne) => {
            // La réponse à une demande de BHOST (ticks par seconde) ne s'affiche pas.
            if !suivre_tps(&app2, &id2, &ligne) {
                dire_ligne(&app2, &id2, &ligne);
                problemes::suivre(&id2, &ligne);
                suivre_joueurs(&app2, &id2, &ligne);
                suivre_refus(&app2, &id2, &ligne);
                catalogue::suivre_pannes(&app2, &id2, &ligne);
            }
        }
        Signal::Pret => {
            dire_statut(&app2, &id2, "en-ligne");
            appliquer_regles(&app2, &id2);
            automatique::pret(&app2, &id2);
        }
        Signal::Fini { propre } => {
            let ancien = app2.state::<Etat>().vivants.lock().unwrap().remove(&id2);
            app2.state::<Etat>().tps.lock().unwrap().remove(&id2);
            // Le monde est enregistré : les règles envoyées y sont écrites.
            app2.state::<Etat>().regles_envoyees.lock().unwrap().remove(&id2);
            app2.state::<Etat>().arrivees.lock().unwrap().remove(&id2);
            // Serveur fermé : plus personne n'est connecté.
            if app2.state::<Etat>().joueurs.lock().unwrap().remove(&id2).is_some_and(|l| !l.is_empty()) {
                let _ = app2.emit("joueurs", NouvelleJoueurs { id: &id2, joueurs: &[] });
            }
            let relancer = ancien.is_some_and(|p| p.relancer.load(Ordering::SeqCst));
            // Des mises à jour de plugins ou mods attendaient l'arrêt (automatisation « Une mise à jour sort »).
            if propre || relancer {
                automatique::arrete(&app2, &id2, relancer);
            }
            if relancer {
                // Redémarrage : on enchaîne sur un nouveau lancement.
                if let Err(erreur) = lancer_serveur(&app2, &id2) {
                    dire_ligne(&app2, &id2, &format!("[BHOST!] {erreur}"));
                    dire_statut(&app2, &id2, "plante");
                    problemes::plante(&app2, &id2);
                }
            } else {
                dire_statut(&app2, &id2, if propre { "arrete" } else { "plante" });
                if !propre {
                    // La carte sur l'Accueil (port pris, mémoire, plantage), puis l'automatisation « Il plante ».
                    problemes::plante(&app2, &id2);
                    automatique::plante(&app2, &id2);
                }
            }
        }
    });

    dire_statut(app, id, "demarrage");
    match processus::lancer(dossier, programme, &arguments, &variables, ecoute) {
        Ok(p) => {
            if let Err(erreur) = p.regler_priorite(serveur.priorite) {
                dire_ligne(app, id, &format!("[BHOST] {erreur}"));
            }
            vivants.insert(id.to_string(), p);
            drop(vivants);
            problemes::effacer(app, id, false);
            Ok(())
        }
        Err(erreur) => {
            drop(vivants);
            dire_ligne(app, id, &format!("[BHOST!] {erreur}"));
            dire_statut(app, id, "arrete");
            // Pas de Java sur le PC : la carte propose de l'installer.
            if programme == "java" && erreur.contains("introuvable") {
                problemes::java(app, id, java_auto(serveur).unwrap_or(21));
            }
            Err(erreur)
        }
    }
}

fn vivant(etat: &Etat, id: &str) -> Result<Arc<Processus>, String> {
    if etat.preparation.lock().unwrap().contains(id) {
        return Err("Attends la fin du téléchargement de Java.".into());
    }
    etat.vivants.lock().unwrap().get(id).cloned().ok_or_else(|| "Ce serveur est arrêté.".to_string())
}

/// La liste des serveurs connus de BHOST, avec leur état.
#[tauri::command]
fn lister_serveurs(etat: State<Etat>) -> Vec<ServeurVu> {
    let mut liste: Vec<ServeurVu> = etat.liste.lock().unwrap().iter().map(|s| vu(&etat, s)).collect();
    // Version de dev : les serveurs de la machine de test « TEST-TEST ».
    for (serveur, statut, depuis, disque_mo) in machine::serveurs_de_test(&etat.dossier) {
        liste.push(ServeurVu { statut: statut.into(), depuis, disque_mo: Some(disque_mo), port: 25565, joueurs: Vec::new(), joueurs_max: 20, serveur });
    }
    liste
}

/// Ajoute à BHOST un serveur qui existe déjà dans un dossier.
#[tauri::command]
fn importer_serveur(dossier: String, etat: State<Etat>) -> Result<ServeurVu, String> {
    let trouve = serveurs::detecter(Path::new(&dossier))?;
    let mut liste = etat.liste.lock().unwrap();
    let serveur = serveurs::ajouter(&mut liste, trouve)?;
    serveurs::enregistrer(&etat.fichier, &liste)?;
    Ok(vu(&etat, &serveur))
}

/// Le refus de démarrer : une phrase, ou (machine dédiée) le manque de mémoire,
/// pour que l'interface propose d'arrêter un autre serveur.
#[derive(Serialize)]
#[serde(untagged)]
enum Refus {
    Phrase(String),
    Memoire(ressources::Manque),
}

#[tauri::command]
fn demarrer_serveur(app: AppHandle, id: String) -> Result<(), Refus> {
    if machine::est_de_test(&id) {
        return machine::demarrer_test(&id).map(|depuis| dire_statut_test(&app, &id, "en-ligne", depuis)).map_err(Refus::Memoire);
    }
    lancer_serveur(&app, &id).map_err(Refus::Phrase)
}

/// Version de dev : un serveur de la machine de test change d'état (rien ne tourne vraiment).
fn dire_statut_test(app: &AppHandle, id: &str, statut: &str, depuis: Option<u64>) {
    let _ = app.emit("statut", NouvelleStatut { id, statut, depuis });
}

/// Carte « Java introuvable » › Installer Java N : si BHOST ne sait pas quel Java il lui faut,
/// on le note dans ses réglages (Paramètres › Java), puis on démarre (le téléchargement se fait au démarrage).
#[tauri::command]
fn installer_java_serveur(app: AppHandle, id: String, version: u32) -> Result<(), String> {
    if ![8, 17, 21, 25].contains(&version) {
        return Err("Version de Java inconnue.".into());
    }
    {
        let etat = app.state::<Etat>();
        let mut liste = etat.liste.lock().unwrap();
        let serveur = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
        if java_auto(serveur) != Some(version) {
            serveur.java_choix = Some(version.to_string());
            serveurs::enregistrer(&etat.fichier, &liste)?;
        }
    }
    lancer_serveur(&app, &id)
}

/// Arrêt propre : on envoie « stop », le serveur sauvegarde puis se ferme.
#[tauri::command]
fn arreter_serveur(app: AppHandle, id: String) -> Result<(), String> {
    if machine::est_de_test(&id) {
        machine::arreter_test(&id);
        dire_statut_test(&app, &id, "arrete", None);
        return Ok(());
    }
    arreter(&app, &id)
}

/// Arrêt propre (bouton Arrêter, menu de la zone de notification).
pub(crate) fn arreter(app: &AppHandle, id: &str) -> Result<(), String> {
    let p = vivant(&app.state::<Etat>(), id)?;
    p.arret_demande.store(true, Ordering::SeqCst);
    p.relancer.store(false, Ordering::SeqCst);
    dire_statut(app, id, "arret");
    p.ecrire("stop")
}

/// Arrêt propre, puis nouveau lancement dès que le serveur est fermé.
#[tauri::command]
fn redemarrer_serveur(app: AppHandle, id: String, etat: State<Etat>) -> Result<(), String> {
    let p = vivant(&etat, &id)?;
    p.arret_demande.store(true, Ordering::SeqCst);
    p.relancer.store(true, Ordering::SeqCst);
    dire_statut(&app, &id, "arret");
    p.ecrire("stop")
}

/// Coupe le serveur tout de suite, sans sauvegarder.
#[tauri::command]
fn forcer_arret(id: String, etat: State<Etat>) -> Result<(), String> {
    let p = vivant(&etat, &id)?;
    p.arret_demande.store(true, Ordering::SeqCst);
    p.relancer.store(false, Ordering::SeqCst);
    p.tuer();
    Ok(())
}

/// Envoie une commande au serveur, comme tapée dans sa console.
#[tauri::command]
fn envoyer_commande(app: AppHandle, id: String, commande: String, etat: State<Etat>) -> Result<(), String> {
    let p = vivant(&etat, &id)?;
    // Plusieurs lignes collées d'un coup : une commande par ligne.
    for commande in commande.lines().map(str::trim).filter(|c| !c.is_empty()) {
        dire_ligne(&app, &id, &format!("> {commande}"));
        p.ecrire(commande)?;
    }
    Ok(())
}

/// L'interface dit quel serveur elle affiche à l'accueil (`actif`), ou qu'elle ne l'affiche plus.
/// BHOST ne demande les ticks par seconde qu'à ce serveur-là, pour ne pas remplir son journal pour rien.
#[tauri::command]
fn regarder_tps(id: String, actif: bool, etat: State<Etat>) {
    let mut regarde = etat.tps_regarde.lock().unwrap();
    if actif {
        *regarde = Some(id);
    } else if regarde.as_deref() == Some(id.as_str()) {
        *regarde = None;
    }
}

/// Les réglages de BHOST gardés par Rust : %APPDATA%\BHOST\reglages.json.
#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReglagesBhost {
    /// Où créer les nouveaux serveurs. Absent : %APPDATA%\BHOST\serveurs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dossier_serveurs: Option<String>,
    /// Réglages › Notifications.
    #[serde(default = "oui")]
    notifications: bool,
    /// Réglages › Empêcher la veille.
    #[serde(default = "oui")]
    veille: bool,
    /// Réglages › Garder les serveurs allumés.
    #[serde(default = "oui")]
    garder: bool,
    /// Réglages › Arrêter proprement.
    #[serde(default = "oui")]
    proprement: bool,
    /// Réglages › Ton pseudo Minecraft : OP sur chaque nouveau serveur.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pseudo: Option<String>,
}

fn oui() -> bool {
    true
}

impl Default for ReglagesBhost {
    fn default() -> Self {
        ReglagesBhost { dossier_serveurs: None, notifications: true, veille: true, garder: true, proprement: true, pseudo: None }
    }
}

fn lire_reglages(etat: &Etat) -> ReglagesBhost {
    std::fs::read(etat.dossier.join("reglages.json")).ok().and_then(|o| serde_json::from_slice(&o).ok()).unwrap_or_default()
}

fn ecrire_reglages(etat: &Etat, reglages: &ReglagesBhost) -> Result<(), String> {
    let texte = serde_json::to_vec_pretty(reglages).map_err(|e| e.to_string())?;
    garde::ecrire_atomique(&etat.dossier.join("reglages.json"), &texte).map_err(|_| "Impossible d'enregistrer le réglage.".to_string())
}

fn dossier_serveurs(etat: &Etat) -> PathBuf {
    lire_reglages(etat).dossier_serveurs.map(PathBuf::from).unwrap_or_else(|| etat.dossier.join("serveurs"))
}

/// Ce que l'assistant de création affiche : dossier des serveurs, place libre, mémoire du PC.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InfosCreation {
    dossier: String,
    /// Disque du dossier : « C: », place libre et totale (en Go).
    disque: String,
    libre_go: f64,
    total_go: f64,
    /// Mémoire du PC, en Go.
    memoire_go: f64,
    /// Vrai si BHOST n'a encore aucun serveur : l'assistant commence par le dossier.
    premier: bool,
}

#[tauri::command]
fn infos_creation(etat: State<Etat>) -> InfosCreation {
    use sysinfo::{Disks, System};
    let dossier = dossier_serveurs(&etat);
    let go = |octets: u64| (octets as f64 / 1_073_741_824.0 * 10.0).round() / 10.0;
    // Le disque qui contient le dossier : celui dont le point de montage est le plus long préfixe.
    let (mut disque, mut libre, mut total) = (String::new(), 0, 0);
    if let Ok(disques) = Disks::new_with_refreshed_list() {
        if let Some(d) = disques.iter().filter(|d| dossier.starts_with(d.mount_point())).max_by_key(|d| d.mount_point().as_os_str().len()) {
            disque = d.mount_point().to_string_lossy().trim_end_matches(['\\', '/']).to_string();
            libre = d.available_space();
            total = d.total_space();
        }
    }
    let memoire = System::new().map(|mut s| {
        s.refresh_memory();
        s.total_memory()
    });
    InfosCreation {
        dossier: dossier.to_string_lossy().into_owned(),
        disque,
        libre_go: go(libre),
        total_go: go(total),
        memoire_go: go(memoire.unwrap_or(0)),
        premier: etat.liste.lock().unwrap().is_empty(),
    }
}

/// Change le dossier où BHOST crée les nouveaux serveurs.
#[tauri::command]
fn changer_dossier_serveurs(dossier: String, etat: State<Etat>) -> Result<(), String> {
    if !Path::new(&dossier).is_dir() {
        return Err("Ce dossier n'existe pas.".into());
    }
    let mut reglages = lire_reglages(&etat);
    reglages.dossier_serveurs = Some(dossier);
    ecrire_reglages(&etat, &reglages)
}

// ---------------------------------------------------------------------------------------------
// Réglages de BHOST (bouton en bas de la barre latérale)
// ---------------------------------------------------------------------------------------------

/// Ce que la page Réglages affiche.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VueReglages {
    lancer_windows: bool,
    garder: bool,
    proprement: bool,
    veille: bool,
    notifications: bool,
    pseudo: String,
    /// Le dossier des serveurs, combien de serveurs de BHOST il contient, et sa taille (octets).
    dossier: String,
    serveurs: usize,
    octets: u64,
    /// La version de BHOST (« 0.1.0 »).
    version: String,
}

/// Un changement de la page Réglages : seuls les champs donnés changent.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangementReglages {
    lancer_windows: Option<bool>,
    garder: Option<bool>,
    proprement: Option<bool>,
    veille: Option<bool>,
    notifications: Option<bool>,
    pseudo: Option<String>,
}

/// La taille d'un dossier, avec tout ce qu'il contient (les raccourcis ne sont pas suivis).
fn taille_dossier(dossier: &Path) -> u64 {
    mondes::taille(dossier)
}

fn vue_reglages(app: &AppHandle) -> VueReglages {
    use tauri_plugin_autostart::ManagerExt;
    let etat = app.state::<Etat>();
    let reglages = lire_reglages(&etat);
    let dossier = dossier_serveurs(&etat);
    let serveurs = etat.liste.lock().unwrap().iter().filter(|s| Path::new(&s.dossier).starts_with(&dossier)).count();
    VueReglages {
        lancer_windows: app.autolaunch().is_enabled().unwrap_or(false),
        garder: reglages.garder,
        proprement: reglages.proprement,
        veille: reglages.veille,
        notifications: reglages.notifications,
        pseudo: reglages.pseudo.unwrap_or_default(),
        octets: taille_dossier(&dossier),
        dossier: dossier.to_string_lossy().into_owned(),
        serveurs,
        version: app.package_info().version.to_string(),
    }
}

#[tauri::command]
async fn reglages_bhost(app: AppHandle) -> Result<VueReglages, String> {
    // Mesurer le dossier des serveurs peut prendre un moment (gros mondes) : on le fait à part.
    a_part(move || Ok(vue_reglages(&app))).await
}

#[tauri::command]
async fn regler_bhost(app: AppHandle, changement: ChangementReglages) -> Result<VueReglages, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        if let Some(oui) = changement.lancer_windows {
            use tauri_plugin_autostart::ManagerExt;
            let lanceur = app.autolaunch();
            let fait = if oui { lanceur.enable() } else { lanceur.disable() };
            fait.map_err(|_| "Windows a refusé le changement.".to_string())?;
        }
        let mut reglages = lire_reglages(&etat);
        if let Some(oui) = changement.veille {
            reglages.veille = oui;
            VEILLE.store(oui, Ordering::Relaxed);
        }
        if let Some(oui) = changement.garder {
            reglages.garder = oui;
            GARDER.store(oui, Ordering::Relaxed);
        }
        if let Some(oui) = changement.proprement {
            reglages.proprement = oui;
            PROPREMENT.store(oui, Ordering::Relaxed);
        }
        if let Some(oui) = changement.notifications {
            reglages.notifications = oui;
            NOTIFICATIONS.store(oui, Ordering::Relaxed);
        }
        if let Some(pseudo) = changement.pseudo {
            let pseudo = pseudo.trim().to_string();
            if !pseudo.is_empty() && !listes::pseudo_valide(&pseudo) {
                return Err("Pseudo invalide.".into());
            }
            reglages.pseudo = (!pseudo.is_empty()).then_some(pseudo);
        }
        ecrire_reglages(&etat, &reglages)?;
        Ok(vue_reglages(&app))
    })
    .await
}

/// Réglages › Dossier des serveurs › Ouvrir.
#[tauri::command]
fn ouvrir_dossier_serveurs(app: AppHandle, etat: State<Etat>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dossier = dossier_serveurs(&etat);
    std::fs::create_dir_all(&dossier).map_err(|_| "Le dossier n'a pas pu être créé.".to_string())?;
    app.opener().open_path(dossier.to_string_lossy(), None::<&str>).map_err(|_| "Le dossier n'a pas pu s'ouvrir.".to_string())
}

/// Les versions d'avant la 1.7.6 gardent les opérateurs dans ops.txt (un pseudo par ligne).
fn ops_en_texte(version: &str) -> bool {
    let n: Vec<u32> = version.split('.').map_while(|x| x.parse().ok()).collect();
    match n.as_slice() {
        [1, m, ..] if *m < 7 => true,
        [1, 7] => true,
        [1, 7, p, ..] => *p < 6,
        _ => false,
    }
}

/// Réglages › Ton pseudo Minecraft : il devient OP sur le serveur qui vient d'être créé.
/// Sans Internet (identifiant introuvable), on passe : il pourra le faire dans l'onglet Joueurs.
fn pseudo_op(app: &AppHandle, dossier: &Path, version: &str) {
    let Some(pseudo) = lire_reglages(&app.state::<Etat>()).pseudo else { return };
    if ops_en_texte(version) {
        let _ = garde::ecrire_atomique(&dossier.join("ops.txt"), format!("{pseudo}\n").as_bytes());
        return;
    }
    let provisoire = Serveur { dossier: dossier.to_string_lossy().into_owned(), ..Default::default() };
    if let Ok(Some(uuid)) = uuid_joueur(app, &provisoire, &pseudo, true) {
        let _ = listes::modifier(dossier, listes::OPS, listes::Changement::Ajouter { nom: &pseudo, uuid: &uuid, motif: None }, SystemTime::now());
    }
}

/// Les versions de Minecraft qu'on peut installer pour un logiciel.
#[tauri::command]
async fn versions_minecraft(app: AppHandle, logiciel: String, snapshots: bool) -> Result<Vec<creation::VersionMc>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if logiciel == "Spigot" {
            return versions_spigot(&app);
        }
        let adresse = creation::adresse_liste(&logiciel).or_else(|| installeur::adresse_liste(&logiciel)).ok_or("Logiciel pas encore pris en charge.")?;
        let reponse = reseau::telecharger(&app.state::<Etat>().reseau, adresse)
            .map_err(|_| "Pas de connexion. Réessaie.".to_string())?
            .ok_or("Liste des versions introuvable.")?;
        if installeur::LOGICIELS.contains(&logiciel.as_str()) {
            installeur::lire_versions(&logiciel, &reponse, snapshots)
        } else {
            creation::lire_versions(&logiciel, &reponse, snapshots)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Les versions que BuildTools sait fabriquer. Si la page de SpigotMC ne répond pas : celles de Mojang depuis la 1.8.
fn versions_spigot(app: &AppHandle) -> Result<Vec<creation::VersionMc>, String> {
    let agent = app.state::<Etat>().reseau.clone();
    if let Ok(Some(page)) = reseau::telecharger(&agent, spigot::ADRESSE_VERSIONS) {
        if let Ok(liste) = spigot::lire_versions(&page) {
            return Ok(liste);
        }
    }
    let mojang = reseau::telecharger(&agent, creation::adresse_liste("Vanilla").unwrap())
        .map_err(|_| "Pas de connexion. Réessaie.".to_string())?
        .ok_or("Liste des versions introuvable.")?;
    Ok(spigot::depuis_mojang(creation::lire_versions("Vanilla", &mojang, false)?))
}

/// Ce que l'assistant demande de créer.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DemandeCreation {
    nom: String,
    logiciel: String,
    version: String,
    ram_go: u32,
    eula: bool,
    /// Couleur du cube (absente : selon le logiciel).
    #[serde(default)]
    couleur: Option<String>,
    /// Ouvert à tes amis dès le départ (changeable ensuite dans Paramètres).
    #[serde(default)]
    ouvert_amis: bool,
    /// Machine dédiée choisie à gauche (absente : ce PC).
    #[serde(default)]
    machine: Option<String>,
    #[serde(flatten)]
    monde: creation::Monde,
}

/// L'avancement de la création : tâche 0 = Java, 1 = serveur, 2 = monde.
#[derive(Serialize, Clone)]
struct NouvelleCreation {
    tache: u8,
    pourcent: u8,
}

/// Crée un serveur neuf : Java si besoin, téléchargement du serveur, fichiers du monde, ajout à la liste.
#[tauri::command]
async fn creer_serveur(app: AppHandle, demande: DemandeCreation) -> Result<ServeurVu, String> {
    tauri::async_runtime::spawn_blocking(move || creer(&app, demande)).await.map_err(|e| e.to_string())?
}

fn creer(app: &AppHandle, demande: DemandeCreation) -> Result<ServeurVu, String> {
    if demande.machine.is_some() {
        // La machine installe elle-même le serveur : ça arrive avec BHOST OS.
        return Err("Bientôt : créer un serveur sur une machine dédiée.".into());
    }
    let nom = demande.nom.trim().to_string();
    if nom.is_empty() || nom.chars().count() > 40 {
        return Err("Donne-lui un nom (40 lettres au plus).".into());
    }
    if !demande.eula {
        return Err("Il faut accepter le contrat de Minecraft.".into());
    }
    if !creation::LOGICIELS.contains(&demande.logiciel.as_str()) && !installeur::LOGICIELS.contains(&demande.logiciel.as_str()) && demande.logiciel != "Spigot" {
        return Err("Logiciel pas encore pris en charge.".into());
    }
    if demande.couleur.as_deref().is_some_and(|c| !serveurs::icone_valide(c)) {
        return Err("Couleur inconnue.".into());
    }
    let etat = app.state::<Etat>();
    let dire = |tache: u8, pourcent: u8| {
        let _ = app.emit("creation", NouvelleCreation { tache, pourcent });
    };

    // 1. Un dossier neuf. 2. Java et le serveur (téléchargé, ou installé par son installateur).
    let dossier = creation::creer_dossier(&dossier_serveurs(&etat), &nom)?;
    let resultat = (|| {
        let (jar, java_snapshot) = installer_logiciel(app, &demande.logiciel, &demande.version, &dossier, None, &mut |tache, p| dire(tache, p))?;

        // 3. Le monde : eula.txt et server.properties, puis le serveur entre dans la liste de BHOST.
        let pris: Vec<u16> = etat.liste.lock().unwrap().iter().map(|s| serveurs::port(Path::new(&s.dossier))).collect();
        creation::ecrire_fichiers(&dossier, &nom, &demande.version, &demande.monde, creation::port_libre(&pris))?;
        pseudo_op(app, &dossier, &demande.version);
        let icone = demande.couleur.clone().unwrap_or_else(|| icone_jeu::par_defaut(&demande.logiciel).to_string());
        let _ = icone_jeu::poser(&dossier, &icone, false);
        let serveur = Serveur {
            id: String::new(),
            nom: nom.clone(),
            dossier: dossier.to_string_lossy().into_owned(),
            jar,
            logiciel: demande.logiciel.clone(),
            version: demande.version.clone(),
            ram_mo: demande.ram_go.clamp(1, 64) * 1024,
            couleur: demande.couleur.clone(),
            java: java_snapshot,
            ouvert_amis: demande.ouvert_amis,
            ..Default::default()
        };
        let mut liste = etat.liste.lock().unwrap();
        let serveur = serveurs::ajouter(&mut liste, serveur)?;
        serveurs::enregistrer(&etat.fichier, &liste)?;
        drop(liste);
        if serveur.ouvert_amis {
            let _ = app.emit("acces", etat.acces.reveiller(&serveur.id, true));
        }
        dire(2, 100);
        Ok(vu(&etat, &serveur))
    })();
    if resultat.is_err() {
        // Ce dossier vient d'être créé par BHOST pour ce serveur : on ne laisse pas un serveur à moitié fait.
        let _ = std::fs::remove_dir_all(&dossier);
    }
    resultat
}

/// Met le logiciel `logiciel` en version `version` dans `dossier` : Java d'abord (tâche 0), puis le serveur (tâche 1).
/// Vanilla, Paper, Purpur, Mohist, Youer, Arclight : un .jar téléchargé. Fabric, Forge, NeoForge : leur installateur, lancé dans le dossier.
/// Renvoie ce qu'il faut lancer, et le Java à retenir pour une snapshot (sa version ne le dit pas).
/// `jar_actuel` : en changeant de version, le fichier lancé jusqu'ici (Arclight garde son moteur de mods).
fn installer_logiciel(app: &AppHandle, logiciel: &str, version: &str, dossier: &Path, jar_actuel: Option<&str>, avance: &mut dyn FnMut(u8, u8)) -> Result<(String, Option<u32>), String> {
    let agent = app.state::<Etat>().reseau.clone();
    let lire = |adresse: &str| reseau::telecharger(&agent, adresse).map_err(|_| "Pas de connexion. Réessaie.".to_string())?.ok_or_else(|| "Introuvable.".to_string());
    if creation::LOGICIELS.contains(&logiciel) {
        let moteur = jar_actuel.filter(|_| logiciel == "Arclight").map(extensions::moteur_arclight);
        let fichier = creation::trouver_fichier_pour(logiciel, version, moteur, &lire)?;
        let java = java::requise(version).or(fichier.java);
        if let Some(j) = java {
            java_pret(app, j, false, &mut |p| avance(0, p)).map_err(|e| format!("Java {j} n'a pas pu être installé : {e}"))?;
        }
        avance(0, 100);
        let flux = reseau::ouvrir(&agent, &fichier.adresse)?;
        // « server.jar », ou le vrai nom pour Arclight (il dit son moteur de mods : Forge, NeoForge ou Fabric).
        let jar = creation::nom_du_jar(logiciel, &fichier);
        creation::telecharger(flux, &fichier, &dossier.join(&jar), &mut |p| avance(1, p))?;
        return Ok((jar, java.filter(|_| java::requise(version).is_none())));
    }
    if logiciel == "Spigot" {
        return fabriquer_spigot(app, version, dossier, avance);
    }
    if !installeur::LOGICIELS.contains(&logiciel) {
        return Err("Logiciel pas encore pris en charge.".into());
    }
    let installation = installeur::trouver(logiciel, version, &lire)?;
    // Fabric garde une copie de travail du jeu (.fabric) : à jeter quand la version change (sinon erreurs au démarrage).
    if logiciel == "Fabric" {
        let _ = std::fs::remove_dir_all(dossier.join(".fabric"));
    }
    // L'installateur tourne avec le Java de cette version de Minecraft (25 pour une snapshot récente).
    let j = java::requise(version).or_else(|| java::requise(version.split('-').next().unwrap_or(""))).unwrap_or(25);
    let programme = java_pret(app, j, false, &mut |p| avance(0, p)).map_err(|e| format!("Java {j} n'a pas pu être installé : {e}"))?;
    avance(0, 100);
    let installateur = "bhost-installateur.jar";
    let flux = reseau::ouvrir(&agent, &installation.fichier.adresse)?;
    // Téléchargement : jusqu'à 30 %. L'installation elle-même (une à trois minutes) fait le reste.
    creation::telecharger(flux, &installation.fichier, &dossier.join(installateur), &mut |p| avance(1, (u16::from(p) * 30 / 100) as u8))?;
    avance(1, 30);
    let fait = installeur::executer(&programme.to_string_lossy(), dossier, installateur, &installation.arguments);
    let _ = std::fs::remove_file(dossier.join(installateur));
    let _ = std::fs::remove_file(dossier.join(format!("{installateur}.log")));
    fait?;
    let lancement = installeur::lancement(dossier, logiciel, &installation.build).ok_or("Installation incomplète. Détails : bhost-installation.log.")?;
    let _ = std::fs::remove_file(dossier.join("bhost-installation.log"));
    avance(1, 100);
    Ok((lancement, Some(j).filter(|_| java::requise(version).is_none())))
}

/// Fabrique Spigot avec BuildTools (dossier de travail gardé dans %APPDATA%\BHOST\buildtools), directement en server.jar.
fn fabriquer_spigot(app: &AppHandle, version: &str, dossier: &Path, avance: &mut dyn FnMut(u8, u8)) -> Result<(String, Option<u32>), String> {
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(format!("Spigot {version} introuvable."));
    }
    let etat = app.state::<Etat>();
    // BuildTools tourne avec le Java de cette version de Minecraft, en JDK : il compile Spigot.
    let j = java::requise(version).unwrap_or(25);
    let programme = java_pret(app, j, true, &mut |p| avance(0, p)).map_err(|e| format!("Java {j} n'a pas pu être installé : {e}"))?;
    avance(0, 100);
    let _une_a_la_fois = etat.buildtools.lock().unwrap();
    let atelier = etat.dossier.join("buildtools");
    std::fs::create_dir_all(&atelier).map_err(|e| e.to_string())?;
    // L'outil lui-même : retéléchargé s'il a plus d'une semaine.
    let outil = atelier.join("BuildTools.jar");
    let vieux = std::fs::metadata(&outil).and_then(|m| m.modified()).map(|t| t.elapsed().unwrap_or_default() > Duration::from_secs(7 * 86_400)).unwrap_or(true);
    if vieux {
        let partiel = atelier.join("BuildTools.jar.part");
        let resultat = (|| {
            let mut flux = reseau::ouvrir(&etat.reseau, spigot::ADRESSE_BUILDTOOLS).map_err(|_| "SpigotMC ne répond pas. Réessaie.".to_string())?;
            let mut sortie = std::fs::File::create(&partiel).map_err(|e| e.to_string())?;
            const MAX: u64 = 64 * 1024 * 1024;
            let n = std::io::copy(&mut std::io::Read::take(&mut flux, MAX + 1), &mut sortie).map_err(|e| format!("Téléchargement coupé ({e})."))?;
            drop(sortie);
            if n > MAX {
                return Err("BuildTools est trop gros : téléchargement refusé.".to_string());
            }
            std::fs::rename(&partiel, &outil).map_err(|e| e.to_string())
        })();
        if resultat.is_err() {
            let _ = std::fs::remove_file(&partiel);
            if !outil.is_file() {
                return resultat.map(|_| (String::new(), None));
            }
        }
    }
    avance(1, 2);
    let _ = std::fs::remove_file(dossier.join("server.jar"));
    let journal = dossier.join("bhost-installation.log");
    let mut meilleur = 2u8;
    installeur::executer_suivi(
        &programme.to_string_lossy(),
        &atelier,
        &outil.to_string_lossy(),
        &spigot::arguments(version, &dossier.to_string_lossy()),
        spigot::DUREE_MAX_MINUTES,
        &journal,
        &mut |ligne| {
            if let Some(p) = spigot::avancement(ligne).filter(|p| *p > meilleur) {
                meilleur = p;
                avance(1, p.min(99));
            }
        },
    )?;
    if !dossier.join("server.jar").is_file() {
        return Err("BuildTools n'a pas donné de serveur. Détails : bhost-installation.log.".into());
    }
    let _ = std::fs::remove_file(journal);
    avance(1, 100);
    Ok(("server.jar".into(), None))
}

/// Change le nom d'un serveur dans BHOST (le dossier, lui, ne change pas).
#[tauri::command]
fn renommer_serveur(id: String, nom: String, etat: State<Etat>) -> Result<ServeurVu, String> {
    let nom = nom.trim();
    if nom.is_empty() {
        return Err("Donne-lui un nom.".into());
    }
    if nom.chars().count() > 40 {
        return Err("40 lettres au plus.".into());
    }
    let mut liste = etat.liste.lock().unwrap();
    let serveur = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
    serveur.nom = nom.to_string();
    let copie = serveur.clone();
    serveurs::enregistrer(&etat.fichier, &liste)?;
    Ok(vu(&etat, &copie))
}

/// Ce que l'onglet Paramètres affiche en plus du serveur lui-même.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Parametres {
    /// Mode, difficulté et joueurs, lus dans server.properties.
    partie: creation::Monde,
    /// Le Java que BHOST prendrait tout seul (« Auto (Java 21) »), s'il le sait.
    java_auto: Option<u32>,
    /// La commande lancée, pour l'aperçu.
    commande: String,
    /// Mémoire du PC, en Go.
    memoire_go: f64,
}

fn parametres(serveur: &Serveur) -> Parametres {
    let memoire = sysinfo::System::new().map(|mut s| {
        s.refresh_memory();
        s.total_memory()
    });
    let programme = match choix_java(serveur) {
        ChoixJava::Fichier(f) => format!("\"{f}\""),
        _ => "java".to_string(),
    };
    let arguments: Vec<String> = arguments_java(serveur).into_iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a }).collect();
    Parametres {
        partie: creation::lire_partie(&serveurs::lire_proprietes(Path::new(&serveur.dossier))),
        java_auto: java_auto(serveur),
        commande: format!("{programme} {}", arguments.join(" ")),
        memoire_go: (memoire.unwrap_or(0) as f64 / 1_073_741_824.0 * 10.0).round() / 10.0,
    }
}

#[tauri::command]
fn parametres_serveur(id: String, etat: State<Etat>) -> Result<Parametres, String> {
    let serveur = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned().ok_or("Ce serveur n'existe plus.")?;
    Ok(parametres(&serveur))
}

/// Les réglages d'un serveur gardés par BHOST (pas dans server.properties).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReglagesServeur {
    ram_mo: u32,
    demarrer_auto: bool,
    /// Absent : automatique. « 8 », « 17 », « 21 », « 25 », ou le chemin d'un java.exe.
    java_choix: Option<String>,
    optimise: bool,
    arguments: String,
    #[serde(default)]
    priorite: processus::Priorite,
}

/// Enregistre les réglages d'un serveur. Ils comptent au prochain démarrage.
#[tauri::command]
fn regler_serveur(id: String, reglages: ReglagesServeur, etat: State<Etat>) -> Result<(ServeurVu, Parametres), String> {
    if let Some(choix) = &reglages.java_choix {
        let numero_ok = choix.parse::<u32>().is_ok_and(|n| [8, 17, 21, 25].contains(&n));
        if !numero_ok && !Path::new(choix).is_file() {
            return Err("Ce fichier Java n'existe pas.".into());
        }
    }
    if reglages.arguments.chars().count() > 500 || reglages.arguments.contains(['\n', '\r']) {
        return Err("Arguments trop longs.".into());
    }
    let mut liste = etat.liste.lock().unwrap();
    let serveur = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
    serveur.ram_mo = reglages.ram_mo.clamp(512, 256 * 1024);
    serveur.demarrer_auto = reglages.demarrer_auto;
    serveur.java_choix = reglages.java_choix;
    serveur.optimise = reglages.optimise;
    serveur.arguments = reglages.arguments.trim().to_string();
    serveur.priorite = reglages.priorite;
    let copie = serveur.clone();
    serveurs::enregistrer(&etat.fichier, &liste)?;
    drop(liste);
    // La priorité change tout de suite si le serveur tourne.
    if let Some(p) = etat.vivants.lock().unwrap().get(&id).cloned() {
        p.regler_priorite(copie.priorite)?;
    }
    Ok((vu(&etat, &copie), parametres(&copie)))
}

/// Change le mode, la difficulté et le nombre de joueurs dans server.properties.
/// Si le serveur tourne, la difficulté change tout de suite ; le reste au prochain démarrage.
#[tauri::command]
fn regler_partie(app: AppHandle, id: String, partie: creation::Monde, etat: State<Etat>) -> Result<ServeurVu, String> {
    let serveur = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned().ok_or("Ce serveur n'existe plus.")?;
    let avant = creation::lire_partie(&serveurs::lire_proprietes(Path::new(&serveur.dossier)));
    serveurs::modifier_proprietes(Path::new(&serveur.dossier), &creation::valeurs_partie(&serveur.version, &partie))?;
    let apres = creation::lire_partie(&serveurs::lire_proprietes(Path::new(&serveur.dossier)));
    if apres.difficulte != avant.difficulte && etat.statuts.lock().unwrap().get(&id).is_some_and(|s| s == "en-ligne") {
        let mot = match apres.difficulte.as_str() {
            "paisible" => "peaceful",
            "facile" => "easy",
            "difficile" => "hard",
            _ => "normal",
        };
        if let Ok(p) = vivant(&etat, &id) {
            dire_ligne(&app, &id, &format!("> difficulty {mot}"));
            let _ = p.ecrire(&format!("difficulty {mot}"));
        }
    }
    Ok(vu(&etat, &serveur))
}

/// Avancement d'une copie de serveur.
#[derive(Serialize, Clone)]
struct NouvelleCopie<'a> {
    id: &'a str,
    pourcent: u8,
}

/// Copie un serveur arrêté dans un nouveau dossier (dossier des serveurs de BHOST), avec un autre port.
#[tauri::command]
async fn dupliquer_serveur(app: AppHandle, id: String) -> Result<ServeurVu, String> {
    tauri::async_runtime::spawn_blocking(move || dupliquer(&app, &id)).await.map_err(|_| "Ça n'a pas marché.".to_string())?
}

fn dupliquer(app: &AppHandle, id: &str) -> Result<ServeurVu, String> {
    let etat = app.state::<Etat>();
    arrete(&etat, id)?;
    if etat.travaux.lock().unwrap().contains(id) {
        return Err("Attends la fin de l'opération en cours.".into());
    }
    let source = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned().ok_or("Ce serveur n'existe plus.")?;
    let dossier_source = PathBuf::from(&source.dossier);
    if !dossier_source.is_dir() {
        return Err("Son dossier n'existe plus.".into());
    }
    if !etat.copies.lock().unwrap().insert(id.to_string()) {
        return Err("Copie déjà en cours.".into());
    }
    let nom = copie::nom_de_copie(&source.nom);
    // La copie va dans le dossier des serveurs de BHOST (pas à côté de l'original, qui peut être sur le Bureau).
    let resultat = creation::creer_dossier(&dossier_serveurs(&etat), &nom).and_then(|cible| {
        let fait = (|| {
            copie::copier(&dossier_source, &cible, &mut |pourcent| {
                let _ = app.emit("copie", NouvelleCopie { id, pourcent });
            })?;
            let pris: Vec<u16> = etat.liste.lock().unwrap().iter().map(|s| serveurs::port(Path::new(&s.dossier))).collect();
            serveurs::changer_port(&cible, creation::port_libre(&pris))?;
            let nouveau = Serveur { id: String::new(), nom: nom.clone(), dossier: cible.to_string_lossy().into_owned(), demarrer_auto: false, ..source.clone() };
            let mut liste = etat.liste.lock().unwrap();
            let nouveau = serveurs::ajouter(&mut liste, nouveau)?;
            serveurs::enregistrer(&etat.fichier, &liste)?;
            drop(liste);
            Ok(vu(&etat, &nouveau))
        })();
        if fait.is_err() {
            // Ce dossier vient d'être créé par BHOST pour la copie : on ne laisse pas une copie à moitié faite.
            let _ = std::fs::remove_dir_all(&cible);
        }
        fait
    });
    etat.copies.lock().unwrap().remove(id);
    resultat
}

/// Change l'icône d'un serveur (clic droit › Icône…).
#[tauri::command]
fn couleur_serveur(id: String, couleur: String, etat: State<Etat>) -> Result<ServeurVu, String> {
    if !serveurs::icone_valide(&couleur) {
        return Err("Icône inconnue.".into());
    }
    let mut liste = etat.liste.lock().unwrap();
    let serveur = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
    serveur.couleur = Some(couleur.clone());
    let copie = serveur.clone();
    serveurs::enregistrer(&etat.fichier, &liste)?;
    drop(liste);
    // La même icône dans la liste des serveurs de Minecraft (sauf si l'utilisateur y a mis sa propre image).
    let _ = icone_jeu::poser(Path::new(&copie.dossier), &couleur, false);
    Ok(vu(&etat, &copie))
}

/// Supprime un serveur : son dossier va à la Corbeille, et BHOST l'oublie.
#[tauri::command]
async fn supprimer_serveur(app: AppHandle, id: String) -> Result<(), String> {
    // Mettre un gros monde à la Corbeille prend du temps : on le fait à part, pour ne pas figer BHOST.
    tauri::async_runtime::spawn_blocking(move || {
        supprimer(&app.state::<Etat>(), &id, true)?;
        automatique::oublier(&app, &id);
        oublier_demandes(&app, &id);
        catalogue::oublier(&app, &id);
        Ok(())
    })
    .await
    .map_err(|_| "Ça n'a pas marché.".to_string())?
}

/// Retire un serveur de BHOST sans toucher à son dossier (on peut le réimporter plus tard).
#[tauri::command]
fn retirer_serveur(app: AppHandle, id: String, etat: State<Etat>) -> Result<(), String> {
    supprimer(&etat, &id, false)?;
    automatique::oublier(&app, &id);
    oublier_demandes(&app, &id);
    catalogue::oublier(&app, &id);
    Ok(())
}

/// Un serveur supprimé ou retiré : ses demandes partent avec lui.
fn oublier_demandes(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    let mut registre = etat.demandes.lock().unwrap();
    registre.oublier(id);
    registre.enregistrer(&etat.dossier.join(FICHIER_DEMANDES));
}

/// `jeter` : le dossier va à la Corbeille. Sinon, BHOST l'oublie seulement.
fn supprimer(etat: &Etat, id: &str, jeter: bool) -> Result<(), String> {
    arrete(etat, id)?;
    if etat.travaux.lock().unwrap().contains(id) {
        return Err("Attends la fin de l'opération en cours.".into());
    }
    let serveur = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned().ok_or("Ce serveur n'existe plus.")?;
    let dossier = Path::new(&serveur.dossier);
    if jeter && dossier.exists() {
        if !serveurs::dossier_de_serveur(&serveur) {
            return Err("Ce dossier ne ressemble pas à un serveur. Supprime-le toi-même.".into());
        }
        corbeille::jeter(dossier)?;
    }
    let mut liste = etat.liste.lock().unwrap();
    liste.retain(|s| s.id != id);
    problemes::oublier(&id);
    serveurs::enregistrer(&etat.fichier, &liste)?;
    drop(liste);
    etat.statuts.lock().unwrap().remove(id);
    etat.journaux.lock().unwrap().remove(id);
    etat.depuis.lock().unwrap().remove(id);
    etat.joueurs.lock().unwrap().remove(id);
    etat.tps.lock().unwrap().remove(id);
    Ok(())
}

/// Ouvre le dossier d'un serveur dans l'Explorateur de fichiers.
#[tauri::command]
fn ouvrir_dossier(app: AppHandle, id: String, etat: State<Etat>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dossier = etat.liste.lock().unwrap().iter().find(|s| s.id == id).map(|s| s.dossier.clone()).ok_or("Ce serveur n'existe plus.")?;
    if !Path::new(&dossier).is_dir() {
        return Err("Ce dossier n'existe plus.".into());
    }
    app.opener().open_path(dossier, None::<&str>).map_err(|_| "Le dossier n'a pas pu s'ouvrir.".to_string())
}

/// La tête d'un joueur (son visage, 8 × 8 pixels), prête à afficher. Rien si Mojang ne le connaît pas.
/// BHOST la demande à Mojang puis la garde un jour dans %APPDATA%\BHOST\tetes.
#[tauri::command]
async fn tete_joueur(app: AppHandle, nom: String) -> Option<String> {
    // Aller sur Internet prend du temps : on le fait à part, pour ne pas figer BHOST.
    let travail = tauri::async_runtime::spawn_blocking(move || {
        let etat = app.state::<Etat>();
        let agent = etat.reseau.clone();
        let telecharger = move |adresse: &str| reseau::telecharger(&agent, adresse);
        let uuid = etat.identifiants.lock().unwrap().get(&nom.to_ascii_lowercase()).cloned();
        let chercher = || {
            let resultat = tetes::chercher(&telecharger, &nom, uuid.as_deref());
            // Visible dans la fenêtre où tourne « npm run tauri dev » : utile si une tête ne vient pas.
            match &resultat {
                Err(erreur) => eprintln!("[BHOST] Tête de {nom} : {erreur}"),
                Ok(None) => eprintln!("[BHOST] Tête de {nom} : pas de skin chez Mojang"),
                Ok(Some(_)) => {}
            }
            resultat
        };
        tetes::avec_cache(&etat.dossier.join("tetes"), &nom, chercher).map(|tete| tetes::en_adresse(&tete))
    });
    travail.await.ok().flatten()
}

/// L'icône de BHOST dans Windows (barre des tâches, Alt+Tab, zone de notification) suit le thème.
#[tauri::command]
fn changer_icone(app: AppHandle, fenetre: tauri::WebviewWindow, theme: String, etat: State<Etat>) -> Result<(), String> {
    zone::changer_icone(&app, &theme);
    icone::appliquer(&fenetre, &theme, &mut etat.icones.lock().unwrap())
}

/// Les lignes de console gardées en mémoire (pour l'ouverture de la page).
#[tauri::command]
fn lire_console(id: String, etat: State<Etat>) -> ConsoleVue {
    match etat.journaux.lock().unwrap().get(&id) {
        Some(journal) => ConsoleVue { dernier: journal.total, lignes: journal.lignes.clone() },
        None => ConsoleVue { dernier: 0, lignes: Vec::new() },
    }
}

// ---------------------------------------------------------------------------------------------
// Onglet Sauvegardes
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SauvegardesVue {
    reglages: serveurs::ReglagesSauvegarde,
    liste: Vec<sauvegarde::Sauvegarde>,
    /// Prochaine sauvegarde automatique (millisecondes), si le serveur tourne.
    prochaine: Option<u64>,
    /// Place libre sur le disque des sauvegardes, en Go.
    libre_go: f64,
}

fn sauvegardes_vues(etat: &Etat, serveur: &Serveur) -> SauvegardesVue {
    let rangement = dossier_sauvegardes(etat, serveur);
    let liste = sauvegarde::lister(&rangement, &serveur.epinglees);
    let derniere = liste.iter().filter(|s| s.genre == "auto").map(|s| s.quand).max();
    let allume = etat.statuts.lock().unwrap().get(&serveur.id).is_some_and(|s| s == "en-ligne");
    let depuis = etat.depuis.lock().unwrap().get(&serveur.id).copied();
    let prochaine = (serveur.sauvegarde.auto && allume)
        .then(|| derniere.max(depuis).unwrap_or_else(maintenant_ms) + serveur.sauvegarde.intervalle_ms());
    let libre = sysinfo::Disks::new_with_refreshed_list()
        .ok()
        .and_then(|d| d.iter().filter(|x| etat.dossier.starts_with(x.mount_point())).max_by_key(|x| x.mount_point().as_os_str().len()).map(|x| x.available_space()))
        .unwrap_or(0);
    SauvegardesVue { reglages: serveur.sauvegarde.clone(), liste, prochaine, libre_go: (libre as f64 / 1_073_741_824.0 * 10.0).round() / 10.0 }
}

#[tauri::command]
async fn sauvegardes_serveur(app: AppHandle, id: String) -> Result<SauvegardesVue, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        Ok(sauvegardes_vues(&etat, &serveur_de(&etat, &id)?))
    })
    .await
}

/// Change les réglages de la sauvegarde automatique.
#[tauri::command]
fn regler_sauvegardes(id: String, reglages: serveurs::ReglagesSauvegarde, etat: State<Etat>) -> Result<SauvegardesVue, String> {
    if !["1h", "6h", "24h"].contains(&reglages.frequence.as_str()) {
        return Err("Fréquence inconnue.".into());
    }
    let mut liste = etat.liste.lock().unwrap();
    let s = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
    s.sauvegarde = serveurs::ReglagesSauvegarde { garder: reglages.garder.clamp(2, 50), ..reglages };
    let copie = s.clone();
    serveurs::enregistrer(&etat.fichier, &liste)?;
    drop(liste);
    Ok(sauvegardes_vues(&etat, &copie))
}

/// « Sauvegarder maintenant » : tout le serveur, même s'il tourne.
#[tauri::command]
async fn sauvegarder_maintenant(app: AppHandle, id: String) -> Result<SauvegardesVue, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        let _travail = commencer_travail(&etat, &id)?;
        faire_sauvegarde(&app, &serveur, "manuelle", "tout", &mondes::niveau(Path::new(&serveur.dossier)), None)?;
        Ok(sauvegardes_vues(&etat, &serveur))
    })
    .await
}

/// Le chemin d'une sauvegarde du serveur, si elle existe.
fn sauvegarde_existante(etat: &Etat, serveur: &Serveur, fichier: &str) -> Result<PathBuf, String> {
    let chemin = dossier_sauvegardes(etat, serveur).join(fichier);
    if sauvegarde::nom_valide(fichier) && chemin.is_file() { Ok(chemin) } else { Err("Cette sauvegarde n'existe plus.".into()) }
}

/// Épingle une sauvegarde (jamais effacée par le nettoyage automatique), ou la désépingle.
#[tauri::command]
async fn epingler_sauvegarde(app: AppHandle, id: String, fichier: String, epinglee: bool) -> Result<SauvegardesVue, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        sauvegarde_existante(&etat, &serveur_de(&etat, &id)?, &fichier)?;
        let mut liste = etat.liste.lock().unwrap();
        let s = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
        s.epinglees.retain(|f| *f != fichier);
        if epinglee {
            s.epinglees.push(fichier);
        }
        let copie = s.clone();
        serveurs::enregistrer(&etat.fichier, &liste)?;
        drop(liste);
        Ok(sauvegardes_vues(&etat, &copie))
    })
    .await
}

/// Copie une sauvegarde où l'on veut (« Exporter en .zip »).
#[tauri::command]
async fn exporter_sauvegarde(app: AppHandle, id: String, fichier: String, cible: String) -> Result<(), String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let source = sauvegarde_existante(&etat, &serveur_de(&etat, &id)?, &fichier)?;
        std::fs::copy(source, cible).map(|_| ()).map_err(|e| format!("Copie impossible ({e})."))
    })
    .await
}

/// Met une sauvegarde à la Corbeille.
#[tauri::command]
async fn supprimer_sauvegarde(app: AppHandle, id: String, fichier: String) -> Result<SauvegardesVue, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        let chemin = sauvegarde_existante(&etat, &serveur, &fichier)?;
        corbeille::jeter(&chemin).or_else(|_| std::fs::remove_file(&chemin).map_err(|e| e.to_string()))?;
        let mut liste = etat.liste.lock().unwrap();
        if let Some(s) = liste.iter_mut().find(|s| s.id == id) {
            s.epinglees.retain(|f| *f != fichier);
        }
        let copie = liste.iter().find(|s| s.id == id).cloned().unwrap_or(serveur);
        serveurs::enregistrer(&etat.fichier, &liste)?;
        drop(liste);
        Ok(sauvegardes_vues(&etat, &copie))
    })
    .await
}

/// Revient à une sauvegarde (serveur arrêté). `tout` : tout le serveur ; sinon seulement le monde.
/// L'état actuel est sauvegardé juste avant.
#[tauri::command]
async fn restaurer_sauvegarde(app: AppHandle, id: String, fichier: String, tout: bool) -> Result<ServeurVu, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        arrete(&etat, &id)?;
        let archive = sauvegarde_existante(&etat, &serveur, &fichier)?;
        let meta = sauvegarde::lire_meta(&archive).unwrap_or_default();
        let _travail = commencer_travail(&etat, &id)?;
        let racine = PathBuf::from(&serveur.dossier);
        // Le monde de la sauvegarde : celui qu'elle note, sinon le premier dossier qui a un level.dat… ou le monde joué.
        let haut = sauvegarde::dossiers_du_haut(&archive);
        // Un monde = un dossier au nom valide, avec un level.dat : jamais plugins/, server.jar…
        let est_monde = |n: &str| {
            mondes::nom_valide(n)
                && !["plugins", "mods", "config", "libraries", "versions"].contains(&n.to_ascii_lowercase().as_str())
                && sauvegarde::contient(&archive, &format!("{n}/level.dat"))
        };
        let monde = Some(meta.monde.clone())
            .filter(|m| haut.contains(m) && est_monde(m))
            .or_else(|| haut.iter().find(|n| !n.ends_with("_nether") && !n.ends_with("_the_end") && est_monde(n)).cloned())
            .unwrap_or_else(|| mondes::niveau(&racine));
        // 1. L'état actuel, au cas où.
        faire_sauvegarde(&app, &serveur, "restauration", "tout", &mondes::niveau(&racine), Some(format!("Avant retour au {}", fichier.trim_end_matches(".zip"))))?;
        // 2. Le monde de la sauvegarde remplace l'actuel (on efface d'abord : pas de morceaux mélangés).
        let morceaux: Vec<String> =
            [monde.clone(), format!("{monde}_nether"), format!("{monde}_the_end")].into_iter().filter(|m| haut.contains(m) && mondes::nom_valide(m)).collect();
        for m in &morceaux {
            let d = racine.join(m);
            if d.is_dir() {
                std::fs::remove_dir_all(&d).map_err(|_| "Le monde n'a pas pu être effacé. Un fichier est peut-être ouvert.".to_string())?;
            }
        }
        let filtre = if tout { None } else { Some(morceaux.as_slice()) };
        sauvegarde::restaurer(&archive, &racine, filtre, &mut |p| dire_travail(&app, &id, "Restauration", p))?;
        if mondes::nom_valide(&monde) {
            choisir_monde(&serveur, &monde)?;
        }
        // 3. Tout le serveur : il reprend aussi le logiciel de la sauvegarde.
        let mut liste = etat.liste.lock().unwrap();
        let s = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
        if tout && !meta.jar.is_empty() && !meta.logiciel.is_empty() {
            s.logiciel = meta.logiciel;
            s.version = meta.version;
            s.jar = meta.jar;
        }
        let copie = s.clone();
        serveurs::enregistrer(&etat.fichier, &liste)?;
        drop(liste);
        Ok(vu(&etat, &copie))
    })
    .await
}

/// Ouvre le dossier des sauvegardes dans l'Explorateur.
#[tauri::command]
fn ouvrir_sauvegardes(app: AppHandle, id: String, etat: State<Etat>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dossier = dossier_sauvegardes(&etat, &serveur_de(&etat, &id)?);
    std::fs::create_dir_all(&dossier).map_err(|e| e.to_string())?;
    app.opener().open_path(dossier.to_string_lossy(), None::<&str>).map_err(|_| "Le dossier n'a pas pu s'ouvrir.".to_string())
}

/// Toutes les minutes : les serveurs allumés dont la dernière sauvegarde automatique est trop vieille en ont une nouvelle.
fn sauvegarder_en_continu(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let etat = app.state::<Etat>();
            let serveurs: Vec<Serveur> = etat.liste.lock().unwrap().iter().filter(|s| s.sauvegarde.auto).cloned().collect();
            for serveur in serveurs {
                if let Some(quand) = sauvegardes_vues(&etat, &serveur).prochaine {
                    if quand > maintenant_ms() {
                        continue;
                    }
                    let Ok(_travail) = commencer_travail(&etat, &serveur.id) else { continue };
                    let monde = mondes::niveau(Path::new(&serveur.dossier));
                    if let Err(erreur) = faire_sauvegarde(&app, &serveur, "auto", "tout", &monde, None) {
                        dire_ligne(&app, &serveur.id, &format!("[BHOST!] {erreur}"));
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Onglet Fichiers
// ---------------------------------------------------------------------------------------------

#[tauri::command]
async fn fichiers_serveur(app: AppHandle, id: String, dossier: String) -> Result<Vec<fichiers::Element>, String> {
    a_part(move || fichiers::lister(Path::new(&serveur_de(&app.state::<Etat>(), &id)?.dossier), &dossier)).await
}

#[tauri::command]
async fn lire_fichier(app: AppHandle, id: String, chemin: String) -> Result<fichiers::Fichier, String> {
    a_part(move || fichiers::lire(Path::new(&serveur_de(&app.state::<Etat>(), &id)?.dossier), &chemin)).await
}

/// Enregistre un fichier texte. Les listes de joueurs se gèrent dans l'onglet Joueurs.
#[tauri::command]
fn ecrire_fichier(id: String, chemin: String, texte: String, etat: State<Etat>) -> Result<(), String> {
    let nom = chemin.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
    if [listes::OPS, listes::BLANCHE, listes::BANNIS, "banned-ips.json", "usercache.json"].contains(&nom.as_str()) || nom.ends_with(".log") {
        return Err("Ce fichier ne se modifie pas ici.".into());
    }
    if texte.len() > 2 * 1024 * 1024 {
        return Err("Texte trop long.".into());
    }
    fichiers::ecrire(Path::new(&serveur_de(&etat, &id)?.dossier), &chemin, &texte)
}

/// Copie des fichiers du PC (choisis, ou glissés dans la fenêtre) dans un dossier du serveur.
#[tauri::command]
async fn importer_fichiers(app: AppHandle, id: String, dossier: String, sources: Vec<String>) -> Result<usize, String> {
    a_part(move || {
        let serveur = serveur_de(&app.state::<Etat>(), &id)?;
        let sources: Vec<PathBuf> = sources.into_iter().map(PathBuf::from).collect();
        fichiers::importer(Path::new(&serveur.dossier), &dossier, &sources)
    })
    .await
}

/// L'icône du serveur dans Minecraft : pose celle de BHOST (si l'utilisateur n'a pas mis la sienne,
/// ou s'il veut la `remettre`), et dit si c'est une image à lui.
#[tauri::command]
async fn icone_du_jeu(app: AppHandle, id: String, remettre: bool) -> Result<bool, String> {
    a_part(move || {
        let serveur = serveur_de(&app.state::<Etat>(), &id)?;
        let dossier = Path::new(&serveur.dossier);
        let cle = serveur.couleur.clone().filter(|c| serveurs::icone_valide(c)).unwrap_or_else(|| icone_jeu::par_defaut(&serveur.logiciel).to_string());
        icone_jeu::poser(dossier, &cle, remettre)?;
        Ok(icone_jeu::perso(dossier))
    })
    .await
}

/// L'icône du serveur (dans la liste des serveurs du jeu), à partir d'une image PNG.
#[tauri::command]
async fn icone_serveur(app: AppHandle, id: String, source: String) -> Result<(), String> {
    a_part(move || fichiers::icone(Path::new(&source), Path::new(&serveur_de(&app.state::<Etat>(), &id)?.dossier))).await
}

/// Montre un fichier ou un dossier du serveur dans l'Explorateur.
#[tauri::command]
fn montrer_fichier(app: AppHandle, id: String, chemin: String, etat: State<Etat>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let complet = fichiers::chemin_sur(Path::new(&serveur_de(&etat, &id)?.dossier), &chemin)?;
    let resultat = if complet.is_dir() { app.opener().open_path(complet.to_string_lossy(), None::<&str>) } else { app.opener().reveal_item_in_dir(&complet) };
    resultat.map_err(|_| "L'Explorateur n'a pas pu s'ouvrir.".to_string())
}

/// Les réglages de server.properties (vue « Réglages faciles »).
#[tauri::command]
fn proprietes_serveur(id: String, etat: State<Etat>) -> Result<BTreeMap<String, String>, String> {
    Ok(serveurs::lire_proprietes(Path::new(&serveur_de(&etat, &id)?.dossier)).into_iter().collect())
}

/// Les clés que la vue « Réglages faciles » peut changer.
const PROPRIETES_FACILES: [&str; 11] =
    ["motd", "max-players", "online-mode", "server-port", "hardcore", "pvp", "allow-flight", "enable-command-block", "spawn-protection", "view-distance", "simulation-distance"];

/// Vérifie et met en forme une valeur de server.properties.
fn propriete_valide(cle: &str, valeur: &str) -> Result<String, String> {
    let nombre = |min: u32, max: u32| valeur.trim().parse::<u32>().ok().filter(|n| (min..=max).contains(n)).map(|n| n.to_string());
    match cle {
        "motd" => Ok(creation::echapper(&valeur.chars().take(120).collect::<String>())),
        "max-players" => nombre(1, 1000).ok_or_else(|| "Entre 1 et 1000.".to_string()),
        "server-port" => nombre(1024, 65535).ok_or_else(|| "Port entre 1024 et 65535.".to_string()),
        "spawn-protection" => nombre(0, 256).ok_or_else(|| "Valeur refusée.".to_string()),
        "view-distance" | "simulation-distance" => nombre(2, 32).ok_or_else(|| "Entre 2 et 32.".to_string()),
        _ if valeur == "true" || valeur == "false" => Ok(valeur.into()),
        _ => Err("Valeur refusée.".into()),
    }
}

/// Change des réglages de server.properties (au prochain démarrage du serveur).
#[tauri::command]
fn regler_proprietes(id: String, valeurs: BTreeMap<String, String>, etat: State<Etat>) -> Result<BTreeMap<String, String>, String> {
    let serveur = serveur_de(&etat, &id)?;
    let mut propres = Vec::new();
    for (cle, valeur) in &valeurs {
        let cle = PROPRIETES_FACILES.iter().find(|k| **k == cle.as_str()).ok_or("Réglage inconnu.")?;
        propres.push((*cle, propriete_valide(cle, valeur)?));
    }
    let dossier = Path::new(&serveur.dossier);
    serveurs::modifier_proprietes(dossier, &propres)?;
    Ok(serveurs::lire_proprietes(dossier).into_iter().collect())
}

// ---------------------------------------------------------------------------------------------
// Onglet Joueurs : connectés, opérateurs, liste blanche, bannis, fiche
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
struct Connecte {
    nom: String,
    /// Arrivé à cette heure (millisecondes), si BHOST l'a vu arriver.
    depuis: Option<u64>,
}

/// Ce que l'onglet Joueurs affiche.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JoueursVue {
    connectes: Vec<Connecte>,
    /// Les pseudos des opérateurs.
    ops: Vec<String>,
    /// La liste blanche est active (white-list=true).
    liste_blanche: bool,
    blancs: Vec<listes::Inscrit>,
    bannis: Vec<listes::Inscrit>,
    /// Les joueurs refusés par la liste blanche, qui attendent une réponse.
    demandes: Vec<demandes::Demande>,
}

fn joueurs_vus(etat: &Etat, serveur: &Serveur) -> JoueursVue {
    let dossier = Path::new(&serveur.dossier);
    let arrivees = etat.arrivees.lock().unwrap().get(&serveur.id).cloned().unwrap_or_default();
    let connectes = etat.joueurs.lock().unwrap().get(&serveur.id).cloned().unwrap_or_default();
    let blancs = listes::inscrits(dossier, listes::BLANCHE);
    let demandes = {
        // Un joueur ajouté à la liste blanche ailleurs (console, fichier) n'a plus besoin de demande.
        let mut registre = etat.demandes.lock().unwrap();
        let noms: Vec<String> = blancs.iter().map(|b| b.nom.clone()).collect();
        if registre.nettoyer(&serveur.id, &noms) {
            registre.enregistrer(&etat.dossier.join(FICHIER_DEMANDES));
        }
        registre.liste(&serveur.id)
    };
    JoueursVue {
        connectes: connectes.into_iter().map(|nom| Connecte { depuis: arrivees.get(&nom).copied(), nom }).collect(),
        ops: listes::inscrits(dossier, listes::OPS).into_iter().map(|i| i.nom).collect(),
        liste_blanche: serveurs::lire_proprietes(dossier).iter().any(|(k, v)| k == "white-list" && v == "true"),
        blancs,
        bannis: listes::inscrits(dossier, listes::BANNIS),
        demandes,
    }
}

#[tauri::command]
fn joueurs_serveur(id: String, etat: State<Etat>) -> Result<JoueursVue, String> {
    Ok(joueurs_vus(&etat, &serveur_de(&etat, &id)?))
}

/// Le serveur tourne et a fini de démarrer : on peut lui parler.
fn en_ligne(etat: &Etat, id: &str) -> Option<Arc<Processus>> {
    if etat.statuts.lock().unwrap().get(id).is_some_and(|s| s == "en-ligne") { etat.vivants.lock().unwrap().get(id).cloned() } else { None }
}

/// Envoie une commande au serveur et l'écrit dans sa console.
fn commander(app: &AppHandle, id: &str, p: &Processus, commande: &str) -> Result<(), String> {
    dire_ligne(app, id, &format!("> {commande}"));
    p.ecrire(commande)
}

/// L'identifiant (avec tirets) d'un joueur, pour l'écrire dans une liste quand le serveur est arrêté.
/// Déjà vu par BHOST ou par le serveur ; sinon demandé à Mojang (ou fabriqué, si le serveur ne vérifie pas les comptes).
fn uuid_joueur(app: &AppHandle, serveur: &Serveur, nom: &str, demander: bool) -> Result<Option<String>, String> {
    let etat = app.state::<Etat>();
    if let Some(u) = etat.identifiants.lock().unwrap().get(&nom.to_ascii_lowercase()).and_then(|u| listes::avec_tirets(u)) {
        return Ok(Some(u));
    }
    let dossier = Path::new(&serveur.dossier);
    if let Some(u) = listes::uuid_connu(dossier, nom) {
        return Ok(Some(u));
    }
    let hors_ligne = serveurs::lire_proprietes(dossier).iter().any(|(k, v)| k == "online-mode" && v == "false");
    if hors_ligne || nom.starts_with('.') {
        return Ok(Some(listes::uuid_hors_ligne(nom)));
    }
    if !demander {
        return Ok(None);
    }
    let agent = etat.reseau.clone();
    let telecharger = |adresse: &str| reseau::telecharger(&agent, adresse);
    match tetes::identifiant_du_pseudo(&telecharger, nom) {
        Ok(Some(u)) => listes::avec_tirets(&u).map(Some).ok_or_else(|| "Réponse de Mojang illisible.".to_string()),
        Ok(None) => Err(format!("« {nom} » n'existe pas chez Mojang.")),
        Err(_) => Err("Pas de connexion. Réessaie.".into()),
    }
}

/// OP, liste blanche, expulsion, bannissement. Serveur en ligne : par commande. Arrêté : dans ses fichiers.
#[tauri::command]
async fn action_joueur(app: AppHandle, id: String, action: String, nom: String, motif: Option<String>) -> Result<JoueursVue, String> {
    a_part(move || agir_joueur(&app, &id, &action, &nom, motif)).await
}

fn agir_joueur(app: &AppHandle, id: &str, action: &str, nom: &str, motif: Option<String>) -> Result<JoueursVue, String> {
    if !listes::pseudo_valide(nom) {
        return Err("Pseudo invalide.".into());
    }
    let etat = app.state::<Etat>();
    let serveur = serveur_de(&etat, id)?;
    let motif = motif.map(|m| listes::motif_propre(&m)).unwrap_or_default();
    let (commande, fichier, ajout) = match action {
        "op" => (format!("op {nom}"), listes::OPS, true),
        "deop" => (format!("deop {nom}"), listes::OPS, false),
        "blanc_ajouter" => (format!("whitelist add {nom}"), listes::BLANCHE, true),
        "blanc_retirer" => (format!("whitelist remove {nom}"), listes::BLANCHE, false),
        "bannir" if motif.is_empty() => (format!("ban {nom}"), listes::BANNIS, true),
        "bannir" => (format!("ban {nom} {motif}"), listes::BANNIS, true),
        "debannir" => (format!("pardon {nom}"), listes::BANNIS, false),
        "expulser" => (format!("kick {nom}"), "", false),
        _ => return Err("Action inconnue.".into()),
    };
    if let Some(p) = en_ligne(&etat, id) {
        commander(app, id, &p, &commande)?;
        // Le serveur réécrit ses fichiers juste après : on lui laisse un instant.
        std::thread::sleep(Duration::from_millis(600));
    } else if fichier.is_empty() {
        return Err("Il n'est pas connecté.".into());
    } else {
        let dossier = Path::new(&serveur.dossier);
        let changement = if ajout {
            let uuid = uuid_joueur(app, &serveur, nom, true)?.unwrap_or_default();
            listes::modifier(dossier, fichier, listes::Changement::Ajouter { nom, uuid: &uuid, motif: Some(&motif) }, SystemTime::now())
        } else {
            listes::modifier(dossier, fichier, listes::Changement::Retirer { nom }, SystemTime::now())
        };
        changement?;
    }
    // Ajouté à la liste blanche : sa demande (s'il en avait une) disparaît.
    if action == "blanc_ajouter" && etat.demandes.lock().unwrap().accepter(id, nom) {
        dire_demandes(app, id, None);
    }
    Ok(joueurs_vus(&etat, &serveur))
}

/// Joueurs › Demandes : Accepter (ajout à la liste blanche) ou Refuser (ses prochains essais sont ignorés).
#[tauri::command]
async fn repondre_demande(app: AppHandle, id: String, nom: String, accepter: bool) -> Result<JoueursVue, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        if accepter {
            // L'identifiant lu dans la ligne de refus : pas besoin de le demander à Mojang.
            let uuid = etat.demandes.lock().unwrap().trouver(&id, &nom).and_then(|d| d.uuid.clone());
            if let Some(u) = uuid {
                noter_identifiant(&etat, &nom, u, false);
            }
            return agir_joueur(&app, &id, "blanc_ajouter", &nom, None);
        }
        etat.demandes.lock().unwrap().refuser(&id, &nom);
        dire_demandes(&app, &id, None);
        Ok(joueurs_vus(&etat, &serveur_de(&etat, &id)?))
    })
    .await
}

/// Active ou coupe la liste blanche (server.properties, et tout de suite si le serveur est en ligne).
#[tauri::command]
fn liste_blanche(app: AppHandle, id: String, active: bool, etat: State<Etat>) -> Result<JoueursVue, String> {
    let serveur = serveur_de(&etat, &id)?;
    serveurs::modifier_proprietes(Path::new(&serveur.dossier), &[("white-list", active.to_string())])?;
    if let Some(p) = en_ligne(&etat, &id) {
        commander(&app, &id, &p, if active { "whitelist on" } else { "whitelist off" })?;
    }
    Ok(joueurs_vus(&etat, &serveur))
}

/// La fiche d'un joueur, lue dans le monde joué. `None` si BHOST ne la trouve pas.
#[tauri::command]
async fn fiche_joueur(app: AppHandle, id: String, nom: String) -> Result<Option<fiche::Fiche>, String> {
    a_part(move || {
        if !listes::pseudo_valide(&nom) {
            return Err("Pseudo invalide.".into());
        }
        let serveur = serveur_de(&app.state::<Etat>(), &id)?;
        let Some(uuid) = uuid_joueur(&app, &serveur, &nom, false)? else { return Ok(None) };
        let dossier = Path::new(&serveur.dossier);
        Ok(fiche::lire(&dossier.join(mondes::niveau(dossier)), &uuid))
    })
    .await
}

/// Vrai pour les versions d'avant la 1.13 (commandes avec des chiffres, pas de « execute in »).
fn avant_1_13(version: &str) -> bool {
    let n: Vec<u32> = version.split('.').map_while(|x| x.parse().ok()).collect();
    matches!(n.as_slice(), [1, mineure, ..] if *mineure < 13)
}

/// Les commandes de la fiche d'un joueur connecté : mode de jeu, retour au point d'apparition.
#[tauri::command]
fn commande_joueur(app: AppHandle, id: String, nom: String, action: String, etat: State<Etat>) -> Result<(), String> {
    if !listes::pseudo_valide(&nom) {
        return Err("Pseudo invalide.".into());
    }
    let serveur = serveur_de(&etat, &id)?;
    let p = en_ligne(&etat, &id).ok_or("Le serveur est arrêté.")?;
    let vieux = avant_1_13(&serveur.version);
    let commande = match action.as_str() {
        "apparition" => {
            let dossier = Path::new(&serveur.dossier);
            let [x, y, z] = fiche::apparition(&dossier.join(mondes::niveau(dossier))).ok_or("Point d'apparition introuvable.")?;
            if vieux { format!("tp {nom} {x} {y} {z}") } else { format!("execute in minecraft:overworld run tp {nom} {x}.5 {y} {z}.5") }
        }
        mode => {
            let (mot, chiffre) = match mode {
                "mode:survie" => ("survival", 0),
                "mode:creatif" => ("creative", 1),
                "mode:aventure" => ("adventure", 2),
                "mode:spectateur" => ("spectator", 3),
                _ => return Err("Action inconnue.".into()),
            };
            if vieux { format!("gamemode {chiffre} {nom}") } else { format!("gamemode {mot} {nom}") }
        }
    };
    commander(&app, &id, &p, &commande)
}

// ---------------------------------------------------------------------------------------------
// Paramètres › Règles du jeu, Mondes, Changer de version
// ---------------------------------------------------------------------------------------------

/// Le serveur `id`, ou une phrase si BHOST ne le connaît plus.
fn serveur_de(etat: &Etat, id: &str) -> Result<Serveur, String> {
    etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned().ok_or_else(|| "Ce serveur n'existe plus.".to_string())
}

/// Refuse si le serveur tourne (ou attend son Java).
fn arrete(etat: &Etat, id: &str) -> Result<(), String> {
    if etat.vivants.lock().unwrap().contains_key(id) || etat.preparation.lock().unwrap().contains(id) {
        return Err("Arrête-le d'abord.".into());
    }
    Ok(())
}

/// Un long travail sur un serveur : un seul à la fois, et pas de démarrage pendant ce temps.
/// Le serveur est libéré quand le travail se termine (même en cas d'erreur).
struct Travail<'a> {
    etat: &'a Etat,
    id: String,
}

impl Drop for Travail<'_> {
    fn drop(&mut self) {
        self.etat.travaux.lock().unwrap().remove(&self.id);
    }
}

fn commencer_travail<'a>(etat: &'a Etat, id: &str) -> Result<Travail<'a>, String> {
    if !etat.travaux.lock().unwrap().insert(id.to_string()) {
        return Err("Attends la fin de l'opération en cours.".into());
    }
    Ok(Travail { etat, id: id.to_string() })
}

/// Où en est un long travail (pour la petite fenêtre qui attend).
#[derive(Serialize, Clone)]
struct NouvelleTravail<'a> {
    id: &'a str,
    /// « Sauvegarde », « Java », « Téléchargement », « Copie »…
    etape: &'a str,
    pourcent: u8,
}

fn dire_travail(app: &AppHandle, id: &str, etape: &str, pourcent: u8) {
    let _ = app.emit("travail", NouvelleTravail { id, etape, pourcent });
}

/// Lance un travail lourd à part, pour ne pas figer BHOST.
async fn a_part<T: Send + 'static>(travail: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(travail).await.map_err(|_| "Ça n'a pas marché.".to_string())?
}

/// Les règles du jeu du monde joué, avec les changements pas encore écrits dans le monde.
fn regles_vues(etat: &Etat, serveur: &Serveur) -> Vec<regles::RegleVue> {
    let dossier = Path::new(&serveur.dossier);
    let mut liste = regles::lire(&dossier.join(mondes::niveau(dossier)), &serveur.version);
    let envoyees = etat.regles_envoyees.lock().unwrap().get(&serveur.id).cloned().unwrap_or_default();
    for r in &mut liste {
        if let Some(v) = serveur.regles_attente.get(r.cle).or_else(|| envoyees.get(r.cle)) {
            r.valeur = *v;
        }
    }
    liste
}

#[tauri::command]
fn regles_serveur(id: String, etat: State<Etat>) -> Result<Vec<regles::RegleVue>, String> {
    Ok(regles_vues(&etat, &serveur_de(&etat, &id)?))
}

/// Change une règle du jeu : tout de suite si le serveur est en ligne, sinon à son prochain démarrage.
#[tauri::command]
fn regler_regle(app: AppHandle, id: String, cle: String, valeur: i64, etat: State<Etat>) -> Result<Vec<regles::RegleVue>, String> {
    if !regles::valide(&cle, valeur) {
        return Err("Valeur refusée.".into());
    }
    let serveur = serveur_de(&etat, &id)?;
    let commande = regles::commande(&serveur.version, &cle, valeur).ok_or("Cette version n'a pas cette règle.")?;
    let en_ligne = etat.statuts.lock().unwrap().get(&id).is_some_and(|s| s == "en-ligne");
    let processus = if en_ligne { etat.vivants.lock().unwrap().get(&id).cloned() } else { None };
    match processus {
        Some(p) => {
            dire_ligne(&app, &id, &format!("> {commande}"));
            p.ecrire(&commande)?;
            etat.regles_envoyees.lock().unwrap().entry(id.clone()).or_default().insert(cle, valeur);
        }
        None => {
            let mut liste = etat.liste.lock().unwrap();
            let s = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
            s.regles_attente.insert(cle, valeur);
            serveurs::enregistrer(&etat.fichier, &liste)?;
        }
    }
    Ok(regles_vues(&etat, &serveur_de(&etat, &id)?))
}

/// Quand un serveur vient de démarrer : on lui envoie les règles changées pendant qu'il était arrêté.
fn appliquer_regles(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    let (version, attente) = {
        let mut liste = etat.liste.lock().unwrap();
        let Some(s) = liste.iter_mut().find(|s| s.id == id) else { return };
        if s.regles_attente.is_empty() {
            return;
        }
        let attente = std::mem::take(&mut s.regles_attente);
        let version = s.version.clone();
        let _ = serveurs::enregistrer(&etat.fichier, &liste);
        (version, attente)
    };
    let Some(p) = etat.vivants.lock().unwrap().get(id).cloned() else { return };
    for (cle, valeur) in &attente {
        if let Some(commande) = regles::commande(&version, cle, *valeur) {
            dire_ligne(app, id, &format!("> {commande}"));
            let _ = p.ecrire(&commande);
        }
    }
    etat.regles_envoyees.lock().unwrap().entry(id.to_string()).or_default().extend(attente);
}

/// Les mondes d'un serveur.
#[tauri::command]
async fn mondes_serveur(app: AppHandle, id: String) -> Result<Vec<mondes::Monde>, String> {
    a_part(move || Ok(mondes::lister(Path::new(&serveur_de(&app.state::<Etat>(), &id)?.dossier)))).await
}

/// Le dossier d'un monde du serveur, s'il existe.
fn monde_existant(serveur: &Serveur, nom: &str) -> Result<PathBuf, String> {
    let dossier = Path::new(&serveur.dossier).join(nom);
    if mondes::nom_valide(nom) && dossier.is_dir() { Ok(dossier) } else { Err("Ce monde n'existe plus.".into()) }
}

/// Le monde que le serveur jouera à son prochain démarrage.
fn choisir_monde(serveur: &Serveur, nom: &str) -> Result<(), String> {
    serveurs::modifier_proprietes(Path::new(&serveur.dossier), &[("level-name", creation::echapper(nom))])
}

/// « Utiliser » : ce monde devient celui du serveur (au prochain démarrage).
#[tauri::command]
async fn utiliser_monde(app: AppHandle, id: String, dossier: String) -> Result<Vec<mondes::Monde>, String> {
    a_part(move || {
        let serveur = serveur_de(&app.state::<Etat>(), &id)?;
        monde_existant(&serveur, &dossier)?;
        choisir_monde(&serveur, &dossier)?;
        Ok(mondes::lister(Path::new(&serveur.dossier)))
    })
    .await
}

/// « Nouveau monde » : un monde vide, que le serveur créera quand on l'utilisera.
#[tauri::command]
async fn nouveau_monde(app: AppHandle, id: String) -> Result<Vec<mondes::Monde>, String> {
    a_part(move || {
        let serveur = serveur_de(&app.state::<Etat>(), &id)?;
        let dossier = Path::new(&serveur.dossier);
        mondes::nouveau(dossier)?;
        // Une graine laissée par une réinitialisation ne doit pas resservir.
        serveurs::modifier_proprietes(dossier, &[("level-seed", String::new())])?;
        Ok(mondes::lister(dossier))
    })
    .await
}

/// Met un monde (pas celui joué) à la Corbeille, avec son Nether et son End.
#[tauri::command]
async fn supprimer_monde(app: AppHandle, id: String, dossier: String) -> Result<Vec<mondes::Monde>, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        let racine = Path::new(&serveur.dossier);
        if mondes::niveau(racine) == dossier {
            return Err("C'est le monde joué. Choisis-en un autre d'abord.".into());
        }
        monde_existant(&serveur, &dossier)?;
        let _travail = commencer_travail(&etat, &id)?;
        for (chemin, _) in mondes::dossiers(racine, &dossier) {
            corbeille::jeter(&chemin)?;
        }
        Ok(mondes::lister(racine))
    })
    .await
}

/// Le dossier des sauvegardes d'un serveur : %APPDATA%\BHOST\sauvegardes\<nom de son dossier>-<8 signes>.
/// Les 8 signes viennent du chemin complet : deux serveurs au même nom de dossier ne se mélangent pas.
fn dossier_sauvegardes(etat: &Etat, serveur: &Serveur) -> PathBuf {
    use sha1::{Digest, Sha1};
    let chemin = serveur.dossier.trim_end_matches(['\\', '/']).to_lowercase();
    let empreinte: String = Sha1::digest(chemin.as_bytes()).iter().take(4).map(|b| format!("{b:02x}")).collect();
    let nom = Path::new(&serveur.dossier).file_name().map(|n| creation::nom_de_dossier(&n.to_string_lossy())).unwrap_or_else(|| "serveur".into());
    etat.dossier.join("sauvegardes").join(format!("{nom}-{empreinte}"))
}

/// Fait une sauvegarde. `quoi` : « tout » (le serveur, sans journaux ni caches) ou « monde » (le monde `monde`).
/// Si le serveur tourne, on lui fait d'abord tout écrire sur le disque, et il n'écrit plus pendant la copie.
fn faire_sauvegarde(app: &AppHandle, serveur: &Serveur, genre: &str, quoi: &str, monde: &str, etiquette: Option<String>) -> Result<PathBuf, String> {
    let etat = app.state::<Etat>();
    let racine = Path::new(&serveur.dossier);
    let elements = if quoi == "monde" { mondes::dossiers(racine, monde) } else { sauvegarde::elements_du_serveur(racine, serveur.jar.starts_with('@')) };
    let rangement = dossier_sauvegardes(&etat, serveur);
    let archive = sauvegarde::nom_libre(&rangement, SystemTime::now(), genre);
    let meta = sauvegarde::Meta {
        genre: genre.into(),
        quoi: quoi.into(),
        monde: monde.into(),
        logiciel: serveur.logiciel.clone(),
        version: serveur.version.clone(),
        jar: serveur.jar.clone(),
        etiquette,
    };
    let allume = en_ligne(&etat, &serveur.id);
    if let Some(p) = &allume {
        let _ = commander(app, &serveur.id, p, "save-off");
        let _ = commander(app, &serveur.id, p, if avant_1_13(&serveur.version) { "save-all" } else { "save-all flush" });
        std::thread::sleep(Duration::from_secs(3));
    }
    let fait = sauvegarde::zipper_avec(&elements, Some(&meta), &archive, &mut |p| dire_travail(app, &serveur.id, "Sauvegarde", p));
    if let Some(p) = &allume {
        let _ = commander(app, &serveur.id, p, "save-on");
    }
    fait.map_err(|e| format!("Sauvegarde impossible : {e}"))?;
    // Les automatiques en trop partent (les plus anciennes, jamais les épinglées).
    let garder = serveur.sauvegarde.garder.clamp(1, 100) as usize;
    for fichier in sauvegarde::en_trop(&sauvegarde::lister(&rangement, &serveur.epinglees), garder) {
        let _ = std::fs::remove_file(rangement.join(fichier));
    }
    Ok(archive)
}

/// Réinitialise un monde : sauvegarde, puis effacé. Le serveur le recrée (même graine, ou une nouvelle).
#[tauri::command]
async fn reinitialiser_monde(app: AppHandle, id: String, dossier: String, meme_graine: bool) -> Result<Vec<mondes::Monde>, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        let racine = Path::new(&serveur.dossier);
        let joue = mondes::niveau(racine) == dossier;
        if joue {
            arrete(&etat, &id)?;
        }
        let chemin = monde_existant(&serveur, &dossier)?;
        let _travail = commencer_travail(&etat, &id)?;
        let graine = if meme_graine { mondes::graine(&chemin) } else { None };
        if !mondes::dossiers(racine, &dossier).is_empty() {
            faire_sauvegarde(&app, &serveur, "reinit", "monde", &dossier, Some(format!("Avant réinitialisation de « {dossier} »")))?;
        }
        for (d, _) in mondes::dossiers(racine, &dossier) {
            std::fs::remove_dir_all(&d).map_err(|_| "Le monde n'a pas pu être effacé. Un fichier est peut-être ouvert.".to_string())?;
        }
        // Le dossier revient vide : le serveur recréera le monde (un monde pas joué garde sa place dans la liste).
        std::fs::create_dir_all(&chemin).map_err(|e| e.to_string())?;
        if !joue {
            mondes::marquer(&chemin)?;
        }
        serveurs::modifier_proprietes(racine, &[("level-seed", graine.map(|g| g.to_string()).unwrap_or_default())])?;
        Ok(mondes::lister(racine))
    })
    .await
}

/// « Télécharger en .zip » : le monde (avec son Nether et son End) dans le fichier choisi.
#[tauri::command]
async fn exporter_monde(app: AppHandle, id: String, dossier: String, fichier: String) -> Result<(), String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        monde_existant(&serveur, &dossier)?;
        let _travail = commencer_travail(&etat, &id)?;
        let fichier = if fichier.to_ascii_lowercase().ends_with(".zip") { PathBuf::from(fichier) } else { PathBuf::from(format!("{fichier}.zip")) };
        let elements = mondes::dossiers(Path::new(&serveur.dossier), &dossier);
        sauvegarde::zipper(&elements, &fichier, &mut |p| dire_travail(&app, &id, "Archive", p))
    })
    .await
}

/// Un monde du jeu solo, pour la fenêtre « Importer un monde ».
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MondeSolo {
    #[serde(flatten)]
    monde: mondes::Monde,
    /// Son dossier complet, à renvoyer pour l'importer.
    chemin: String,
    /// Ouvert par une version plus récente que le serveur : il ne pourra pas s'ouvrir.
    trop_recent: bool,
}

/// Les mondes du jeu solo (%APPDATA%\.minecraft\saves).
#[tauri::command]
async fn mondes_solo(app: AppHandle, id: String) -> Result<Vec<MondeSolo>, String> {
    a_part(move || {
        let serveur = serveur_de(&app.state::<Etat>(), &id)?;
        let saves = app.path().data_dir().map_err(|e| e.to_string())?.join(".minecraft").join("saves");
        Ok(mondes::solo(&saves)
            .into_iter()
            .map(|m| MondeSolo {
                chemin: saves.join(&m.dossier).to_string_lossy().into_owned(),
                trop_recent: m.version.as_deref().is_some_and(|v| mondes::trop_recent(v, &serveur.version)),
                monde: m,
            })
            .collect())
    })
    .await
}

/// Copie un monde (dossier, ou archive .zip) dans le serveur. `actuel` : il devient le monde joué.
#[tauri::command]
async fn importer_monde(app: AppHandle, id: String, source: String, actuel: bool) -> Result<Vec<mondes::Monde>, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        let _travail = commencer_travail(&etat, &id)?;
        let racine = PathBuf::from(&serveur.dossier);
        let source = PathBuf::from(&source);
        let provisoire = racine.join(".bhost-import");
        let _ = std::fs::remove_dir_all(&provisoire);
        let resultat = (|| {
            let monde = if source.is_file() {
                dire_travail(&app, &id, "Ouverture", 0);
                java::extraire(&source, &provisoire)?;
                mondes::racine(&provisoire).ok_or("Pas de monde dans cette archive.")?
            } else {
                mondes::racine(&source).ok_or("Pas de monde dans ce dossier.")?
            };
            if monde.starts_with(&racine) && !monde.starts_with(&provisoire) {
                return Err("Ce monde est déjà dans le serveur.".into());
            }
            if let Some(v) = mondes::version_du_monde(&monde).filter(|v| mondes::trop_recent(v, &serveur.version)) {
                return Err(format!("Ce monde vient de Minecraft {v}. Trop récent pour ce serveur ({}).", serveur.version));
            }
            let nom = mondes::copier(&monde, &racine, &mut |p| dire_travail(&app, &id, "Copie", p))?;
            if actuel {
                choisir_monde(&serveur, &nom)?;
            }
            Ok(mondes::lister(&racine))
        })();
        let _ = std::fs::remove_dir_all(&provisoire);
        resultat
    })
    .await
}

/// Les logiciels vers lesquels un serveur peut passer (Changer de version…).
fn familles(logiciel: &str) -> &'static [&'static str] {
    match logiciel {
        "Vanilla" => &["Vanilla", "Paper", "Fabric"],
        "Paper" | "Purpur" | "Spigot" => &["Paper", "Purpur", "Spigot"],
        "Fabric" => &["Fabric"],
        "Forge" => &["Forge"],
        "NeoForge" => &["NeoForge"],
        "Mohist" => &["Mohist"],
        "Youer" => &["Youer"],
        "Arclight" => &["Arclight"],
        _ => &[],
    }
}

/// Change le logiciel ou la version d'un serveur arrêté. Le monde est sauvegardé juste avant.
#[tauri::command]
async fn changer_version(app: AppHandle, id: String, logiciel: String, version: String) -> Result<ServeurVu, String> {
    a_part(move || {
        let etat = app.state::<Etat>();
        let serveur = serveur_de(&etat, &id)?;
        arrete(&etat, &id)?;
        if !familles(&serveur.logiciel).contains(&logiciel.as_str()) {
            return Err("Ce changement n'est pas possible.".into());
        }
        if logiciel == serveur.logiciel && version == serveur.version {
            return Err("C'est déjà cette version.".into());
        }
        let _travail = commencer_travail(&etat, &id)?;
        let dossier = PathBuf::from(&serveur.dossier);
        if serveur.sauvegarde.avant_maj {
            faire_sauvegarde(&app, &serveur, "maj", "tout", &mondes::niveau(&dossier), Some(format!("Avant {logiciel} {version}")))?;
        }
        let jar_actuel = (logiciel == serveur.logiciel).then_some(serveur.jar.as_str());
        let (jar, java_snapshot) = installer_logiciel(&app, &logiciel, &version, &dossier, jar_actuel, &mut |tache, p| {
            dire_travail(&app, &id, if tache == 0 { "Java" } else { "Installation" }, p)
        })?;
        let mut liste = etat.liste.lock().unwrap();
        let s = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
        s.logiciel = logiciel;
        s.version = version;
        let ancien = std::mem::replace(&mut s.jar, jar);
        s.java = java_snapshot;
        let copie = s.clone();
        serveurs::enregistrer(&etat.fichier, &liste)?;
        drop(liste);
        // Arclight garde le vrai nom de son fichier : l'ancien ne sert plus.
        if copie.logiciel == "Arclight" && ancien != copie.jar && ancien.to_ascii_lowercase().starts_with("arclight-") && !ancien.contains(['/', '\\']) {
            let _ = std::fs::remove_file(dossier.join(&ancien));
        }
        Ok(vu(&etat, &copie))
    })
    .await
}

/// À la fermeture de BHOST : on arrête proprement les serveurs qui tournent,
/// pour que les mondes soient sauvegardés. Après 20 secondes, on coupe.
/// Ouvert à tes amis : le fil qui ouvre les ports sur la box (acces.rs).
fn ouvrir_en_continu(app: AppHandle) {
    let lire = app.clone();
    let voulus = move || {
        let etat = lire.state::<Etat>();
        let liste: Vec<Serveur> = etat.liste.lock().unwrap().iter().filter(|s| s.ouvert_amis).cloned().collect();
        liste
            .iter()
            .map(|s| {
                let dossier = Path::new(&s.dossier);
                let server_ip = serveurs::lire_proprietes(dossier).into_iter().find(|(c, _)| c == "server-ip").map(|(_, v)| v).unwrap_or_default();
                // Joueurs Bedrock autorisés : leur port s'ouvre aussi.
                let bedrock = s.bedrock_port.filter(|_| bedrock::actif(dossier, &s.logiciel, &s.version));
                acces::Voulu { id: s.id.clone(), nom: s.nom.clone(), port: serveurs::port(dossier), server_ip, bedrock }
            })
            .collect()
    };
    let envoi = app.clone();
    let etat = app.state::<Etat>();
    // Le relais ne branche un joueur que sur un serveur allumé, ou endormi avec BHOST qui écoute pour le réveiller.
    let qui = app.clone();
    etat.acces.tunnels.regler_actif(std::sync::Arc::new(move |id: &str| {
        let etat = qui.state::<Etat>();
        let statut = etat.statuts.lock().unwrap().get(id).cloned().unwrap_or_default();
        matches!(statut.as_str(), "demarrage" | "en-ligne" | "arret") || automatique::ecoute(&etat.auto, id)
    }));
    etat.acces.demarrer(etat.dossier.clone(), voulus, move |vues| {
        let _ = envoi.emit("acces", vues);
    });
}

/// Ce que montrent l'accueil et les paramètres : adresse pour les amis, ou erreur.
#[tauri::command]
fn etats_acces(etat: State<Etat>) -> Vec<acces::Acces> {
    etat.acces.vues()
}

/// Paramètres › Général › Ouvert à tes amis.
#[tauri::command]
fn ouvrir_aux_amis(app: AppHandle, id: String, ouvert: bool, etat: State<Etat>) -> Result<ServeurVu, String> {
    let mut liste = etat.liste.lock().unwrap();
    let serveur = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
    serveur.ouvert_amis = ouvert;
    let copie = serveur.clone();
    serveurs::enregistrer(&etat.fichier, &liste)?;
    drop(liste);
    let vues = etat.acces.reveiller(&id, ouvert);
    let _ = app.emit("acces", vues);
    Ok(vu(&etat, &copie))
}

/// Arrête tous les serveurs proprement (« stop ») et attend qu'ils aient fini d'enregistrer leur monde.
/// Au bout de 90 s, ceux qui ne répondent plus sont coupés.
pub(crate) fn tout_arreter(app: &AppHandle) {
    let etat = app.state::<Etat>();
    let en_cours: Vec<(String, Arc<Processus>)> = etat.vivants.lock().unwrap().iter().map(|(id, p)| (id.clone(), p.clone())).collect();
    for (id, p) in &en_cours {
        p.arret_demande.store(true, Ordering::SeqCst);
        p.relancer.store(false, Ordering::SeqCst);
        dire_statut(app, id, "arret");
        let _ = p.ecrire("stop");
    }
    // Un gros monde peut mettre du temps à s'enregistrer : on lui laisse le temps.
    let limite = Instant::now() + Duration::from_secs(90);
    while !etat.vivants.lock().unwrap().is_empty() && Instant::now() < limite {
        std::thread::sleep(Duration::from_millis(100));
    }
    for p in etat.vivants.lock().unwrap().values() {
        p.tuer();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Toujours en premier : BHOST relancé alors qu'il tourne déjà (même rangé) → on réaffiche la fenêtre.
        .plugin(tauri_plugin_single_instance::init(|app, _arguments, _dossier| zone::montrer(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![ARRIERE_PLAN])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(mise_a_jour::EtatMaj::default())
        // Bouton ✕ : avec « Garder les serveurs allumés », BHOST se range dans la zone de notification.
        .on_window_event(|fenetre, evenement| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = evenement {
                if fenetre.label() == "main" && GARDER.load(Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = fenetre.hide();
                    if !DEJA_RANGE.swap(true, Ordering::Relaxed) {
                        use tauri_plugin_notification::NotificationExt;
                        let _ = fenetre.app_handle().notification().builder().title("BHOST continue en arrière-plan").body("Tes serveurs restent allumés.").show();
                    }
                }
            }
        })
        .setup(|app| {
            // Les données de BHOST vont dans %APPDATA%\BHOST\ (avant le nouveau nom : %APPDATA%\Cubo\, déplacé une fois).
            let appdata = app.path().data_dir()?;
            let dossier = serveurs::demenager(&appdata.join("Cubo"), &appdata.join("BHOST"));
            std::fs::create_dir_all(&dossier)?;
            let fichier = dossier.join("serveurs.json");
            let liste = serveurs::charger(&fichier);
            let auto = automatique::EtatAuto::nouveau(&dossier);
            let dossier_donnees = dossier.clone();
            app.manage(Etat {
                dossier,
                reseau: reseau::agent(),
                icones: Mutex::new(icone::Icones::default()),
                fichier,
                liste: Mutex::new(liste),
                vivants: Mutex::new(HashMap::new()),
                preparation: Mutex::new(HashSet::new()),
                copies: Mutex::new(HashSet::new()),
                travaux: Mutex::new(HashSet::new()),
                regles_envoyees: Mutex::new(HashMap::new()),
                installation_java: Mutex::new(()),
                buildtools: Mutex::new(()),
                statuts: Mutex::new(HashMap::new()),
                journaux: Mutex::new(HashMap::new()),
                depuis: Mutex::new(HashMap::new()),
                joueurs: Mutex::new(HashMap::new()),
                arrivees: Mutex::new(HashMap::new()),
                identifiants: Mutex::new(HashMap::new()),
                tps_regarde: Mutex::new(None),
                tps: Mutex::new(HashMap::new()),
                auto,
                acces: Arc::new(acces::EtatAcces::default()),
                demandes: Mutex::new(demandes::Registre::charger(&dossier_donnees.join(FICHIER_DEMANDES))),
                catalogue: catalogue::EtatCatalogue::nouveau(&dossier_donnees),
            });
            // Les réglages Notifications et Empêcher la veille, lus une fois au lancement.
            {
                let reglages = lire_reglages(&app.state::<Etat>());
                NOTIFICATIONS.store(reglages.notifications, Ordering::Relaxed);
                VEILLE.store(reglages.veille, Ordering::Relaxed);
                GARDER.store(reglages.garder, Ordering::Relaxed);
                PROPREMENT.store(reglages.proprement, Ordering::Relaxed);
            }
            // La zone de notification (en bas à droite) et l'extinction de Windows.
            if let Err(erreur) = zone::installer(app.handle()) {
                eprintln!("[BHOST] Zone de notification : {erreur}");
            }
            #[cfg(windows)]
            extinction::surveiller(app.handle());
            // Lancer avec Windows : l'ancienne inscription n'avait pas l'option « --arriere-plan », on la refait.
            {
                use tauri_plugin_autostart::ManagerExt;
                if app.autolaunch().is_enabled().unwrap_or(false) {
                    let _ = app.autolaunch().enable();
                }
            }
            // La fenêtre est cachée au départ (tauri.conf.json) : on la montre, sauf lancement avec Windows
            // quand BHOST doit rester rangé.
            let range = GARDER.load(Ordering::Relaxed) && std::env::args().any(|a| a == ARRIERE_PLAN);
            if !range {
                zone::montrer(app.handle());
            } else {
                DEJA_RANGE.store(true, Ordering::Relaxed);
            }
            mesurer_en_continu(app.handle().clone());
            sauvegarder_en_continu(app.handle().clone());
            automatique::demarrer(app.handle().clone());
            ouvrir_en_continu(app.handle().clone());
            problemes::surveiller_internet(app.handle().clone());
            // Paramètres › Démarrer avec BHOST : on allume ces serveurs tout de suite.
            let handle = app.handle().clone();
            let a_demarrer: Vec<String> = handle.state::<Etat>().liste.lock().unwrap().iter().filter(|s| s.demarrer_auto).map(|s| s.id.clone()).collect();
            for id in a_demarrer {
                if let Err(erreur) = lancer_serveur(&handle, &id) {
                    eprintln!("[BHOST] Démarrage de {id} : {erreur}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            lister_serveurs,
            machine::lister_cles_usb,
            machine::machines_liees,
            machine::lier_machine,
            machine::delier_machine,
            machine::renommer_machine,
            importer_serveur,
            demarrer_serveur,
            installer_java_serveur,
            problemes::problemes_serveurs,
            problemes::etat_internet,
            problemes::verifier_internet,
            arreter_serveur,
            redemarrer_serveur,
            forcer_arret,
            envoyer_commande,
            regarder_tps,
            tete_joueur,
            renommer_serveur,
            infos_creation,
            changer_dossier_serveurs,
            versions_minecraft,
            creer_serveur,
            couleur_serveur,
            parametres_serveur,
            regler_serveur,
            regler_partie,
            dupliquer_serveur,
            supprimer_serveur,
            retirer_serveur,
            ouvrir_dossier,
            changer_icone,
            lire_console,
            regles_serveur,
            regler_regle,
            mondes_serveur,
            utiliser_monde,
            nouveau_monde,
            supprimer_monde,
            reinitialiser_monde,
            exporter_monde,
            mondes_solo,
            importer_monde,
            changer_version,
            joueurs_serveur,
            action_joueur,
            repondre_demande,
            liste_blanche,
            fiche_joueur,
            commande_joueur,
            sauvegardes_serveur,
            regler_sauvegardes,
            sauvegarder_maintenant,
            epingler_sauvegarde,
            exporter_sauvegarde,
            supprimer_sauvegarde,
            restaurer_sauvegarde,
            ouvrir_sauvegardes,
            fichiers_serveur,
            lire_fichier,
            ecrire_fichier,
            importer_fichiers,
            icone_serveur,
            icone_du_jeu,
            montrer_fichier,
            proprietes_serveur,
            regler_proprietes,
            automatique::automatisations_serveur,
            automatique::enregistrer_automatisation,
            automatique::supprimer_automatisation,
            automatique::activer_automatisation,
            etats_acces,
            ouvrir_aux_amis,
            reglages_bhost,
            regler_bhost,
            ouvrir_dossier_serveurs,
            catalogue::extensions_serveur,
            catalogue::chercher_extensions,
            catalogue::installer_extension,
            catalogue::activer_extension,
            catalogue::datapacks_manquants,
            catalogue::remettre_datapack,
            catalogue::desinstaller_extension,
            catalogue::mettre_a_jour_extensions,
            catalogue::bedrock_serveur,
            catalogue::regler_bedrock,
            mise_a_jour::chercher_mise_a_jour,
            mise_a_jour::installer_mise_a_jour
        ])
        .build(tauri::generate_context!())
        .expect("BHOST n'a pas pu démarrer")
        .run(|app, evenement| {
            if let RunEvent::Exit = evenement {
                automatique::FERMETURE.store(true, Ordering::Relaxed);
                tout_arreter(app);
                app.state::<Etat>().acces.tout_fermer();
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn lit_la_version_d_un_java_choisi() {
        let d = std::env::temp_dir().join(format!("bhost-java-{}", std::process::id()));
        std::fs::create_dir_all(d.join("bin")).unwrap();
        std::fs::write(d.join("release"), "IMPLEMENTOR=\"Azul\"\nJAVA_VERSION=\"21.0.6\"\n").unwrap();
        assert_eq!(super::java_d_un_programme(&d.join("bin").join("java.exe")), Some(21));
        std::fs::write(d.join("release"), "JAVA_VERSION=\"1.8.0_402\"\n").unwrap();
        assert_eq!(super::java_d_un_programme(&d.join("bin").join("java.exe")), Some(8));
        assert_eq!(super::java_d_un_programme(std::path::Path::new("java.exe")), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    use super::*;

    #[test]
    fn sait_ou_ranger_les_operateurs() {
        assert!(ops_en_texte("1.2.5") && ops_en_texte("1.6.4") && ops_en_texte("1.7") && ops_en_texte("1.7.5"));
        assert!(!ops_en_texte("1.7.6") && !ops_en_texte("1.7.10") && !ops_en_texte("1.8.8") && !ops_en_texte("1.21.4"));
        assert!(!ops_en_texte("24w14a") && !ops_en_texte(""));
    }

    #[test]
    fn lit_les_reglages() {
        let r: ReglagesBhost = serde_json::from_str(r#"{"dossierServeurs":"D:\\Serveurs"}"#).unwrap();
        assert!(r.notifications && r.veille && r.pseudo.is_none());
        let r: ReglagesBhost = serde_json::from_str(r#"{"veille":false,"pseudo":"KOTORO_YT"}"#).unwrap();
        assert!(!r.veille && r.notifications);
        assert_eq!(r.pseudo.as_deref(), Some("KOTORO_YT"));
    }

    #[test]
    fn coupe_les_arguments_en_plus() {
        assert_eq!(decouper(r#"-Da=1  "-Dnom=mon serveur" -Xss4M"#), ["-Da=1", "-Dnom=mon serveur", "-Xss4M"]);
        assert!(decouper("   ").is_empty());
    }

    #[test]
    fn construit_la_commande() {
        let mut s: Serveur = serde_json::from_value(serde_json::json!({
            "id": "s1", "nom": "x", "dossier": "", "logiciel": "Paper", "version": "1.21.11", "jar": "server.jar", "ramMo": 4096
        }))
        .unwrap();
        assert_eq!(arguments_java(&s), ["-Xmx4096M", "-Djline.terminal=jline.UnsupportedTerminal", "-jar", "server.jar", "nogui"]);
        let forge = Serveur { jar: "@libraries/net/minecraftforge/forge/1.21.11-61.2.4/win_args.txt".into(), ..s.clone() };
        assert_eq!(arguments_java(&forge)[2..], ["@libraries/net/minecraftforge/forge/1.21.11-61.2.4/win_args.txt", "nogui"]);
        s.optimise = true;
        s.arguments = "-Dfile.encoding=UTF-8".into();
        let a = arguments_java(&s);
        assert_eq!(a.len(), 1 + OPTIONS_OPTIMISEES.len() + 1 + 4);
        assert_eq!(a[a.len() - 5], "-Dfile.encoding=UTF-8");
        assert!(matches!(choix_java(&s), ChoixJava::Auto));
        s.java_choix = Some("17".into());
        assert!(matches!(choix_java(&s), ChoixJava::Version(17)));
        s.java_choix = Some(r"C:\Java\bin\java.exe".into());
        assert!(matches!(choix_java(&s), ChoixJava::Fichier(_)));
    }

    #[test]
    fn verifie_les_proprietes() {
        assert_eq!(propriete_valide("motd", "Café\nx").unwrap(), "Caf\\u00e9x");
        assert_eq!(propriete_valide("server-port", " 25570 ").unwrap(), "25570");
        assert!(propriete_valide("server-port", "80").is_err());
        assert!(propriete_valide("pvp", "oui").is_err());
        assert_eq!(propriete_valide("pvp", "false").unwrap(), "false");
        assert!(propriete_valide("view-distance", "64").is_err());
    }

    #[test]
    fn reconnait_les_vieilles_versions() {
        assert!(avant_1_13("1.8.8") && avant_1_13("1.12.2"));
        assert!(!avant_1_13("1.13") && !avant_1_13("26.3") && !avant_1_13(""));
    }

    #[test]
    fn familles_de_logiciels() {
        assert!(familles("Vanilla").contains(&"Fabric"));
        assert!(familles("Spigot").contains(&"Paper") && familles("Paper").contains(&"Spigot"));
        assert_eq!(familles("Mohist"), ["Mohist"]);
        assert!(familles("Inconnu").is_empty());
    }
}
