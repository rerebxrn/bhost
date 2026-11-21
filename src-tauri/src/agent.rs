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
