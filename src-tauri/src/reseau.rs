// Aller chercher quelque chose sur Internet (en HTTPS).
// BHOST passe par la sécurité de Windows (ses certificats), sans rien installer en plus.

use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Comment BHOST se présente aux sites qu'il contacte.
pub const AGENT: &str = "BHOST/0.1 (+https://catgraphique.fr)";

/// Taille maximale d'une réponse : 4 Mo. Les réponses de Mojang font quelques Ko.
const TAILLE_MAX: u64 = 4 * 1024 * 1024;

/// Le « navigateur » de BHOST : on en crée un seul, il garde ses connexions ouvertes.
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        // HTTPS seulement : une adresse ou une redirection en http:// est refusée.
        .https_only(true)
        .tls_config(TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build())
        // Jamais plus de 8 secondes par demande : sans Internet, BHOST n'attend pas dans le vide.
        .timeout_global(Some(Duration::from_secs(8)))
        // PaperMC exige un nom de logiciel avec un moyen de contact (provisoire : le site de Thomas).
        .user_agent(AGENT)
        .build()
        .new_agent()
}

/// Télécharge une adresse. Renvoie `None` si elle n'existe pas (réponses 204 et 404).
pub fn telecharger(agent: &ureq::Agent, adresse: &str) -> Result<Option<Vec<u8>>, String> {
    match agent.get(adresse).call() {
        Ok(mut reponse) => {
            if reponse.status().as_u16() == 204 {
                return Ok(None);
            }
            reponse.body_mut().with_config().limit(TAILLE_MAX).read_to_vec().map(Some).map_err(|e| e.to_string())
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(erreur) => Err(erreur.to_string()),
    }
}

/// Ouvre un gros téléchargement (Java, serveurs…) pour le lire petit à petit.
/// Pas de limite de temps totale ; on abandonne seulement si rien n'arrive pendant 30 secondes.
pub fn ouvrir(agent: &ureq::Agent, adresse: &str) -> Result<Box<dyn std::io::Read>, String> {
    let reponse = agent
        .get(adresse)
        .config()
        .timeout_global(None)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    Ok(Box::new(reponse.into_body().into_reader()))
}

/// Envoie du JSON (POST) et renvoie la réponse. Sert au catalogue des mods et plugins.
pub fn envoyer_json(agent: &ureq::Agent, adresse: &str, corps: &serde_json::Value) -> Result<Vec<u8>, String> {
    let texte = serde_json::to_string(corps).map_err(|e| e.to_string())?;
    let mut reponse = agent.post(adresse).content_type("application/json").send(texte).map_err(|e| e.to_string())?;
    reponse.body_mut().with_config().limit(TAILLE_MAX).read_to_vec().map_err(|e| e.to_string())
}

/// Télécharge un fichier entier (un mod, un plugin…), sans dépasser `max` octets.
pub fn telecharger_gros(agent: &ureq::Agent, adresse: &str, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut lecteur = ouvrir(agent, adresse)?.take(max + 1);
    let mut octets = Vec::new();
    lecteur.read_to_end(&mut octets).map_err(|e| e.to_string())?;
    if octets.len() as u64 > max {
        return Err("Fichier trop gros.".into());
    }
    Ok(octets)
}
