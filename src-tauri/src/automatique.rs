// Le moteur des automatisations : un fil qui regarde l'heure toutes les deux secondes,
// et des « crochets » appelés par le reste de BHOST (serveur prêt, crash, joueur qui arrive, mesures).
// Il fait aussi dormir les serveurs qui ont « Quand quelqu'un veut entrer » : voir reveil.rs.
// Les règles et les calculs sont dans automatisations.rs (testés sans Tauri).

use crate::automatisations::{self as au, Automatisation, Fait, Memoire, Relances, Seuil};
use crate::reveil::{self, Veilleur};
use crate::serveurs::{self, Serveur};
use crate::{Etat, commander, dire_ligne, en_ligne, lancer_serveur, maintenant_ms, mondes, tps};
use chrono::Local;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use crate::hote::{AppHandle, Emitter, Manager, State};

/// Ce que le moteur garde en mémoire.
pub struct EtatAuto {
    fichier: PathBuf,
    memoire: Mutex<Memoire>,
    interne: Mutex<Interne>,
    veilleurs: Mutex<HashMap<String, Veille>>,
}

/// BHOST écoute sur le port de ce serveur endormi (pour le réveiller quand quelqu'un arrive).
pub fn ecoute(auto: &EtatAuto, id: &str) -> bool {
    matches!(auto.veilleurs.lock().unwrap().get(id), Some(Veille::Ecoute(_)))
}

impl EtatAuto {
    /// `dossier` : %APPDATA%\BHOST (l'historique va dans automatisations.json).
    pub fn nouveau(dossier: &Path) -> EtatAuto {
        let fichier = dossier.join("automatisations.json");
        EtatAuto { memoire: Mutex::new(Memoire::charger(&fichier)), fichier, interne: Mutex::default(), veilleurs: Mutex::default() }
    }
}

/// Le réveil d'un serveur : BHOST écoute sur son port, ou n'a pas pu (port pris).
enum Veille {
    Ecoute(Veilleur),
    Echec(Instant),
}

#[derive(Default)]
struct Interne {
    /// Depuis quand chaque serveur en ligne n'a personne.
    vide_depuis: HashMap<String, Instant>,
    /// « Il est vide » déjà fait (serveur/automatisation), jusqu'à ce que quelqu'un revienne.
    vide_fait: HashSet<String>,
    /// Serveurs redémarrés par « Il est vide » : leur redémarrage ne compte pas comme un retour.
    relance_vide: HashSet<String>,
    /// Lag et mémoire (serveur/automatisation).
    seuils: HashMap<String, Seuil>,
    /// Relances après un crash, par serveur.
    relances: HashMap<String, Relances>,
    /// Serveurs en train de démarrer : on n'écoute pas sur leur port.
    en_reveil: HashSet<String>,
    /// « Une mise à jour sort » : quand regarder à nouveau, par serveur.
    maj_prochaine: HashMap<String, Instant>,
    /// Serveurs dont on regarde les mises à jour en ce moment.
    maj_en_cours: HashSet<String>,
    /// Mises à jour trouvées pendant que le serveur tournait : posées à son prochain arrêt ou redémarrage.
    maj_attente: HashSet<String>,
    /// Ce qu'on a déjà annoncé (serveur/automatisation → fichiers et versions) : on ne prévient qu'une fois.
    maj_annoncees: HashMap<String, String>,
}

/// BHOST se ferme : on ne lance plus de mise à jour à l'arrêt des serveurs.
pub static FERMETURE: AtomicBool = AtomicBool::new(false);
/// On regarde les mises à jour toutes les 6 heures (la première fois, 2 minutes après l'ouverture de BHOST).
const MAJ_TOUTES_LES: Duration = Duration::from_secs(6 * 3600);
const MAJ_PREMIERE: Duration = Duration::from_secs(2 * 60);
/// Le catalogue n'a pas répondu : on réessaie dans 30 minutes.
const MAJ_REESSAI: Duration = Duration::from_secs(30 * 60);

const MOTD_DORT: &str = "Le serveur dort. Rejoins pour le réveiller !";
const MESSAGE_REVEIL: &str = "Le serveur se réveille. Reviens dans 1 min !";

fn cle(id: &str, aid: &str) -> String {
    format!("{id}/{aid}")
}

/// Les automatisations actives d'un déclencheur, pour un serveur.
fn actives(etat: &Etat, id: &str, trig: &str) -> Option<(Serveur, Vec<Automatisation>)> {
    let serveur = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned()?;
    let liste: Vec<Automatisation> = serveur.automatisations.iter().filter(|a| a.on && a.trig == trig).cloned().collect();
    (!liste.is_empty()).then_some((serveur, liste))
}

fn statut(etat: &Etat, id: &str) -> String {
    etat.statuts.lock().unwrap().get(id).cloned().unwrap_or_else(|| "arrete".into())
}

fn allume(etat: &Etat, id: &str) -> bool {
    etat.vivants.lock().unwrap().contains_key(id) || etat.preparation.lock().unwrap().contains(id)
}

/// Une notification Windows.
fn notifier(app: &AppHandle, titre: &str, texte: &str) {
    crate::hote::notifier(app, titre, texte, true);
}

/// Note une action faite dans « Dernières actions » et prévient l'interface.
fn noter(app: &AppHandle, id: &str, act: &str, raison: &str) {
    let etat = app.state::<Etat>();
    {
        let mut memoire = etat.auto.memoire.lock().unwrap();
        memoire.noter(id, Fait { quoi: format!("{} · {raison}", au::nom_action(act)), act: act.into(), quand: maintenant_ms() });
        memoire.enregistrer(&etat.auto.fichier);
    }
    let _ = app.emit("automatisations", id);
}

/// Fait l'action, à part (une sauvegarde peut être longue). `raison` va dans l'historique.
fn executer(app: &AppHandle, serveur: &Serveur, a: &Automatisation, raison: String, joueur: Option<String>) {
    let (app, serveur, a) = (app.clone(), serveur.clone(), a.clone());
    std::thread::spawn(move || match faire(&app, &serveur, &a, joueur.as_deref()) {
        Ok(true) => noter(&app, &serveur.id, &a.act, &raison),
        Ok(false) => {}
        Err(erreur) => dire_ligne(&app, &serveur.id, &format!("[BHOST!] Automatisation : {erreur}")),
    });
}

/// L'action elle-même. Ok(false) : rien à faire (déjà allumé, serveur arrêté pour une annonce…).
fn faire(app: &AppHandle, s: &Serveur, a: &Automatisation, joueur: Option<&str>) -> Result<bool, String> {
    let etat = app.state::<Etat>();
    let qui = joueur.unwrap_or("");
    match a.act.as_str() {
        "start" => {
            if allume(&etat, &s.id) {
                return Ok(false);
            }
            lancer_serveur(app, &s.id)?;
            Ok(true)
        }
        "stop" | "restart" => {
            if !etat.vivants.lock().unwrap().contains_key(&s.id) {
                return Ok(false);
            }
            if a.act == "stop" {
                crate::arreter(app, &s.id)?;
            } else {
                crate::redemarrer_serveur(app.clone(), s.id.clone(), app.state::<Etat>())?;
            }
            Ok(true)
        }
        "backup" => {
            let _travail = crate::commencer_travail(&etat, &s.id)?;
            let monde = mondes::niveau(Path::new(&s.dossier));
            crate::faire_sauvegarde(app, s, "auto", "tout", &monde, Some("Automatisation".into()))?;
            Ok(true)
        }
        "say" | "cmd" => {
            let Some(p) = en_ligne(&etat, &s.id) else { return Ok(false) };
            let commande = if a.act == "say" { format!("say {}", au::remplir(&a.msg, qui)) } else { au::remplir(&a.cmd, qui) };
            commander(app, &s.id, &p, &commande)?;
            Ok(true)
        }
        // Rien ici : « Une mise à jour sort » a son propre chemin (mises_a_jour).
        "update" => Ok(false),
        "notify" => {
            let texte = match a.trig.as_str() {
                "crash" => "Le serveur a planté.".to_string(),
                "join" => format!("{qui} vient d'arriver."),
                "boot" => "Le serveur est prêt.".to_string(),
                "lag" => "Le serveur lag.".to_string(),
                "mem" => "La mémoire est presque pleine.".to_string(),
                _ => format!("Rappel : il est {}.", Local::now().format("%H:%M")),
            };
            notifier(app, &s.nom, &texte);
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Le fil du moteur : l'heure, les serveurs vides, le réveil, les ticks par seconde à surveiller.
pub fn demarrer(app: AppHandle) {
    std::thread::spawn(move || {
        let mut avant = Local::now().naive_local();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let maintenant = Local::now().naive_local();
            // PC en veille, heure changée : on ne rattrape pas ce qui est passé.
            let ecart = maintenant - avant;
            if ecart > chrono::Duration::zero() && ecart < chrono::Duration::minutes(2) {
                heures(&app, avant, maintenant);
            }
            avant = maintenant;
            vides(&app);
            veiller(&app);
            surveiller_tps(&app);
            mises_a_jour(&app);
        }
    });
}

/// « À une heure » : les avertissements aux joueurs, puis l'action.
fn heures(app: &AppHandle, avant: chrono::NaiveDateTime, maintenant: chrono::NaiveDateTime) {
    let etat = app.state::<Etat>();
    let serveurs: Vec<Serveur> = etat.liste.lock().unwrap().iter().filter(|s| s.automatisations.iter().any(|a| a.on && a.trig == "time")).cloned().collect();
    for s in &serveurs {
        for a in s.automatisations.iter().filter(|a| a.on && a.trig == "time") {
            for secondes in au::echeances(a, avant, maintenant) {
                if secondes == 0 {
                    executer(app, s, a, maintenant.format("%H:%M").to_string(), None);
                } else if let Some(p) = en_ligne(&etat, &s.id) {
                    let _ = commander(app, &s.id, &p, &format!("say {}", au::message_avant(&a.act, secondes)));
                }
            }
        }
    }
}

/// « Il est vide » : personne depuis N minutes (une fois, jusqu'à ce que quelqu'un revienne).
fn vides(app: &AppHandle) {
    let etat = app.state::<Etat>();
    let serveurs: Vec<Serveur> = etat.liste.lock().unwrap().iter().filter(|s| s.automatisations.iter().any(|a| a.on && a.trig == "empty")).cloned().collect();
    let maintenant = Instant::now();
    for s in &serveurs {
        let personne = statut(&etat, &s.id) == "en-ligne" && etat.joueurs.lock().unwrap().get(&s.id).is_none_or(|l| l.is_empty());
        let mut a_faire = Vec::new();
        {
            let mut interne = etat.auto.interne.lock().unwrap();
            if !personne {
                interne.vide_depuis.remove(&s.id);
                continue;
            }
            let depuis = *interne.vide_depuis.entry(s.id.clone()).or_insert(maintenant);
            for a in s.automatisations.iter().filter(|a| a.on && a.trig == "empty") {
                if maintenant.duration_since(depuis) >= Duration::from_secs(a.mins as u64 * 60) && interne.vide_fait.insert(cle(&s.id, &a.id)) {
                    if a.act == "restart" {
                        interne.relance_vide.insert(s.id.clone());
                    }
                    a_faire.push(a.clone());
                }
            }
        }
        for a in a_faire {
            executer(app, s, &a, format!("personne depuis {} min", a.mins), None);
        }
    }
}

/// Le réveil : BHOST écoute sur le port des serveurs endormis qui ont « Quand quelqu'un veut entrer ».
fn veiller(app: &AppHandle) {
    let etat = app.state::<Etat>();
    let liste: Vec<Serveur> = etat.liste.lock().unwrap().clone();
    let mut a_arreter = Vec::new();
    for s in &liste {
        let reveil = s.automatisations.iter().find(|a| a.on && a.trig == "wake");
        let en_reveil = etat.auto.interne.lock().unwrap().en_reveil.contains(&s.id);
        let dort = reveil.is_some()
            && matches!(statut(&etat, &s.id).as_str(), "arrete" | "plante")
            && !allume(&etat, &s.id)
            && !etat.travaux.lock().unwrap().contains(&s.id)
            && !en_reveil;
        let mut veilleurs = etat.auto.veilleurs.lock().unwrap();
        match (dort, veilleurs.get(&s.id)) {
            // Quelqu'un est entré : le veilleur s'est arrêté (reveiller() s'occupe du reste).
            (_, Some(Veille::Ecoute(v))) if v.fini() => {
                veilleurs.remove(&s.id);
            }
            (false, Some(_)) => {
                if let Some(Veille::Ecoute(v)) = veilleurs.remove(&s.id) {
                    a_arreter.push(v);
                }
            }
            (true, None) => {
                let v = ecouter(app, s, reveil.map(|a| a.motd.as_str()).unwrap_or(""), true);
                veilleurs.insert(s.id.clone(), v);
            }
            (true, Some(Veille::Echec(depuis))) if depuis.elapsed() > Duration::from_secs(30) => {
                let v = ecouter(app, s, reveil.map(|a| a.motd.as_str()).unwrap_or(""), false);
                veilleurs.insert(s.id.clone(), v);
            }
            _ => {}
        }
    }
    // Hors du verrou : arrêter un veilleur attend la fin de son fil.
    for v in a_arreter {
        v.arreter();
    }
}

/// Lance l'écoute sur le port du serveur. `dire` : écrire dans la console si le port est pris.
fn ecouter(app: &AppHandle, s: &Serveur, motd: &str, dire: bool) -> Veille {
    use base64::Engine;
    let dossier = Path::new(&s.dossier);
    let port = serveurs::port(dossier);
    let icone = std::fs::read(dossier.join("server-icon.png"))
        .ok()
        .filter(|o| o.len() < 64 * 1024)
        .map(|o| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(o)));
    let infos = reveil::Infos {
        motd: if motd.is_empty() { MOTD_DORT.into() } else { motd.into() },
        version: format!("{} {}", s.logiciel, s.version).trim().to_string(),
        joueurs_max: serveurs::joueurs_max(dossier),
        icone,
        message: MESSAGE_REVEIL.into(),
    };
    let (app2, id) = (app.clone(), s.id.clone());
    match Veilleur::lancer(port, infos, move |pseudo| reveiller(&app2, &id, &pseudo)) {
        Ok(v) => Veille::Ecoute(v),
        Err(_) => {
            if dire {
                dire_ligne(app, &s.id, &format!("[BHOST!] Réveil impossible : le port {port} est pris. Nouvel essai toutes les 30 s."));
            }
            Veille::Echec(Instant::now())
        }
    }
}

/// Quelqu'un veut entrer : on allume le serveur (appelé par le veilleur, port déjà libéré).
fn reveiller(app: &AppHandle, id: &str, pseudo: &str) {
    let etat = app.state::<Etat>();
    etat.auto.interne.lock().unwrap().en_reveil.insert(id.to_string());
    etat.auto.veilleurs.lock().unwrap().remove(id);
    // Le pseudo vient d'Internet : on ne l'affiche que s'il ressemble à un vrai pseudo.
    let qui = if crate::listes::pseudo_valide(pseudo) { pseudo.to_string() } else { "Quelqu'un".to_string() };
    dire_ligne(app, id, &format!("[BHOST] {qui} veut entrer : le serveur se réveille."));
    match lancer_serveur(app, id) {
        Ok(()) => noter(app, id, "start", &format!("réveillé par {qui}")),
        Err(erreur) => dire_ligne(app, id, &format!("[BHOST!] {erreur}")),
    }
    etat.auto.interne.lock().unwrap().en_reveil.remove(id);
}

/// Avant chaque démarrage : BHOST arrête d'écouter sur le port. Tant que la garde vit,
/// il ne recommence pas (le temps que le serveur soit noté « démarrage »).
pub struct Garde {
    app: AppHandle,
    id: String,
}

impl Drop for Garde {
    fn drop(&mut self) {
        self.app.state::<Etat>().auto.interne.lock().unwrap().en_reveil.remove(&self.id);
    }
}

pub fn liberer_port(app: &AppHandle, id: &str) -> Garde {
    let etat = app.state::<Etat>();
    etat.auto.interne.lock().unwrap().en_reveil.insert(id.to_string());
    let veille = etat.auto.veilleurs.lock().unwrap().remove(id);
    if let Some(Veille::Ecoute(v)) = veille {
        v.arreter();
    }
    Garde { app: app.clone(), id: id.to_string() }
}

/// « Une mise à jour sort » : toutes les 6 heures, on demande au catalogue ce qui a une mise à jour (à part).
fn mises_a_jour(app: &AppHandle) {
    let etat = app.state::<Etat>();
    let serveurs: Vec<Serveur> = etat.liste.lock().unwrap().iter().filter(|s| s.automatisations.iter().any(|a| a.on && a.trig == "upd")).cloned().collect();
    let maintenant = Instant::now();
    let mut interne = etat.auto.interne.lock().unwrap();
    // Plus d'automatisation active : on oublie ce serveur.
    let ids: HashSet<&str> = serveurs.iter().map(|s| s.id.as_str()).collect();
    interne.maj_prochaine.retain(|id, _| ids.contains(id.as_str()));
    interne.maj_attente.retain(|id| ids.contains(id.as_str()));
    for s in serveurs {
        let prochaine = *interne.maj_prochaine.entry(s.id.clone()).or_insert(maintenant + MAJ_PREMIERE);
        if prochaine > maintenant || interne.maj_en_cours.contains(&s.id) {
            continue;
        }
        interne.maj_prochaine.insert(s.id.clone(), maintenant + MAJ_TOUTES_LES);
        interne.maj_en_cours.insert(s.id.clone());
        let app = app.clone();
        std::thread::spawn(move || {
            let reessayer = regarder_mises_a_jour(&app, &s).is_err();
            let etat = app.state::<Etat>();
            let mut interne = etat.auto.interne.lock().unwrap();
            interne.maj_en_cours.remove(&s.id);
            if reessayer {
                interne.maj_prochaine.insert(s.id.clone(), Instant::now() + MAJ_REESSAI);
            }
        });
    }
}

/// Regarde les mises à jour d'un serveur, puis prévient ou met à jour (serveur arrêté),
/// ou les garde pour son prochain arrêt (serveur allumé : Windows bloque les fichiers).
fn regarder_mises_a_jour(app: &AppHandle, s: &Serveur) -> Result<(), String> {
    let dispo = crate::catalogue::mises_a_jour_dispo(app, &s.id)?;
    let etat = app.state::<Etat>();
    let liste: Vec<Automatisation> = s.automatisations.iter().filter(|a| a.on && a.trig == "upd").cloned().collect();
    // « EssentialsX 2.21.1, Chunky 1.4.36 » : ce qui change d'une fois sur l'autre.
    let mut signature: Vec<String> = dispo.iter().map(|d| format!("{}|{}", d.fichier, d.version)).collect();
    signature.sort();
    let signature = signature.join(",");
    let noms: Vec<String> = dispo.iter().map(|d| d.nom.clone()).collect();
    for a in &liste {
        let cle = cle(&s.id, &a.id);
        if dispo.is_empty() {
            etat.auto.interne.lock().unwrap().maj_annoncees.remove(&cle);
            continue;
        }
        let nouveau = etat.auto.interne.lock().unwrap().maj_annoncees.get(&cle) != Some(&signature);
        match a.act.as_str() {
            "notify" if nouveau => {
                notifier(app, &s.nom, &format!("Mise à jour : {}.", au::resume_noms(&noms)));
                noter(app, &s.id, "notify", &au::resume_noms(&noms));
            }
            "update" if allume(&etat, &s.id) => {
                etat.auto.interne.lock().unwrap().maj_attente.insert(s.id.clone());
                if nouveau {
                    dire_ligne(app, &s.id, &format!("[BHOST] Mise à jour prête : {}. Elle sera faite au prochain arrêt.", au::resume_noms(&noms)));
                }
            }
            "update" => poser_mises_a_jour(app, &s.id),
            _ => {}
        }
        etat.auto.interne.lock().unwrap().maj_annoncees.insert(cle, signature.clone());
    }
    Ok(())
}

/// Met à jour les plugins et mods d'un serveur arrêté (personne ne peut le démarrer pendant ce temps).
fn poser_mises_a_jour(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    let Ok(_travail) = crate::commencer_travail(&etat, id) else {
        // Une autre opération est en cours : on réessaiera au prochain tour.
        etat.auto.interne.lock().unwrap().maj_attente.insert(id.to_string());
        return;
    };
    etat.auto.interne.lock().unwrap().maj_attente.remove(id);
    let resultat = crate::catalogue::mises_a_jour_dispo(app, id).and_then(|dispo| crate::catalogue::appliquer_mises_a_jour(app, id, &dispo));
    match resultat {
        Ok(noms) if !noms.is_empty() => {
            dire_ligne(app, id, &format!("[BHOST] Mis à jour : {}.", au::resume_noms(&noms)));
            noter(app, id, "update", &au::resume_noms(&noms));
        }
        Ok(_) => {}
        Err(erreur) => dire_ligne(app, id, &format!("[BHOST!] Mise à jour : {erreur}")),
    }
}

/// Le serveur s'est arrêté proprement (`relance` : il redémarre tout de suite).
/// Des mises à jour attendaient : on les pose maintenant, avant le redémarrage s'il y en a un.
pub fn arrete(app: &AppHandle, id: &str, relance: bool) {
    if FERMETURE.load(Ordering::Relaxed) {
        return;
    }
    let etat = app.state::<Etat>();
    if !etat.auto.interne.lock().unwrap().maj_attente.contains(id) {
        return;
    }
    if relance {
        dire_ligne(app, id, "[BHOST] Mise à jour des plugins et mods avant de redémarrer…");
        poser_mises_a_jour(app, id);
    } else {
        let (app, id) = (app.clone(), id.to_string());
        std::thread::spawn(move || poser_mises_a_jour(&app, &id));
    }
}

/// Demande les ticks par seconde aux serveurs qui ont « Ça lag » (les autres : seulement celui affiché).
fn surveiller_tps(app: &AppHandle) {
    let etat = app.state::<Etat>();
    let ids: Vec<String> = etat
        .liste
        .lock()
        .unwrap()
        .iter()
        .filter(|s| tps::commande(&s.logiciel, &s.version).is_some() && s.automatisations.iter().any(|a| a.on && a.trig == "lag"))
        .map(|s| s.id.clone())
        .collect();
    for id in ids {
        crate::demander_tps_de(app, &id);
    }
}

// ---------------------------------------------------------------------------------------------
// Crochets appelés par le reste de BHOST
// ---------------------------------------------------------------------------------------------

/// Le serveur vient de démarrer (« Done »).
pub fn pret(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    {
        let mut interne = etat.auto.interne.lock().unwrap();
        interne.vide_depuis.remove(id);
        // Redémarré par « Il est vide » : on attend toujours que quelqu'un revienne. Sinon on repart de zéro.
        if !interne.relance_vide.remove(id) {
            let prefixe = format!("{id}/");
            interne.vide_fait.retain(|c| !c.starts_with(&prefixe));
        }
    }
    if let Some((serveur, liste)) = actives(&etat, id, "boot") {
        for a in &liste {
            executer(app, &serveur, a, "au démarrage".into(), None);
        }
    }
}

/// Le serveur a planté (pas un arrêt demandé).
pub fn plante(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    let Some((serveur, liste)) = actives(&etat, id, "crash") else { return };
    for a in &liste {
        if a.act != "restart" {
            executer(app, &serveur, a, "après un crash".into(), None);
            continue;
        }
        let essai = etat.auto.interne.lock().unwrap().relances.entry(id.to_string()).or_default().essayer(3, Instant::now());
        match essai {
            Some(n) => {
                dire_ligne(app, id, &format!("[BHOST] Crash : relance dans 5 s (essai {n} sur 3)."));
                let (app, id) = (app.clone(), id.to_string());
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(5));
                    if allume(&app.state::<Etat>(), &id) {
                        return;
                    }
                    match lancer_serveur(&app, &id) {
                        Ok(()) => noter(&app, &id, "restart", "après un crash"),
                        Err(erreur) => dire_ligne(&app, &id, &format!("[BHOST!] {erreur}")),
                    }
                });
            }
            None => dire_ligne(app, id, "[BHOST!] 3 crashs en 15 min : BHOST ne relance plus."),
        }
    }
}

/// Un joueur vient d'arriver.
pub fn arrivee(app: &AppHandle, id: &str, nom: &str) {
    let etat = app.state::<Etat>();
    {
        let mut interne = etat.auto.interne.lock().unwrap();
        interne.vide_depuis.remove(id);
        interne.relance_vide.remove(id);
        let prefixe = format!("{id}/");
        interne.vide_fait.retain(|c| !c.starts_with(&prefixe));
    }
    let Some(serveur) = etat.liste.lock().unwrap().iter().find(|s| s.id == id).cloned() else { return };
    if !serveur.automatisations.iter().any(|a| a.trig == "join") {
        return;
    }
    // On suit les joueurs déjà venus dès qu'il existe une automatisation « Un joueur arrive ».
    let premiere = {
        let mut memoire = etat.auto.memoire.lock().unwrap();
        memoire.preparer_connus(id, Path::new(&serveur.dossier));
        let p = memoire.premiere_venue(id, nom);
        if p {
            memoire.enregistrer(&etat.auto.fichier);
        }
        p
    };
    for a in serveur.automatisations.iter().filter(|a| a.on && a.trig == "join") {
        if a.first && !premiere {
            continue;
        }
        executer(app, &serveur, a, format!("{nom} arrive"), Some(nom.to_string()));
    }
}

/// Un relevé des ticks par seconde (« Ça lag » : moins de 15 pendant 1 min).
pub fn tps_recu(app: &AppHandle, id: &str, valeur: f32) {
    let etat = app.state::<Etat>();
    let Some((serveur, liste)) = actives(&etat, id, "lag") else { return };
    for a in &liste {
        let agir = etat.auto.interne.lock().unwrap().seuils.entry(cle(id, &a.id)).or_default().mesurer(
            valeur < 15.0,
            Instant::now(),
            Duration::from_secs(60),
            Duration::from_secs(10 * 60),
        );
        if agir {
            executer(app, &serveur, a, "ça lag".into(), None);
        }
    }
}

/// Un relevé de la mémoire (« Mémoire pleine » : plus de 90 % pendant 5 min).
pub fn memoire_recue(app: &AppHandle, id: &str, memoire_mo: u64) {
    let etat = app.state::<Etat>();
    let Some((serveur, liste)) = actives(&etat, id, "mem") else { return };
    let pleine = serveur.ram_mo > 0 && memoire_mo * 100 > serveur.ram_mo as u64 * 90;
    for a in &liste {
        let agir = etat.auto.interne.lock().unwrap().seuils.entry(cle(id, &a.id)).or_default().mesurer(
            pleine,
            Instant::now(),
            Duration::from_secs(5 * 60),
            Duration::from_secs(30 * 60),
        );
        if agir {
            executer(app, &serveur, a, "mémoire pleine".into(), None);
        }
    }
}

/// Un serveur retiré de BHOST : on oublie son historique et on n'écoute plus sur son port.
pub fn oublier(app: &AppHandle, id: &str) {
    let etat = app.state::<Etat>();
    {
        let mut memoire = etat.auto.memoire.lock().unwrap();
        memoire.oublier(id);
        memoire.enregistrer(&etat.auto.fichier);
    }
    drop(liberer_port(app, id));
}

// ---------------------------------------------------------------------------------------------
// Commandes pour l'onglet Automatisations
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueAuto {
    liste: Vec<Automatisation>,
    historique: Vec<Fait>,
    /// Le logiciel donne ses ticks par seconde (« Ça lag » possible).
    tps_possible: bool,
    /// Le serveur a des plugins ou des mods (« Une mise à jour sort » possible).
    maj_possible: bool,
}

/// Plugins ou mods possibles sur ce serveur (pas seulement des datapacks).
fn maj_possible(serveur: &Serveur) -> bool {
    crate::extensions::genres(&serveur.logiciel, &serveur.version).iter().any(|g| *g != crate::extensions::Genre::Datapacks)
}

fn vue(etat: &Etat, id: &str) -> Result<VueAuto, String> {
    let serveur = crate::serveur_de(etat, id)?;
    Ok(VueAuto {
        historique: etat.auto.memoire.lock().unwrap().historique.get(id).cloned().unwrap_or_default(),
        tps_possible: tps::commande(&serveur.logiciel, &serveur.version).is_some(),
        maj_possible: maj_possible(&serveur),
        liste: serveur.automatisations,
    })
}

/// Change la liste d'un serveur, l'enregistre, puis relance le réveil (nouveau message…).
fn changer(app: &AppHandle, id: &str, change: impl FnOnce(&mut Vec<Automatisation>, &Serveur) -> Result<(), String>) -> Result<VueAuto, String> {
    let etat = app.state::<Etat>();
    {
        let mut liste = etat.liste.lock().unwrap();
        let serveur = liste.iter_mut().find(|s| s.id == id).ok_or("Ce serveur n'existe plus.")?;
        let copie = serveur.clone();
        change(&mut serveur.automatisations, &copie)?;
        serveurs::enregistrer(&etat.fichier, &liste)?;
    }
    // Le veilleur sera relancé au prochain tour, avec les nouveaux réglages.
    drop(liberer_port(app, id));
    vue(&etat, id)
}

#[cfg_attr(feature = "bureau", tauri::command)]
pub fn automatisations_serveur(id: String, etat: State<Etat>) -> Result<VueAuto, String> {
    vue(&etat, &id)
}

/// Ajoute (id vide) ou remplace une automatisation.
#[cfg_attr(feature = "bureau", tauri::command)]
pub fn enregistrer_automatisation(app: AppHandle, id: String, automatisation: Automatisation) -> Result<VueAuto, String> {
    let mut a = automatisation;
    let vue = changer(&app, &id, |liste, serveur| {
        au::verifier(&mut a, tps::commande(&serveur.logiciel, &serveur.version).is_some(), maj_possible(serveur))?;
        if a.trig == "time" && a.when == "once" && au::prochaine_apres(&a, Local::now().naive_local()).is_none() {
            return Err("Cette heure est passée.".into());
        }
        if a.trig == "wake" && liste.iter().any(|x| x.trig == "wake" && x.id != a.id) {
            return Err("Ce serveur a déjà un réveil.".into());
        }
        match liste.iter_mut().find(|x| !a.id.is_empty() && x.id == a.id) {
            Some(x) => *x = a.clone(),
            None => {
                if liste.len() >= au::MAX {
                    return Err(format!("{} automatisations au plus.", au::MAX));
                }
                a.id = format!("a{}", maintenant_ms());
                a.on = true;
                liste.push(a.clone());
            }
        }
        Ok(())
    })?;
    // « Première venue » : on note dès maintenant les joueurs déjà venus.
    // « Une mise à jour sort » : on regarde dans 2 minutes (pas besoin d'attendre 6 heures).
    if a.trig == "upd" {
        let etat = app.state::<Etat>();
        let mut interne = etat.auto.interne.lock().unwrap();
        interne.maj_prochaine.insert(id.clone(), Instant::now() + MAJ_PREMIERE);
        interne.maj_annoncees.remove(&cle(&id, &a.id));
    }
    if a.trig == "join" {
        let etat = app.state::<Etat>();
        if let Ok(serveur) = crate::serveur_de(&etat, &id) {
            let mut memoire = etat.auto.memoire.lock().unwrap();
            memoire.preparer_connus(&id, Path::new(&serveur.dossier));
            memoire.enregistrer(&etat.auto.fichier);
        }
    }
    Ok(vue)
}

#[cfg_attr(feature = "bureau", tauri::command)]
pub fn supprimer_automatisation(app: AppHandle, id: String, aid: String) -> Result<VueAuto, String> {
    changer(&app, &id, |liste, _| {
        liste.retain(|a| a.id != aid);
        Ok(())
    })
}

/// Active ou met en pause.
#[cfg_attr(feature = "bureau", tauri::command)]
pub fn activer_automatisation(app: AppHandle, id: String, aid: String, on: bool) -> Result<VueAuto, String> {
    changer(&app, &id, |liste, _| {
        liste.iter_mut().find(|a| a.id == aid).ok_or("Cette automatisation n'existe plus.")?.on = on;
        Ok(())
    })
}
