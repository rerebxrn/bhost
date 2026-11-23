//! L'agent de BHOST OS : le cœur de BHOST sans fenêtre.
//! Il reçoit les commandes de l'appli (les mêmes que celles de Tauri) en JSON, une par ligne,
//! et renvoie les réponses et les nouvelles (statut, console, mesures…) de la même façon.
//!
//! Une ligne reçue :   {"n": 1, "cmd": "demarrer_serveur", "args": {"id": "…"}}
//! Réponse :           {"n": 1, "ok": …}   ou   {"n": 1, "err": …}
//! Nouvelle :          {"evt": "statut", "data": {…}}
//!
//! Pour l'instant, les lignes passent par l'entrée et la sortie du programme (essais).
//! Ensuite, elles passeront par le relais (BHOST pilote la machine à distance).

use crate::hote::{AppHandle, Manager};
use crate::Etat;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json as vers_json};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

include!("agent_appels.rs");

pub use crate::hote::Sortie;

/// Démarre le cœur : l'état partagé, puis les fils qui tournent tout le temps.
/// `sortie` reçoit chaque nouvelle (nom, contenu).
pub fn demarrer(dossier: PathBuf, sortie: Sortie) -> Result<AppHandle, String> {
    std::fs::create_dir_all(&dossier).map_err(|e| format!("Impossible de créer {} : {e}", dossier.display()))?;
    let app = AppHandle::nouveau(sortie);
    app.manage(crate::nouvel_etat(dossier));
    crate::lire_les_reglages(&app.state::<Etat>());
    crate::lancer_le_coeur(app.clone());
    Ok(app)
}

/// La machine s'éteint (ou l'agent est arrêté) : chaque serveur enregistre son monde et s'arrête proprement.
pub fn tout_arreter(app: &AppHandle) {
    crate::tout_arreter(app);
}

/// Une ligne reçue (voir en haut du fichier).
#[derive(serde::Deserialize)]
pub struct Demande {
    pub n: u64,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

/// L'id de cette machine : ajouté à chaque serveur envoyé au PC (champ `machine`), pour qu'il les range à part.
static MACHINE: OnceLock<String> = OnceLock::new();

pub fn noter_machine(id: String) {
    let _ = MACHINE.set(id);
}

/// Fait une commande et renvoie la ligne de réponse.
pub async fn repondre(app: &AppHandle, demande: Demande) -> String {
    let resultat = match demande.cmd.as_str() {
        // Commandes de l'agent seulement (pas dans l'appli).
        "etat_machine" => {
            let app = app.clone();
            tokio::task::spawn_blocking(move || etat_machine(&app)).await.map_err(|e| json(e.to_string()))
        }
        "etat_systeme" => Ok(etat_systeme(app)),
        "systeme_machine" => systeme_machine(app, &demande.args),
        "changer_code" => {
            let dossier = app.state::<Etat>().dossier.clone();
            crate::pilotage::changer_code(&dossier).map(Value::String).map_err(|e| json(format!("Impossible de changer le code : {e}")))
        }
        // Des fichiers envoyés par un PC (mondes, plugins, mods, icône) : par morceaux, puis rangés.
        "recevoir_envoi" => {
            let (dossier, args) = (app.state::<Etat>().dossier.clone(), demande.args.clone());
            tokio::task::spawn_blocking(move || recevoir_envoi(&dossier, &args)).await.map_err(|e| json(e.to_string())).and_then(|r| r)
        }
        "importer_envoi" => importer_envoi(app, &demande.args).await,
        cmd => appeler(app, cmd, &demande.args).await,
    };
    let reponse = match resultat {
        Ok(mut v) => {
            if let Some(id) = MACHINE.get() {
                marquer(&mut v, id);
            }
            vers_json!({ "n": demande.n, "ok": v })
        }
        Err(e) => vers_json!({ "n": demande.n, "err": e }),
    };
    reponse.to_string()
}

// ---- Le système de BHOST OS (page « Machine » de BHOST) ----
// L'agent n'a pas les droits de redémarrer la machine ni d'installer des mises à jour (utilisateur « bhost »).
// Il écrit sa demande dans <dossier>/systeme/demande ; le service bhost-systeme (root) la lit et ne connaît
// que quelques mots. Ce service écrit son état dans /run/bhost-systeme/etat.json (que l'agent ne peut pas modifier).

const ETAT_SYSTEME: &str = "/run/bhost-systeme/etat.json";
const VERSION_OS: &str = "/etc/bhost-os-version";

/// Version de BHOST OS ; mises à jour en attente, recherche ou installation en cours, redémarrage nécessaire
/// (nouveau noyau) : `{version, attente, enCours, redemarrage, verifie}` (verifie : secondes depuis 1970).
fn etat_systeme(app: &AppHandle) -> Value {
    let version = std::fs::read_to_string(VERSION_OS).map(|v| v.trim().to_string()).unwrap_or_default();
    let mut etat = std::fs::read_to_string(ETAT_SYSTEME).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or_else(|| vers_json!({}));
    // Un agent plus récent publié : compté avec les mises à jour du système (« Mettre à jour » l'installe aussi).
    let agent = crate::maj_agent::nouvelle(&app.state::<Etat>().reseau);
    if let Value::Object(o) = &mut etat {
        o.insert("version".into(), Value::String(version));
        o.insert("agent".into(), Value::String(crate::maj_agent::version().into()));
        if let Some(a) = &agent {
            o.insert("agentNouveau".into(), Value::String(a.clone()));
            let attente = o.get("attente").and_then(Value::as_u64).unwrap_or(0);
            o.insert("attente".into(), Value::from(attente + 1));
        }
    }
    etat
}

/// Redémarrer, éteindre, rechercher ou installer les mises à jour.
fn systeme_machine(app: &AppHandle, args: &Value) -> Result<Value, Value> {
    let action = args.get("action").and_then(Value::as_str).unwrap_or_default();
    if !["redemarrer", "eteindre", "verifier", "mettre-a-jour"].contains(&action) {
        return Err(json("Action inconnue."));
    }
    if !std::path::Path::new(VERSION_OS).exists() {
        return Err(json("Seulement sur BHOST OS."));
    }
    let etat = app.state::<Etat>();
    let (racine, reseau) = (etat.dossier.clone(), etat.reseau.clone());
    let dossier = racine.join("systeme");
    std::fs::create_dir_all(&dossier).map_err(|e| json(e.to_string()))?;
    let demander = move |action: &str| crate::garde::ecrire_atomique(&dossier.join("demande"), action.as_bytes());
    match action {
        // Rechercher : le système (bhost-systeme) et l'agent (GitHub) en même temps.
        "verifier" => {
            std::thread::spawn(move || {
                let _ = crate::maj_agent::chercher(&reseau);
            });
        }
        // Mettre à jour : d'abord le nouvel agent (téléchargé, signature vérifiée), puis la demande à bhost-systeme,
        // qui installe le système puis l'agent. Le téléchargement se fait en fond : BHOST n'attend pas.
        "mettre-a-jour" => {
            std::thread::spawn(move || {
                if let Err(e) = crate::maj_agent::preparer(&reseau, &racine) {
                    eprintln!("mise à jour de l'agent : {e}");
                }
                if let Err(e) = demander("mettre-a-jour") {
                    eprintln!("mise à jour : {e}");
                }
            });
            return Ok(Value::Null);
        }
        _ => {}
    }
    demander(action).map_err(|e| json(format!("Ça n’a pas marché : {e}")))?;
    Ok(Value::Null)
}

// ---- Fichiers envoyés par un PC ----
// Le PC envoie un fichier (ou une archive .zip de plusieurs) par morceaux de 1 Mo, en base64 :
// ils arrivent dans <dossier>/envois/<envoi>/. Puis « importer_envoi » les range (fichiers, monde, icône)
// avec les mêmes fonctions que sur le PC, et efface l'envoi.

/// Le dossier d'un envoi (l'id est tiré au hasard par le PC : des chiffres hexa seulement).
fn dossier_envoi(dossier: &std::path::Path, envoi: &str) -> Result<PathBuf, Value> {
    if !(8..=32).contains(&envoi.len()) || !envoi.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(json("Envoi refusé."));
    }
    Ok(dossier.join("envois").join(envoi))
}

/// Au démarrage de l'agent : les envois interrompus sont effacés.
pub fn nettoyer_envois(dossier: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dossier.join("envois"));
}

/// Un morceau d'un fichier envoyé : {envoi, nom, position, donnees (base64), fin, zip}.
fn recevoir_envoi(dossier: &std::path::Path, args: &Value) -> Result<Value, Value> {
    use base64::Engine;
    use std::io::Write;
    let envoi: String = arg(args, "envoi")?;
    let nom: String = arg(args, "nom")?;
    let position: u64 = arg(args, "position")?;
    let donnees: String = arg(args, "donnees")?;
    let fin: bool = arg(args, "fin")?;
    let zip: bool = arg::<Option<bool>>(args, "zip")?.unwrap_or(false);
    // Un nom de fichier seul (pas de dossier, rien qui remonte).
    if nom.is_empty() || nom.len() > 200 || nom.contains(['/', '\\', '\0']) || nom == "." || nom == ".." {
        return Err(json("Nom de fichier refusé."));
    }
    let ici = dossier_envoi(dossier, &envoi)?;
    let fichier = ici.join(&nom);
    let octets = base64::engine::general_purpose::STANDARD.decode(donnees.as_bytes()).map_err(|_| json("Morceau abîmé."))?;
    // Un envoi ne remplit jamais le disque de la machine (les serveurs en ont besoin).
    const ENVOI_MAX: u64 = 32 * 1024 * 1024 * 1024;
    if position + octets.len() as u64 > ENVOI_MAX {
        return Err(json("Envoi trop gros pour la machine."));
    }
    if crate::garde::place_libre(dossier).is_some_and(|libre| libre < octets.len() as u64 + 1024 * 1024 * 1024) {
        return Err(json("Plus assez de place sur la machine."));
    }
    let erreur = |e: std::io::Error| json(format!("Enregistrement impossible sur la machine ({e})."));
    if position == 0 {
        std::fs::create_dir_all(&ici).map_err(erreur)?;
        std::fs::write(&fichier, &octets).map_err(erreur)?;
    } else {
        // Le morceau suit exactement le précédent (sinon : un morceau perdu, on recommence).
        let taille = std::fs::metadata(&fichier).map(|m| m.len()).map_err(|_| json("Envoi interrompu. Réessaie."))?;
        if taille != position {
            return Err(json("Envoi interrompu. Réessaie."));
        }
        let mut f = std::fs::OpenOptions::new().append(true).open(&fichier).map_err(erreur)?;
        f.write_all(&octets).map_err(erreur)?;
    }
    if fin && zip {
        let resultat = crate::java::extraire(&fichier, &ici).map_err(json);
        let _ = std::fs::remove_file(&fichier);
        resultat?;
    }
    Ok(Value::Null)
}

/// Range un envoi : {id, envoi, quoi: "fichiers" | "monde" | "icone", dossier, actuel}.
async fn importer_envoi(app: &AppHandle, args: &Value) -> Result<Value, Value> {
    let id: String = arg(args, "id")?;
    let envoi: String = arg(args, "envoi")?;
    let quoi: String = arg(args, "quoi")?;
    let ici = dossier_envoi(&app.state::<Etat>().dossier, &envoi)?;
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&ici).map_err(|_| json("Envoi introuvable."))?.flatten().map(|e| e.path()).collect();
    sources.sort();
    let texte = |p: &PathBuf| p.to_string_lossy().into_owned();
    // Le stockage max du serveur : la place manque-t-elle ?
    let (app2, id2, ici2) = (app.clone(), id.clone(), ici.clone());
    let place = tokio::task::spawn_blocking(move || {
        let etat = app2.state::<Etat>();
        let serveur = etat.liste.lock().unwrap().iter().find(|s| s.id == id2).cloned();
        serveur.map_or(Ok(()), |s| crate::stockage::verifier_ajout(&etat, &s, crate::stockage::taille(&ici2)))
    })
    .await
    .unwrap_or(Ok(()));
    if let Err(e) = place {
        let _ = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(ici)).await;
        return Err(json(e));
    }
    let resultat = match quoi.as_str() {
        "fichiers" => {
            let dossier: String = arg::<Option<String>>(args, "dossier")?.unwrap_or_default();
            crate::importer_fichiers(app.clone(), id, dossier, sources.iter().map(texte).collect()).await.map(json).map_err(json)
        }
        "monde" => {
            let actuel = arg::<Option<bool>>(args, "actuel")?.unwrap_or(false);
            match sources.first() {
                Some(source) => crate::importer_monde(app.clone(), id, texte(source), actuel).await.map(json).map_err(json),
                None => Err(json("Pas de monde dans cet envoi.")),
            }
        }
        "icone" => match sources.first() {
            Some(source) => crate::icone_serveur(app.clone(), id, texte(source)).await.map(json).map_err(json),
            None => Err(json("Image introuvable.")),
        },
        _ => Err(json("Envoi refusé.")),
    };
    let _ = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(ici)).await;
    resultat
}

/// Un serveur (ou une liste de serveurs) dans la réponse : on y ajoute `machine`.
fn marquer(v: &mut Value, id: &str) {
    match v {
        Value::Array(liste) => liste.iter_mut().for_each(|x| marquer(x, id)),
        Value::Object(o) if o.contains_key("jar") && o.contains_key("dossier") => {
            o.insert("machine".into(), Value::String(id.to_string()));
        }
        _ => {}
    }
}

/// L'état de la machine pour la barre de gauche et la page « Machine » de BHOST (type `Machine` de src/types.ts).
fn etat_machine(app: &AppHandle) -> Value {
    use sysinfo::{Components, Disks, System};
    static SYSTEME: Mutex<Option<System>> = Mutex::new(None);
    let go = |octets: u64| (octets as f64 / 1_073_741_824.0 * 10.0).round() / 10.0;
    let etat = app.state::<Etat>();

    // Le disque où sont les serveurs (le point de montage le plus long qui contient le dossier).
    let dossier = crate::dossier_serveurs(&etat);
    let disque = Disks::new_with_refreshed_list().ok().and_then(|d| {
        d.iter()
            .filter(|x| dossier.starts_with(x.mount_point()))
            .max_by_key(|x| x.mount_point().as_os_str().len())
            .map(|x| vers_json!({ "utiliseGo": go(x.total_space() - x.available_space()), "totalGo": go(x.total_space()) }))
    });

    // Mémoire réservée : celle des serveurs allumés (une machine dédiée ne bloque rien pour un serveur éteint).
    let allumes: Vec<String> = etat.vivants.lock().unwrap().keys().cloned().collect();
    let reservee: u64 = etat.liste.lock().unwrap().iter().filter(|s| allumes.contains(&s.id)).map(|s| u64::from(s.ram_mo)).sum();
    let mut systeme = SYSTEME.lock().unwrap();
    if systeme.is_none() {
        *systeme = System::new().ok();
    }
    // Le processeur se mesure entre deux relevés : trop rapprochés (BHOST demande l'état souvent), ils donnent 0 %.
    // On garde donc la dernière mesure tant qu'une seconde n'est pas passée.
    static PROCESSEUR: Mutex<Option<(std::time::Instant, f64)>> = Mutex::new(None);
    let (total, processeur) = match systeme.as_mut() {
        Some(s) => {
            s.refresh_memory();
            let mut dernier = PROCESSEUR.lock().unwrap();
            let valeur = match *dernier {
                Some((quand, v)) if quand.elapsed() < std::time::Duration::from_secs(1) => v,
                _ => {
                    s.refresh_cpu_usage();
                    let v = f64::from(s.global_cpu_usage()).round();
                    *dernier = Some((std::time::Instant::now(), v));
                    v
                }
            };
            (s.total_memory(), Some(valeur))
        }
        None => (0, None),
    };

    // Température : le capteur du processeur s'il y en a un (sinon : null, BHOST affiche « – »).
    let temperature = Components::new_with_refreshed_list().ok().and_then(|c| {
        let tous: Vec<_> = c.iter().filter_map(|x| x.temperature().map(|t| (x.label().to_lowercase(), t))).collect();
        let processeur = tous.iter().filter(|(l, _)| ["package", "tctl", "k10temp", "coretemp", "cpu"].iter().any(|m| l.contains(m))).map(|x| x.1);
        processeur.reduce(f32::max).or_else(|| tous.iter().map(|x| x.1).reduce(f32::max)).map(|t| f64::from(t).round())
    });

    // L'adresse de la machine dans la maison (aucun paquet n'est envoyé : on demande seulement la route).
    let ip = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|u| u.connect("1.1.1.1:80").map(|_| u))
        .and_then(|u| u.local_addr())
        .ok()
        .map(|a| a.ip().to_string());

    vers_json!({
        "disque": disque,
        "memoire": { "reserveeGo": go(reservee * 1_048_576), "totalGo": go(total) },
        "processeur": processeur,
        "temperature": temperature,
        "ip": ip,
        // BHOST OS : les serveurs ont leurs limites (mémoire max, part du processeur).
        "limites": crate::processus::cgroupes::actives(),
        "serveurs": etat.liste.lock().unwrap().len(),
        "version": env!("CARGO_PKG_VERSION"),
    })
}

/// La ligne d'une nouvelle.
pub fn ligne_nouvelle(evenement: &str, contenu: Value) -> String {
    vers_json!({ "evt": evenement, "data": contenu }).to_string()
}

/// Un argument de la commande (`null` s'il manque : les `Option` deviennent `None`).
fn arg<T: DeserializeOwned>(args: &Value, nom: &str) -> Result<T, Value> {
    serde_json::from_value(args.get(nom).cloned().unwrap_or(Value::Null)).map_err(|e| json(format!("Argument « {nom} » : {e}")))
}

fn json<T: Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_des_commandes_a_jour() {
        // Chaque commande enregistrée dans Tauri est soit faite par l'agent, soit réservée à l'appli.
        // Si ce test échoue : python outils/agent_appels.py
        let lib = include_str!("lib.rs");
        let debut = lib.find("generate_handler![").unwrap();
        let fin = debut + lib[debut..].find(']').unwrap();
        let script = include_str!("../../outils/agent_appels.py");
        for chemin in lib[debut + "generate_handler![".len()..fin].split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let nom = chemin.rsplit("::").next().unwrap();
            let agent = COMMANDES.contains(&nom);
            let bureau = script.contains(&format!("\"{nom}\""));
            assert!(agent != bureau, "{nom} : relance python outils/agent_appels.py");
        }
    }

    #[test]
    fn serveurs_marques_avec_la_machine() {
        let mut v = vers_json!([{ "id": "a", "jar": "server.jar", "dossier": "/srv/a" }, { "autre": 1 }]);
        marquer(&mut v, "7e57");
        assert_eq!(v[0]["machine"], "7e57");
        assert!(v[1].get("machine").is_none());
    }

    #[test]
    fn arguments() {
        let args = vers_json!({ "id": "abc", "actif": true });
        assert_eq!(arg::<String>(&args, "id").unwrap(), "abc");
        assert_eq!(arg::<Option<u32>>(&args, "absent").unwrap(), None);
        assert!(arg::<u32>(&args, "id").is_err());
    }
}
