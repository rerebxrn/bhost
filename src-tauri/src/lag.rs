//! « Pourquoi ça lag ? » (Accueil) : BHOST lance le profileur spark pendant 30 secondes, puis ouvre son rapport.
//!
//! - Paper et Purpur l'ont déjà (depuis la 1.21) : rien à installer.
//! - Les autres (Spigot, anciens Paper, Fabric, Forge, NeoForge) : BHOST installe spark depuis le catalogue ;
//!   il marche au prochain démarrage du serveur.
//! - Vanilla : pas de plugins ni de mods, pas de spark.
//!
//! La commande : `spark profiler start --timeout 30`. À la fin, spark envoie le rapport en ligne et écrit
//! son adresse (https://spark.lucko.me/…) dans la console : BHOST la repère et l'envoie à l'interface (« lag »).

use crate::hote::{AppHandle, Emitter, Manager};
use crate::Etat;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// La durée de l'analyse (secondes).
pub const DUREE: u64 = 30;
/// Le projet spark dans le catalogue (Modrinth).
const PROJET_SPARK: &str = "l6YH9Als";
/// Au-delà, spark n'a pas répondu.
const ATTENTE_MAX: Duration = Duration::from_secs(DUREE + 60);
const ADRESSE: &str = "https://spark.lucko.me/";

/// Les analyses en cours : serveur → début.
static EN_COURS: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

fn en_cours<T>(f: impl FnOnce(&mut HashMap<String, Instant>) -> T) -> T {
    f(EN_COURS.lock().unwrap().get_or_insert_with(HashMap::new))
}

/// Où en est spark pour ce logiciel.
#[derive(Debug, PartialEq)]
pub enum Spark {
    /// Déjà dans le serveur (Paper, Purpur depuis la 1.21).
    Inclus,
    /// À installer comme plugin ou mod.
    AInstaller,
    /// Pas possible (Vanilla).
    Impossible,
}

/// « 1.21.4 » → (1, 21) ; « 26.1 » → (26, 1).
fn majeure(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0)))
}

pub fn spark(logiciel: &str, version: &str) -> Spark {
    match logiciel {
        "Vanilla" => Spark::Impossible,
        "Paper" | "Purpur" if majeure(version).is_some_and(|(a, b)| a > 1 || b >= 21) => Spark::Inclus,
        _ => Spark::AInstaller,
    }
}

/// L'adresse du rapport dans une ligne de console (« [⚡] https://spark.lucko.me/AbCd123 »).
pub fn adresse_rapport(ligne: &str) -> Option<String> {
    let debut = ligne.find(ADRESSE)?;
    let adresse: String = ligne[debut..].chars().take_while(|c| c.is_ascii_alphanumeric() || "/:.-_?=#".contains(*c)).collect();
    (adresse.len() > ADRESSE.len()).then_some(adresse)
}

/// Ce que l'interface reçoit à la fin (« lag ») : l'adresse du rapport, ou la phrase d'erreur.
#[derive(Serialize, Clone)]
struct Fin<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    lien: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    erreur: Option<&'a str>,
}

fn finir(app: &AppHandle, id: &str, lien: Option<String>, erreur: Option<&str>) {
    if en_cours(|e| e.remove(id)).is_some() {
        let _ = app.emit("lag", Fin { id, lien, erreur });
    }
}

/// Chaque ligne de la console d'un serveur passe ici.
pub fn suivre(app: &AppHandle, id: &str, ligne: &str) {
    if !en_cours(|e| e.contains_key(id)) {
        return;
    }
    let bas = ligne.to_lowercase();
    if let Some(lien) = adresse_rapport(ligne) {
        finir(app, id, Some(lien), None);
    } else if bas.contains("profiler") && bas.contains("already") {
        // Un profilage tournait déjà : on l'arrête, son rapport fait l'affaire.
        if let Some(p) = app.state::<Etat>().vivants.lock().unwrap().get(id) {
            let _ = p.ecrire("spark profiler stop");
        }
    } else if bas.contains(".sparkprofile") {
        finir(app, id, None, Some("Le rapport n'a pas pu partir sur Internet. Il est dans le dossier du serveur."));
    }
}

/// Le serveur s'est arrêté : l'analyse aussi.
pub fn oublier(app: &AppHandle, id: &str) {
    finir(app, id, None, Some("Le serveur s'est arrêté."));
}

/// Ce que l'interface reçoit tout de suite.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lancement {
    /// L'analyse dure ce temps (secondes) ; 0 : spark vient d'être installé, il faut redémarrer.
    secondes: u64,
}

/// « Pourquoi ça lag ? » : lance spark (ou l'installe d'abord). Le rapport arrive par la nouvelle « lag ».
#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn pourquoi_lag(app: AppHandle, id: String) -> Result<Lancement, String> {
    let serveur = crate::serveur_de(&app.state::<Etat>(), &id)?;
    if app.state::<Etat>().statuts.lock().unwrap().get(&id).map(String::as_str) != Some("en-ligne") {
        return Err("Allume le serveur d'abord.".into());
    }
    match spark(&serveur.logiciel, &serveur.version) {
        Spark::Impossible => return Err("Pas possible avec Vanilla : il faut Paper ou un serveur à mods.".into()),
        Spark::AInstaller if !deja_installe(std::path::Path::new(&serveur.dossier)) => {
            crate::catalogue::installer_extension(app.clone(), id.clone(), PROJET_SPARK.into(), None).await?;
            return Ok(Lancement { secondes: 0 });
        }
        _ => {}
    }
    if en_cours(|e| e.get(&id).is_some_and(|t| t.elapsed() < ATTENTE_MAX)) {
        return Err("Analyse déjà en cours.".into());
    }
    en_cours(|e| e.insert(id.clone(), Instant::now()));
    let commande = format!("spark profiler start --timeout {DUREE}");
    if let Err(e) = crate::envoyer_commande(app.clone(), id.clone(), commande, app.state::<Etat>()) {
        en_cours(|x| x.remove(&id));
        return Err(e);
    }
    // spark ne répond pas (pas de rapport) : l'interface n'attend pas pour rien.
    let (app2, id2) = (app.clone(), id.clone());
    std::thread::spawn(move || {
        std::thread::sleep(ATTENTE_MAX);
        if en_cours(|e| e.get(&id2).is_some_and(|t| t.elapsed() >= ATTENTE_MAX)) {
            finir(&app2, &id2, None, Some("spark n'a pas répondu. Réessaie."));
        }
    });
    Ok(Lancement { secondes: DUREE })
}

/// spark est-il déjà dans plugins/ ou mods/ ?
fn deja_installe(dossier: &std::path::Path) -> bool {
    ["plugins", "mods"].iter().any(|d| {
        std::fs::read_dir(dossier.join(d)).is_ok_and(|l| {
            l.flatten().any(|e| {
                let nom = e.file_name().to_string_lossy().to_lowercase();
                nom.starts_with("spark") && nom.ends_with(".jar")
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spark_selon_le_logiciel() {
        assert_eq!(spark("Paper", "1.21.4"), Spark::Inclus);
        assert_eq!(spark("Purpur", "1.21"), Spark::Inclus);
        assert_eq!(spark("Paper", "26.1"), Spark::Inclus);
        assert_eq!(spark("Paper", "1.20.6"), Spark::AInstaller);
        assert_eq!(spark("Spigot", "1.21.4"), Spark::AInstaller);
        assert_eq!(spark("Fabric", "1.21.4"), Spark::AInstaller);
        assert_eq!(spark("Vanilla", "1.21.4"), Spark::Impossible);
    }

    #[test]
    fn adresse_dans_la_console() {
        assert_eq!(adresse_rapport("[12:00:01 INFO]: [⚡] https://spark.lucko.me/AbCd12345").as_deref(), Some("https://spark.lucko.me/AbCd12345"));
        assert_eq!(adresse_rapport("[⚡] https://spark.lucko.me/xYz9 (copie)").as_deref(), Some("https://spark.lucko.me/xYz9"));
        assert_eq!(adresse_rapport("[⚡] Profiler stopped"), None);
        assert_eq!(adresse_rapport("https://spark.lucko.me/"), None);
    }

    #[test]
    fn spark_deja_la() {
        let d = std::env::temp_dir().join(format!("bhost-lag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("plugins")).unwrap();
        assert!(!deja_installe(&d));
        std::fs::write(d.join("plugins/spark-1.10.142-bukkit.jar"), b"").unwrap();
        assert!(deja_installe(&d));
        let _ = std::fs::remove_dir_all(&d);
    }
}
