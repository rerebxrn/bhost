// Les demandes à Polar (https://polar.sh/docs/api-reference).

use serde_json::{Value, json};
use std::time::Duration;

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .user_agent("bhost-plus/0.1 (+https://bhost.app)")
        .build()
        .new_agent()
}

/// Ce que Polar répond.
#[derive(Debug, PartialEq)]
pub enum Reponse {
    Ok(Value),
    /// Clé inconnue, retirée, désactivée ou finie (ou activation qui ne correspond pas).
    Absente,
    /// Polar ne répond pas, ou refuse notre clé d'accès.
    Panne,
}

/// Ce que le service demande à Polar : remplacé par un faux Polar dans les tests.
pub trait Source: Send + Sync {
    /// La clé (et l'activation de ce PC) est-elle valable ? Adresse publique.
    fn valider(&self, cle: &str, activation: Option<&str>) -> Reponse;
    /// La clé avec ses activations (clé d'accès de l'organisation).
    fn activations(&self, id: &str) -> Reponse;
    /// Les abonnements du client, le plus récent d'abord (clé d'accès de l'organisation).
    fn abonnements(&self, client: &str) -> Reponse;
}

pub struct Polar {
    pub api: String,
    pub jeton: String,
    pub organisation: String,
    pub agent: ureq::Agent,
}

fn lire(resultat: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Reponse {
    let Ok(mut r) = resultat else { return Reponse::Panne };
    let code = r.status().as_u16();
    let texte = r.body_mut().with_config().limit(1024 * 1024).read_to_string().unwrap_or_default();
    match code {
        200..=299 => serde_json::from_str(&texte).map(Reponse::Ok).unwrap_or(Reponse::Panne),
        403 | 404 | 422 => Reponse::Absente,
        _ => Reponse::Panne,
    }
}

impl Source for Polar {
    fn valider(&self, cle: &str, activation: Option<&str>) -> Reponse {
        if self.organisation.is_empty() {
            return Reponse::Panne;
        }
        let mut corps = json!({ "key": cle, "organization_id": self.organisation });
        if let Some(a) = activation {
            corps["activation_id"] = json!(a);
        }
        let adresse = format!("{}/v1/customer-portal/license-keys/validate", self.api);
        lire(self.agent.post(&adresse).content_type("application/json").send(corps.to_string()))
    }

    fn activations(&self, id: &str) -> Reponse {
        if self.jeton.is_empty() || !id_valide(id) {
            return Reponse::Panne;
        }
        let adresse = format!("{}/v1/license-keys/{id}", self.api);
        lire(self.agent.get(&adresse).header("Authorization", &format!("Bearer {}", self.jeton)).call())
    }

    fn abonnements(&self, client: &str) -> Reponse {
        if self.jeton.is_empty() || !id_valide(client) {
            return Reponse::Panne;
        }
        let adresse = format!("{}/v1/subscriptions/?customer_id={client}&limit=20&sorting=-started_at", self.api);
        lire(self.agent.get(&adresse).header("Authorization", &format!("Bearer {}", self.jeton)).call())
    }
}

/// Un identifiant Polar (UUID) : on ne met rien d'autre dans une adresse.
pub fn id_valide(id: &str) -> bool {
    id.len() == 36 && id.bytes().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == b'-' } else { c.is_ascii_hexdigit() })
}
