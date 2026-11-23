// La mise à jour de l'agent d'une machine dédiée (BHOST OS ou VPS), sans réinstaller.
//
// Chaque version publiée sur GitHub contient l'agent (bhost-agent-linux-x86_64) et sa signature (.sig),
// faite dans GitHub Actions avec la clé secrète de BHOST. L'agent lit le numéro de la dernière version
// (latest.json), télécharge le nouvel agent, vérifie la signature, puis le pose dans <dossier>/systeme/agent.
// Il n'a pas le droit de se remplacer lui-même : c'est bhost-systeme (root) qui l'installe et le redémarre,
// pendant « Mettre à jour » (page Machine de BHOST). Les serveurs s'arrêtent proprement avant.

use crate::signature::{derniere, plus_recente, verifier, CLE_PUBLIQUE};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const BINAIRE: &str = "bhost-agent-linux-x86_64";
/// Un agent fait quelques Mo : au-delà, ce n'est pas lui.
const TAILLE_MAX: u64 = 64 * 1024 * 1024;
/// On ne redemande pas à GitHub plus d'une fois toutes les 6 heures (sauf « Rechercher »).
const ECART: Duration = Duration::from_secs(6 * 3600);

/// La dernière recherche : quand, et la version plus récente trouvée (s'il y en a une).
static VUE: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// La version plus récente déjà trouvée ; relance une recherche en fond si la dernière est vieille.
pub fn nouvelle(reseau: &ureq::Agent) -> Option<String> {
    let vue = VUE.lock().unwrap().clone();
    if vue.as_ref().is_none_or(|(quand, _)| quand.elapsed() > ECART) {
        let reseau = reseau.clone();
        std::thread::spawn(move || {
            let _ = chercher(&reseau);
        });
    }
    vue.and_then(|(_, v)| v)
}

/// Demande à GitHub la dernière version ; renvoie le numéro si elle est plus récente que cet agent.
pub fn chercher(reseau: &ureq::Agent) -> Result<Option<String>, String> {
    // Noté tout de suite : une recherche à la fois, même si GitHub ne répond pas.
    {
        let mut vue = VUE.lock().unwrap();
        let ancienne = vue.as_ref().and_then(|(_, v)| v.clone());
        *vue = Some((Instant::now(), ancienne));
    }
    let octets = crate::reseau::telecharger(reseau, &derniere("latest.json"))?.ok_or("Aucune version publiée.")?;
    let json: serde_json::Value = serde_json::from_slice(&octets).map_err(|_| "Réponse illisible.".to_string())?;
    let publiee = json.get("version").and_then(|v| v.as_str()).ok_or("Réponse illisible.")?.trim_start_matches('v').to_string();
    let nouvelle = plus_recente(&publiee, version()).then_some(publiee);
    *VUE.lock().unwrap() = Some((Instant::now(), nouvelle.clone()));
    Ok(nouvelle)
}

/// Télécharge et vérifie le nouvel agent, puis le pose dans `systeme/agent` pour bhost-systeme.
/// Renvoie la version posée, ou `None` si l'agent est déjà à jour.
pub fn preparer(reseau: &ureq::Agent, dossier: &Path) -> Result<Option<String>, String> {
    let Some(nouvelle) = chercher(reseau)? else { return Ok(None) };
    let binaire = crate::reseau::telecharger_gros(reseau, &derniere(BINAIRE), TAILLE_MAX)?;
    let sig = crate::reseau::telecharger(reseau, &derniere(&format!("{BINAIRE}.sig")))?.ok_or("Signature de l’agent introuvable.")?;
    verifier(&binaire, &String::from_utf8_lossy(&sig), CLE_PUBLIQUE)?;
    // Un programme Linux (ELF), pas une page d'erreur.
    if !binaire.starts_with(b"\x7fELF") {
        return Err("Agent abîmé.".into());
    }
    let cible = dossier.join("systeme").join("agent");
    std::fs::create_dir_all(cible.parent().unwrap()).map_err(|e| e.to_string())?;
    crate::garde::ecrire_atomique(&cible, &binaire).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&cible, std::fs::Permissions::from_mode(0o755));
    }
    Ok(Some(nouvelle))
}
