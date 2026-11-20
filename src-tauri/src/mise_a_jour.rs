// Réglages › Rechercher les mises à jour.
//
// Les nouvelles versions de BHOST sont publiées dans les Releases du dépôt GitHub :
// GitHub Actions fabrique l'installeur, le signe avec notre clé secrète et le pose dans une « release »
// avec un petit fichier latest.json (numéro de version + adresse + signature).
// BHOST lit ce fichier, télécharge l'installeur, vérifie la signature avec la clé publique ci-dessous
// (personne d'autre ne peut publier une fausse version), arrête les serveurs proprement, puis lance l'installeur.
// Marche à suivre pour publier : docs/PUBLIER.md.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Le dépôt GitHub des versions : « compte/dépôt ».
const DEPOT: &str = "rerebxrn/bhost";
/// La clé publique de signature (contenu du fichier .pub de `tauri signer generate`).
const CLE_PUBLIQUE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc4OUE1NzJCOUJBN0E5QjMKUldTenFhZWJLMWVhZVBvV0ppS0Q4K3JrRTAyRHR5QmFDVklBU1dZUEtyYVJBcyswNm9ja3F0aS8K";

/// Où lire la dernière version.
fn adresse() -> Option<String> {
    (!DEPOT.is_empty()).then(|| format!("https://github.com/{DEPOT}/releases/latest/download/latest.json"))
}

/// La mise à jour trouvée par la dernière recherche (pour l'installer ensuite).
#[derive(Default)]
pub struct EtatMaj {
    trouvee: Mutex<Option<Update>>,
}

/// Ce que la page Réglages affiche.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VueMaj {
    /// « a-jour » : rien de neuf ; « dispo » : `version` peut s'installer.
    etat: &'static str,
    version: Option<String>,
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let (Some(adresse), false) = (adresse(), CLE_PUBLIQUE.is_empty()) else {
        return Err("Les mises à jour arrivent bientôt.".into());
    };
    let adresse = adresse.parse().map_err(|_| "Adresse des mises à jour invalide.".to_string())?;
    app.updater_builder()
        .endpoints(vec![adresse])
        .and_then(|b| b.pubkey(CLE_PUBLIQUE).timeout(std::time::Duration::from_secs(30)).build())
        .map_err(|_| "La recherche n'a pas pu démarrer.".to_string())
}

#[tauri::command]
pub async fn chercher_mise_a_jour(app: AppHandle) -> Result<VueMaj, String> {
    let trouvee = updater(&app)?.check().await.map_err(|e| match e {
        // Aucune version publiée pour l'instant : on est donc à jour.
        tauri_plugin_updater::Error::ReleaseNotFound => String::new(),
        _ => "Pas de réponse. Vérifie ta connexion.".to_string(),
    });
    let trouvee = match trouvee {
        Ok(t) => t,
        Err(e) if e.is_empty() => None,
        Err(e) => return Err(e),
    };
    let vue = VueMaj { etat: if trouvee.is_some() { "dispo" } else { "a-jour" }, version: trouvee.as_ref().map(|u| u.version.clone()) };
    *app.state::<EtatMaj>().trouvee.lock().unwrap() = trouvee;
    Ok(vue)
}

/// Télécharge la version trouvée (event « mise-a-jour » : pourcentage), arrête les serveurs proprement,
/// puis lance l'installeur. BHOST se ferme et l'installeur le rouvre.
#[tauri::command]
pub async fn installer_mise_a_jour(app: AppHandle) -> Result<(), String> {
    let Some(maj) = app.state::<EtatMaj>().trouvee.lock().unwrap().clone() else {
        return Err("Recherche d'abord la mise à jour.".into());
    };
    let (mut recu, mut dernier) = (0u64, u8::MAX);
    let envoi = app.clone();
    let octets = maj
        .download(
            move |morceau, total| {
                recu += morceau as u64;
                let pourcent = total.filter(|t| *t > 0).map(|t| (recu * 100 / t).min(100) as u8).unwrap_or(0);
                if pourcent != dernier {
                    dernier = pourcent;
                    let _ = envoi.emit("mise-a-jour", pourcent);
                }
            },
            || {},
        )
        .await
        .map_err(|_| "Le téléchargement a échoué.".to_string())?;
    maj.verify(&octets).map_err(|_| "Version refusée : signature invalide.".to_string())?;
    // Les serveurs s'arrêtent proprement (le monde est enregistré) avant que l'installeur ferme BHOST.
    let _ = app.emit("mise-a-jour", 100u8);
    let arret = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::automatique::FERMETURE.store(true, std::sync::atomic::Ordering::Relaxed);
        crate::tout_arreter(&arret);
        arret.state::<crate::Etat>().acces.tout_fermer();
    })
    .await
    .map_err(|_| "Les serveurs ne se sont pas arrêtés.".to_string())?;
    // Sur Windows, install lance l'installeur et ferme BHOST : on ne revient pas ici si tout va bien.
    maj.install(octets).map_err(|_| "L'installation n'a pas pu démarrer.".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn adresse_github() {
        // Tant que le dépôt n'est pas noté, pas d'adresse (et pas de clé = pas d'installation possible).
        if super::DEPOT.is_empty() {
            assert_eq!(super::adresse(), None);
        } else {
            assert!(super::adresse().unwrap().ends_with("/releases/latest/download/latest.json"));
        }
    }
}
