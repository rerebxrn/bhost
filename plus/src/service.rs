// Ce que répond POST /etat.

use crate::polar::{Reponse, Source};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Une réponse de Polar est gardée une minute (l'appli redemande toutes les 6 heures, et à chaque ouverture).
const CACHE: Duration = Duration::from_secs(60);
/// Demandes permises par adresse IP en 10 minutes.
const DEMANDES_MAX: u32 = 60;
const FENETRE: Duration = Duration::from_secs(600);
/// Le jeton du relais prioritaire vaut 2 jours (l'appli le renouvelle toutes les 6 heures).
const DUREE_JETON: u64 = 2 * 24 * 3600;

#[derive(Deserialize)]
struct Demande {
    cle: String,
    #[serde(default)]
    activation: Option<String>,
    /// La clé publique de ce BHOST (hexadécimal, 32 octets), celle qu'il montre au relais.
    #[serde(default)]
    agent: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pc {
    pub id: String,
    pub nom: String,
    /// Activé depuis (millisecondes).
    pub depuis: Option<i64>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Etat {
    /// « actif », « retard » (paiement refusé, Polar réessaie), « retire » (ce PC a été libéré),
    /// « inconnue » (clé retirée, finie ou fausse).
    pub etat: &'static str,
    /// « mois » ou « an ».
    pub formule: Option<&'static str>,
    /// Prochain renouvellement, ou fin si l'abonnement est arrêté (millisecondes).
    pub fin: Option<i64>,
    /// Arrêté : il finit à `fin`.
    pub arrete: bool,
    pub places: Option<u64>,
    pub pcs: Vec<Pc>,
    /// Le jeton du relais prioritaire : « expire signature » (le relais vérifie avec la clé publique du service).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jeton: Option<String>,
}

/// (clé, activation) → la réponse de Polar et quand on l'a eue.
type Cache = HashMap<(String, Option<String>), (Instant, Etat)>;

pub struct Service {
    source: Box<dyn Source>,
    cle: SigningKey,
    cache: Mutex<Cache>,
    demandes: Mutex<HashMap<String, (Instant, u32)>>,
}

fn maintenant() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn date(v: &Value) -> Option<i64> {
    v.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|d| d.timestamp_millis())
}

fn hexa(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

/// Le message que signe le service, et que le relais vérifie (relais/src/protocole.rs › a_signer_priorite).
pub fn a_signer(agent: &str, expire: u64) -> String {
    format!("BHOST+ PRIORITE {agent} {expire}")
}

/// La clé de signature du jeton : créée au premier lancement, avec sa clé publique à côté (fichier .pub) pour le relais.
pub fn charger_cle(fichier: &Path) -> std::io::Result<SigningKey> {
    let cle = match std::fs::read(fichier) {
        Ok(o) if o.len() == 32 => SigningKey::from_bytes(&o.try_into().expect("32 octets")),
        Ok(_) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "clé abîmée")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut graine = [0u8; 32];
            getrandom::fill(&mut graine).map_err(|_| std::io::Error::other("pas de hasard"))?;
            std::fs::write(fichier, graine)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(fichier, std::fs::Permissions::from_mode(0o600))?;
            }
            SigningKey::from_bytes(&graine)
        }
        Err(e) => return Err(e),
    };
    let mut publique = fichier.as_os_str().to_owned();
    publique.push(".pub");
    std::fs::write(publique, hexa(cle.verifying_key().as_bytes()) + "\n")?;
    Ok(cle)
}

fn json(e: &Etat) -> String {
    serde_json::to_string(e).unwrap_or_default()
}

impl Service {
    pub fn nouveau(source: impl Source + 'static, cle: SigningKey) -> Service {
        Service { source: Box::new(source), cle, cache: Mutex::default(), demandes: Mutex::default() }
    }

    fn trop(&self, ip: &str) -> bool {
        let mut t = self.demandes.lock().unwrap();
        let maintenant = Instant::now();
        t.retain(|_, (depuis, _)| maintenant.duration_since(*depuis) < FENETRE);
        let (_, n) = t.entry(ip.to_string()).or_insert((maintenant, 0));
        *n += 1;
        *n > DEMANDES_MAX
    }

    /// POST /etat : (code HTTP, JSON).
    pub fn etat(&self, ip: &str, corps: &str) -> (u16, String) {
        if self.trop(ip) {
            return (429, r#"{"erreur":"trop"}"#.into());
        }
        let Ok(d) = serde_json::from_str::<Demande>(corps) else { return (400, r#"{"erreur":"demande"}"#.into()) };
        let cle = d.cle.trim().to_string();
        let activation = d.activation.filter(|a| crate::polar::id_valide(a));
        if cle.is_empty() || cle.len() > 200 {
            return (400, r#"{"erreur":"demande"}"#.into());
        }
        let agent = d.agent.filter(|a| a.len() == 64 && a.bytes().all(|c| c.is_ascii_hexdigit())).map(|a| a.to_ascii_lowercase());
        let cle_cache = (cle.clone(), activation.clone());
        let garde = self.cache.lock().unwrap().get(&cle_cache).filter(|(t, _)| t.elapsed() < CACHE).map(|(_, e)| e.clone());
        let mut etat = match garde {
            Some(e) => e,
            None => match self.demander(&cle, activation.as_deref()) {
                Some(e) => {
                    let mut cache = self.cache.lock().unwrap();
                    cache.retain(|_, (t, _)| t.elapsed() < CACHE);
                    cache.insert(cle_cache, (Instant::now(), e.clone()));
                    e
                }
                None => return (503, r#"{"erreur":"polar"}"#.into()),
            },
        };
        // Le jeton seulement pour un abonnement en cours, et pour le PC qui demande.
        if let (Some(agent), "actif" | "retard") = (agent, etat.etat) {
            let expire = maintenant() + DUREE_JETON;
            let signature = self.cle.sign(a_signer(&agent, expire).as_bytes());
            etat.jeton = Some(format!("{expire} {}", hexa(&signature.to_bytes())));
        }
        (200, json(&etat))
    }

    /// Demande tout à Polar. None : Polar ne répond pas.
    fn demander(&self, cle: &str, activation: Option<&str>) -> Option<Etat> {
        let vide = |etat| Etat { etat, formule: None, fin: None, arrete: false, places: None, pcs: Vec::new(), jeton: None };
        let mut retire = false;
        let cle_polar = match self.source.valider(cle, activation) {
            Reponse::Ok(v) => v,
            Reponse::Panne => return None,
            // Ce PC a été libéré (depuis un autre PC ou l'espace client) : la clé, elle, marche encore ?
            Reponse::Absente if activation.is_some() => match self.source.valider(cle, None) {
                Reponse::Ok(v) => {
                    retire = true;
                    v
                }
                Reponse::Absente => return Some(vide("inconnue")),
                Reponse::Panne => return None,
            },
            Reponse::Absente => return Some(vide("inconnue")),
        };
        let id = cle_polar["id"].as_str().unwrap_or_default().to_string();
        let client = cle_polar["customer_id"].as_str().unwrap_or_default().to_string();
        let mut etat = vide(if retire { "retire" } else { "actif" });
        etat.places = cle_polar["limit_activations"].as_u64();
        etat.fin = date(&cle_polar["expires_at"]);
        // Les PC (activations), le plus ancien d'abord.
        if let Reponse::Ok(v) = self.source.activations(&id) {
            let mut pcs: Vec<Pc> = v["activations"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| Pc {
                    id: a["id"].as_str().unwrap_or_default().to_string(),
                    nom: a["label"].as_str().unwrap_or("PC").chars().take(60).collect(),
                    depuis: date(&a["created_at"]),
                })
                .collect();
            pcs.sort_by_key(|p| p.depuis);
            etat.pcs = pcs;
        }
        // L'abonnement : en cours d'abord, sinon le plus récent.
        if let Reponse::Ok(v) = self.source.abonnements(&client) {
            let liste = v["items"].as_array().cloned().unwrap_or_default();
            let en_cours = |s: &Value| matches!(s["status"].as_str(), Some("active" | "trialing" | "past_due" | "unpaid"));
            if let Some(a) = liste.iter().find(|s| en_cours(s)).or(liste.first()) {
                etat.formule = match a["recurring_interval"].as_str() {
                    Some("year") => Some("an"),
                    Some("month") => Some("mois"),
                    _ => None,
                };
                etat.arrete = a["cancel_at_period_end"].as_bool().unwrap_or(false) || a["status"].as_str() == Some("canceled");
                etat.fin = date(&a["ends_at"]).or(date(&a["current_period_end"])).or(etat.fin);
                if !retire && matches!(a["status"].as_str(), Some("past_due" | "unpaid")) {
                    etat.etat = "retard";
                }
            }
        }
        Some(etat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Faux;
    const ID: &str = "11111111-1111-1111-1111-111111111111";
    const CLIENT: &str = "22222222-2222-2222-2222-222222222222";
    const ICI: &str = "33333333-3333-3333-3333-333333333333";
    const AUTRE: &str = "44444444-4444-4444-4444-444444444444";

    impl Source for Faux {
        fn valider(&self, cle: &str, activation: Option<&str>) -> Reponse {
            match (cle, activation) {
                ("BHOST-PANNE", _) => Reponse::Panne,
                ("BHOST-BONNE" | "BHOST-RETARD", None | Some(ICI)) => {
                    Reponse::Ok(json!({ "id": ID, "customer_id": if cle == "BHOST-RETARD" { "retard" } else { CLIENT }, "limit_activations": 3 }))
                }
                ("BHOST-BONNE", Some(_)) => Reponse::Absente,
                _ => Reponse::Absente,
            }
        }
        fn activations(&self, _: &str) -> Reponse {
            Reponse::Ok(json!({ "activations": [
                { "id": AUTRE, "label": "PORTABLE-LEA", "created_at": "2026-11-20T10:00:00Z" },
                { "id": ICI, "label": "PC-THOMAS", "created_at": "2026-11-01T10:00:00Z" },
            ]}))
        }
        fn abonnements(&self, client: &str) -> Reponse {
            let statut = if client == "retard" { "past_due" } else { "active" };
            Reponse::Ok(json!({ "items": [
                { "status": "canceled", "recurring_interval": "month", "current_period_end": "2025-01-01T00:00:00Z" },
                { "status": statut, "recurring_interval": "year", "cancel_at_period_end": false, "current_period_end": "2027-11-23T00:00:00Z" },
            ]}))
        }
    }

    fn service() -> Service {
        Service::nouveau(Faux, SigningKey::from_bytes(&[7; 32]))
    }

    fn lire(r: (u16, String)) -> (u16, Value) {
        (r.0, serde_json::from_str(&r.1).unwrap())
    }

    #[test]
    fn abonne_avec_ses_pc() {
        let (code, v) = lire(service().etat("1.2.3.4", &format!(r#"{{"cle":"BHOST-BONNE","activation":"{ICI}"}}"#)));
        assert_eq!(code, 200);
        assert_eq!(v["etat"], "actif");
        assert_eq!(v["formule"], "an");
        assert_eq!(v["places"], 3);
        assert_eq!(v["pcs"][0]["nom"], "PC-THOMAS");
        assert_eq!(v["pcs"].as_array().unwrap().len(), 2);
        assert!(v.get("jeton").is_none(), "pas de jeton sans clé de PC");
    }

    #[test]
    fn retard_et_pc_retire() {
        let (_, v) = lire(service().etat("1.2.3.4", r#"{"cle":"BHOST-RETARD"}"#));
        assert_eq!(v["etat"], "retard");
        let (_, v) = lire(service().etat("1.2.3.4", &format!(r#"{{"cle":"BHOST-BONNE","activation":"{AUTRE}"}}"#)));
        assert_eq!(v["etat"], "retire");
        let (_, v) = lire(service().etat("1.2.3.4", r#"{"cle":"BHOST-FAUSSE"}"#));
        assert_eq!(v["etat"], "inconnue");
        assert_eq!(service().etat("1.2.3.4", r#"{"cle":"BHOST-PANNE"}"#).0, 503);
    }

    #[test]
    fn jeton_verifiable() {
        let agent = "ab".repeat(32);
        let (_, v) = lire(service().etat("1.2.3.4", &format!(r#"{{"cle":"BHOST-BONNE","agent":"{agent}"}}"#)));
        let jeton = v["jeton"].as_str().unwrap();
        let (expire, signature) = jeton.split_once(' ').unwrap();
        let octets: Vec<u8> = (0..64).map(|i| u8::from_str_radix(&signature[i * 2..i * 2 + 2], 16).unwrap()).collect();
        let signature = ed25519_dalek::Signature::from_bytes(&octets.try_into().unwrap());
        let publique = SigningKey::from_bytes(&[7; 32]).verifying_key();
        assert!(publique.verify_strict(a_signer(&agent, expire.parse().unwrap()).as_bytes(), &signature).is_ok());
    }

    #[test]
    fn limite_par_adresse() {
        let s = service();
        for _ in 0..DEMANDES_MAX {
            assert_ne!(s.etat("9.9.9.9", r#"{"cle":"BHOST-FAUSSE"}"#).0, 429);
        }
        assert_eq!(s.etat("9.9.9.9", r#"{"cle":"BHOST-FAUSSE"}"#).0, 429);
        assert_ne!(s.etat("8.8.8.8", r#"{"cle":"BHOST-FAUSSE"}"#).0, 429);
    }
}
