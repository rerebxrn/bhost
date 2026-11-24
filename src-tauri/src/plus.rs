// BHOST+ : l'abonnement (thème Ambre, 40 icônes en plus, relais prioritaire, versions en avance).
//
// L'abonnement est vendu par Polar (polar.sh). Après le paiement, Polar envoie une clé par email.
// BHOST l'active sur ce PC (3 PC par clé), puis la revérifie au lancement et toutes les 6 heures :
//   - d'abord auprès du service BHOST+ (plus.bhost.app, code dans plus/) : il dit aussi sur quels PC la clé est
//     activée, si le paiement est en retard, et donne le jeton du relais prioritaire ;
//   - s'il ne répond pas, directement auprès de Polar (adresses publiques, aucun secret dans BHOST).
// Sans Internet, l'abonnement tient 7 jours après la dernière vérification réussie.
// Gardé dans %APPDATA%\BHOST\plus.json.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{garde, icone_jeu, serveurs, Etat};

/// L'identifiant de l'organisation BHOST chez Polar (Settings › Organization ID).
const ORGANISATION: &str = "17b8c054-e33d-4816-8d42-908ac29f0900";
/// Les pages Polar : abonnement mensuel, annuel (liens de paiement), et l'espace client (gérer, carte bancaire, factures).
const ACHAT_MENSUEL: &str = "https://buy.polar.sh/polar_cl_p1sxyZR7ihGvpHZxKuUozZ3TU6K5JWZpa9yNP0JgqEa";
const ACHAT_ANNUEL: &str = "https://buy.polar.sh/polar_cl_5MKMw46etnTApFlvjDvv2vAW9d3xKQIqWs7Py3IInk4";
const ESPACE_CLIENT: &str = "https://polar.sh/rebxrn/portal";

const API: &str = "https://api.polar.sh/v1/customer-portal/license-keys";
/// Le service BHOST+ (plus/, sur le serveur du relais).
const SERVICE: &str = "https://plus.bhost.app/etat";
const JOUR: i64 = 24 * 3600 * 1000;
/// Sans Internet : 7 jours d'abonnement après la dernière vérification, prévenu à partir du 3e.
const GRACE: i64 = 7 * JOUR;
const PREVENIR: i64 = 3 * JOUR;
/// Revérifier la clé toutes les 6 heures (toutes les 10 secondes pour les clés d'essai).
const INTERVALLE: i64 = 6 * 3600 * 1000;

/// Ce que BHOST garde de l'abonnement.
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct Abonnement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cle: Option<String>,
    /// L'activation de ce PC chez Polar (pour le retirer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activation: Option<String>,
    /// Fin de la clé ou prochain renouvellement (millisecondes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fin: Option<i64>,
    /// Dernière vérification réussie (millisecondes).
    #[serde(default)]
    verifie: i64,
    /// La clé n'est plus valable (abonnement arrêté ou ce PC retiré).
    #[serde(default)]
    expire: bool,
    /// Réglages › Versions en avance.
    #[serde(default)]
    avance: bool,
}

/// Un PC où la clé est activée (dit par le service BHOST+).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PcVu {
    id: String,
    nom: String,
    depuis: Option<i64>,
    #[serde(default)]
    ici: bool,
}

/// Ce que répond le service BHOST+ (plus/src/service.rs › Etat).
#[derive(Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
struct DuService {
    etat: String,
    formule: Option<String>,
    fin: Option<i64>,
    #[serde(default)]
    arrete: bool,
    places: Option<u64>,
    #[serde(default)]
    pcs: Vec<PcVu>,
    jeton: Option<String>,
}

pub struct EtatPlus {
    fichier: PathBuf,
    abonnement: Mutex<Abonnement>,
    /// La dernière réponse du service BHOST+ (pas gardée sur le disque : redemandée au lancement).
    service: Mutex<Option<DuService>>,
    /// Dernière tentative de vérification (millisecondes), réussie ou non.
    tentative: Mutex<i64>,
}

impl EtatPlus {
    pub fn nouveau(dossier: &std::path::Path) -> Self {
        let fichier = dossier.join("plus.json");
        let abonnement = std::fs::read(&fichier).ok().and_then(|o| serde_json::from_slice(&o).ok()).unwrap_or_default();
        EtatPlus { fichier, abonnement: Mutex::new(abonnement), service: Mutex::new(None), tentative: Mutex::new(0) }
    }

    fn enregistrer(&self, abonnement: &Abonnement) -> Result<(), String> {
        let texte = serde_json::to_vec_pretty(abonnement).map_err(|e| e.to_string())?;
        garde::ecrire_atomique(&self.fichier, &texte).map_err(|_| "Impossible d’enregistrer la clé.".to_string())
    }
}

fn maintenant() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// « aucun », « actif », « retard » (paiement refusé, Polar réessaie), « pause » (7 jours sans vérification) ou « expire ».
fn etat_de(a: &Abonnement, service: Option<&DuService>, maintenant: i64) -> &'static str {
    match (&a.cle, a.expire) {
        (None, _) => "aucun",
        (Some(_), true) => "expire",
        _ if maintenant - a.verifie > GRACE => "pause",
        _ if service.is_some_and(|s| s.etat == "retard") => "retard",
        _ => "actif",
    }
}

fn etat_actuel(plus: &EtatPlus) -> &'static str {
    etat_de(&plus.abonnement.lock().unwrap(), plus.service.lock().unwrap().as_ref(), maintenant())
}

/// Vrai si les avantages de BHOST+ sont ouverts sur ce PC (paiement en retard compris : Polar réessaie).
pub fn abonne(app: &AppHandle) -> bool {
    matches!(etat_actuel(&app.state::<EtatPlus>()), "actif" | "retard")
}

/// BHOST+ est branché à Polar (ou version de développement, pour les clés d'essai).
/// Tant que non, rien n'est fermé : tout reste comme avant BHOST+.
fn disponible() -> bool {
    !ORGANISATION.is_empty() || cfg!(debug_assertions)
}

/// Vrai si le thème Ambre et les icônes payantes sont fermés sur ce PC.
pub fn verrouille(app: &AppHandle) -> bool {
    disponible() && !abonne(app)
}

/// Réglages › Versions en avance, si l'abonnement est actif.
pub fn en_avance(app: &AppHandle) -> bool {
    abonne(app) && app.state::<EtatPlus>().abonnement.lock().unwrap().avance
}

/// Ce que l'interface affiche (event « bhost-plus » à chaque changement).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VuePlus {
    etat: &'static str,
    /// La clé masquée : « ••••-••••-A1B2 ».
    cle: Option<String>,
    fin: Option<i64>,
    /// « mois » ou « an » (dit par le service BHOST+).
    formule: Option<String>,
    /// L'abonnement est arrêté : il finit à `fin`.
    arrete: bool,
    /// Les PC où la clé est activée, et combien elle en permet (dit par le service BHOST+).
    pcs: Vec<PcVu>,
    places: Option<u64>,
    /// Sans vérification depuis plus de 3 jours : jours restants avant la pause.
    jours_sans_internet: Option<i64>,
    avance: bool,
    /// Faux tant que BHOST+ n'est pas branché à Polar (et hors essai).
    disponible: bool,
}

fn vue(plus: &EtatPlus) -> VuePlus {
    let a = plus.abonnement.lock().unwrap().clone();
    let service = plus.service.lock().unwrap().clone();
    let n = maintenant();
    let etat = etat_de(&a, service.as_ref(), n);
    let depuis = n - a.verifie;
    let avec_service = service.filter(|_| matches!(etat, "actif" | "retard"));
    let mut pcs = avec_service.as_ref().map(|s| s.pcs.clone()).unwrap_or_default();
    for pc in &mut pcs {
        pc.ici = a.activation.as_deref() == Some(pc.id.as_str());
    }
    VuePlus {
        etat,
        cle: a.cle.as_deref().map(masquer),
        fin: avec_service.as_ref().and_then(|s| s.fin).or(a.fin),
        formule: avec_service.as_ref().and_then(|s| s.formule.clone()),
        arrete: avec_service.as_ref().is_some_and(|s| s.arrete),
        pcs,
        places: avec_service.as_ref().and_then(|s| s.places),
        jours_sans_internet: (matches!(etat, "actif" | "retard") && depuis > PREVENIR).then(|| ((GRACE - depuis) as f64 / JOUR as f64).ceil().max(1.0) as i64),
        avance: a.avance,
        disponible: disponible(),
    }
}

fn masquer(cle: &str) -> String {
    let fin: String = cle.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("••••-••••-{fin}")
}

fn prevenir(app: &AppHandle) {
    let _ = app.emit("bhost-plus", vue(&app.state::<EtatPlus>()));
}

// ---------------------------------------------------------------------------------------------
// Polar et le service BHOST+
// ---------------------------------------------------------------------------------------------

/// Ce que Polar répond.
enum Reponse {
    Ok(Value),
    /// Clé inconnue, retirée, désactivée ou finie (ou ce PC retiré depuis l'espace client).
    Refusee,
    /// Déjà sur 3 PC.
    Pleine,
    /// Pas de réponse (pas d'Internet, Polar en panne…).
    Injoignable,
}

fn traduire(code: u16, corps: &str) -> Reponse {
    match code {
        200..=299 => Reponse::Ok(serde_json::from_str(corps).unwrap_or(Value::Null)),
        403 if corps.to_ascii_lowercase().contains("limit") => Reponse::Pleine,
        403 | 404 | 422 => Reponse::Refusee,
        _ => Reponse::Injoignable,
    }
}

/// POST en JSON ; (code, corps) ou None sans réponse.
fn poster(app: &AppHandle, adresse: &str, corps: &Value) -> Option<(u16, String)> {
    let agent = app.state::<Etat>().reseau.clone();
    let envoi = agent.post(adresse).config().http_status_as_error(false).build().content_type("application/json").send(corps.to_string());
    let mut r = envoi.ok()?;
    let code = r.status().as_u16();
    Some((code, r.body_mut().with_config().limit(256 * 1024).read_to_string().unwrap_or_default()))
}

fn appeler(app: &AppHandle, quoi: &str, corps: Value) -> Reponse {
    let cle = corps["key"].as_str().unwrap_or_default().to_string();
    if let Some(r) = essai::polar(&cle, quoi, corps["activation_id"].as_str()) {
        return r;
    }
    if ORGANISATION.is_empty() {
        return Reponse::Injoignable;
    }
    let mut corps = corps;
    corps["organization_id"] = json!(ORGANISATION);
    match poster(app, &format!("{API}/{quoi}"), &corps) {
        Some((code, texte)) => traduire(code, &texte),
        None => Reponse::Injoignable,
    }
}

/// Le service BHOST+ : None s'il ne répond pas (on se rabat sur Polar).
fn demander_service(app: &AppHandle, cle: &str, activation: Option<&str>) -> Option<DuService> {
    if let Some(r) = essai::service(cle, activation) {
        return r;
    }
    if ORGANISATION.is_empty() {
        return None;
    }
    // La clé publique de ce BHOST, celle qu'il montre au relais : le jeton prioritaire est fait pour elle.
    let agent = crate::tunnel::cle(&app.state::<Etat>().dossier).ok().map(|c| crate::tunnel::protocole::hexa(c.verifying_key().as_bytes()));
    let (code, texte) = poster(app, SERVICE, &json!({ "cle": cle, "activation": activation, "agent": agent }))?;
    (code == 200).then(|| serde_json::from_str(&texte).ok()).flatten()
}

/// Le jeton du relais prioritaire, pour les serveurs ouverts par le relais.
fn regler_jeton(service: Option<&DuService>) {
    let jeton = service
        .filter(|s| matches!(s.etat.as_str(), "actif" | "retard"))
        .and_then(|s| s.jeton.as_deref())
        .and_then(|j| j.split_once(' '))
        .and_then(|(e, s)| Some((e.parse().ok()?, s.to_string())));
    crate::tunnel::regler_priorite(jeton);
}

fn date(v: &Value) -> Option<i64> {
    v.as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|d| d.timestamp_millis())
}

fn nom_du_pc() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "PC".into())
}

/// Clés d'essai (versions de développement seulement), pour voir chaque cas sans payer :
/// TEST-ACTIF, TEST-RETARD, TEST-PLEIN (3 PC : en libérer un), TEST-EXPIRE (finit 10 s après),
/// TEST-HORSLIGNE (4 jours sans Internet), TEST-PAUSE (8 jours sans Internet). Toute autre clé TEST-… est inconnue.
mod essai {
    use super::{DuService, PcVu, Reponse};
    use serde_json::json;
    use std::sync::Mutex;

    /// Les PC de la clé d'essai, comme chez Polar.
    static PCS: Mutex<Vec<PcVu>> = Mutex::new(Vec::new());

    fn pc(id: &str, nom: &str, jours: i64) -> PcVu {
        PcVu { id: id.into(), nom: nom.into(), depuis: Some(super::maintenant() - jours * super::JOUR), ici: false }
    }

    fn actif(cle: &str) -> bool {
        cfg!(debug_assertions) && cle.starts_with("TEST-")
    }

    pub(super) fn polar(cle: &str, quoi: &str, activation: Option<&str>) -> Option<Reponse> {
        if !actif(cle) {
            return None;
        }
        let ok = || Reponse::Ok(json!({ "id": "essai-ici", "expires_at": null }));
        let mut pcs = PCS.lock().unwrap();
        Some(match (cle, quoi) {
            ("TEST-PLEIN", "activate") if pcs.len() >= 3 || pcs.is_empty() => {
                if pcs.is_empty() {
                    *pcs = vec![pc("essai-salon", "PC-SALON", 40), pc("essai-lea", "PORTABLE-LEA", 12), pc("essai-bureau", "PC-BUREAU", 3)];
                }
                Reponse::Pleine
            }
            ("TEST-ACTIF" | "TEST-RETARD" | "TEST-PLEIN" | "TEST-EXPIRE" | "TEST-HORSLIGNE" | "TEST-PAUSE", "activate") => {
                if cle != "TEST-PLEIN" {
                    *pcs = vec![pc("essai-lea", "PORTABLE-LEA", 12)];
                }
                pcs.push(pc("essai-ici", &super::nom_du_pc(), 0));
                ok()
            }
            (_, "deactivate") => {
                pcs.retain(|p| Some(p.id.as_str()) != activation);
                ok()
            }
            ("TEST-ACTIF" | "TEST-RETARD" | "TEST-PLEIN", "validate") => ok(),
            ("TEST-HORSLIGNE" | "TEST-PAUSE", "validate") => Reponse::Injoignable,
            _ => Reponse::Refusee,
        })
    }

    pub(super) fn service(cle: &str, activation: Option<&str>) -> Option<Option<DuService>> {
        if !actif(cle) {
            return None;
        }
        let pcs = PCS.lock().unwrap().clone();
        let etat = match cle {
            "TEST-HORSLIGNE" | "TEST-PAUSE" => return Some(None),
            "TEST-EXPIRE" => "inconnue",
            _ if activation.is_some() && !pcs.iter().any(|p| Some(p.id.as_str()) == activation) => "retire",
            "TEST-RETARD" => "retard",
            "TEST-ACTIF" | "TEST-PLEIN" => "actif",
            _ => "inconnue",
        };
        Some(Some(DuService {
            etat: etat.into(),
            formule: Some("an".into()),
            fin: Some(super::maintenant() + 300 * super::JOUR),
            arrete: false,
            places: Some(3),
            pcs,
            jeton: None,
        }))
    }
}

// ---------------------------------------------------------------------------------------------
// Commandes
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn etat_plus(plus: State<EtatPlus>) -> VuePlus {
    vue(&plus)
}

/// Réglages › BHOST+ › Clé › Activer.
#[tauri::command]
pub async fn activer_plus(app: AppHandle, cle: String) -> Result<VuePlus, String> {
    crate::a_part(move || {
        let cle = cle.trim().to_string();
        if cle.is_empty() {
            return Err("Colle ta clé BHOST+.".into());
        }
        let reponse = appeler(&app, "activate", json!({ "key": cle, "label": nom_du_pc(), "meta": { "appli": "BHOST" } }));
        let v = match reponse {
            Reponse::Ok(v) => v,
            Reponse::Refusee => return Err("Clé inconnue. Vérifie-la dans l’email de Polar.".into()),
            // L'interface affiche alors les 3 PC (pcs_plus) pour en libérer un.
            Reponse::Pleine => return Err("PLEINE".into()),
            Reponse::Injoignable if !ORGANISATION.is_empty() => return Err("Pas de réponse. Vérifie ta connexion.".into()),
            Reponse::Injoignable => return Err("BHOST+ arrive bientôt.".into()),
        };
        let plus = app.state::<EtatPlus>();
        let n = maintenant();
        {
            let mut a = plus.abonnement.lock().unwrap();
            // Les clés d'essai sans Internet font comme si la dernière vérification datait de 4 ou 8 jours.
            let verifie = match cle.as_str() {
                "TEST-HORSLIGNE" if cfg!(debug_assertions) => n - 4 * JOUR,
                "TEST-PAUSE" if cfg!(debug_assertions) => n - 8 * JOUR,
                _ => n,
            };
            *a = Abonnement {
                cle: Some(cle),
                activation: v["id"].as_str().map(String::from),
                fin: date(&v["license_key"]["expires_at"]).or_else(|| date(&v["expires_at"])),
                verifie,
                expire: false,
                avance: a.avance,
            };
            plus.enregistrer(&a)?;
        }
        // Tout de suite les PC, la formule et le jeton prioritaire.
        *plus.tentative.lock().unwrap() = 0;
        verifier(&app);
        prevenir(&app);
        Ok(vue(&plus))
    })
    .await
}

/// Les PC d'une clé pleine (3 sur 3), pour en libérer un avant d'activer ce PC.
#[tauri::command]
pub async fn pcs_plus(app: AppHandle, cle: String) -> Result<Vec<PcVu>, String> {
    crate::a_part(move || match demander_service(&app, cle.trim(), None) {
        Some(s) if matches!(s.etat.as_str(), "actif" | "retard") => Ok(s.pcs),
        Some(_) => Err("Clé inconnue. Vérifie-la dans l’email de Polar.".into()),
        None => Err("Pas de réponse. Retire un PC depuis ton espace client.".into()),
    })
    .await
}

/// Libère la place d'un autre PC (`cle` : celle qu'on colle, si ce PC n'est pas encore activé).
#[tauri::command]
pub async fn liberer_pc(app: AppHandle, cle: Option<String>, pc: String) -> Result<VuePlus, String> {
    crate::a_part(move || {
        let plus = app.state::<EtatPlus>();
        let a = plus.abonnement.lock().unwrap().clone();
        if cle.is_none() && a.activation.as_deref() == Some(pc.as_str()) {
            return Err("Pour ce PC, utilise « Retirer de ce PC ».".into());
        }
        let Some(cle) = cle.map(|c| c.trim().to_string()).or(a.cle) else { return Err("Colle ta clé BHOST+.".into()) };
        match appeler(&app, "deactivate", json!({ "key": cle, "activation_id": pc })) {
            Reponse::Ok(_) | Reponse::Refusee => {}
            _ => return Err("Pas de réponse. Vérifie ta connexion.".into()),
        }
        *plus.tentative.lock().unwrap() = 0;
        verifier(&app);
        prevenir(&app);
        Ok(vue(&plus))
    })
    .await
}

/// Réglages › BHOST+ › Retirer de ce PC : libère une place de la clé.
#[tauri::command]
pub async fn retirer_plus(app: AppHandle) -> Result<VuePlus, String> {
    crate::a_part(move || {
        let plus = app.state::<EtatPlus>();
        let a = plus.abonnement.lock().unwrap().clone();
        if let (Some(cle), Some(activation), false) = (&a.cle, &a.activation, a.expire) {
            match appeler(&app, "deactivate", json!({ "key": cle, "activation_id": activation })) {
                Reponse::Ok(_) | Reponse::Refusee => {}
                _ => return Err("Pas de réponse. Vérifie ta connexion.".into()),
            }
        }
        let vide = Abonnement::default();
        plus.enregistrer(&vide)?;
        *plus.abonnement.lock().unwrap() = vide;
        *plus.service.lock().unwrap() = None;
        regler_jeton(None);
        rendre_gratuit(&app);
        prevenir(&app);
        Ok(vue(&plus))
    })
    .await
}

/// Réglages › BHOST › Versions en avance.
#[tauri::command]
pub fn regler_avance(app: AppHandle, oui: bool, plus: State<EtatPlus>) -> Result<VuePlus, String> {
    if oui && !abonne(&app) {
        return Err("Les versions en avance viennent avec BHOST+.".into());
    }
    {
        let mut a = plus.abonnement.lock().unwrap();
        a.avance = oui;
        plus.enregistrer(&a)?;
    }
    Ok(vue(&plus))
}

/// Ouvre une page Polar : « mensuel », « annuel » ou « espace ».
#[tauri::command]
pub fn ouvrir_plus(app: AppHandle, page: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let adresse = match page.as_str() {
        "mensuel" => ACHAT_MENSUEL,
        "annuel" => ACHAT_ANNUEL,
        _ => ESPACE_CLIENT,
    };
    if adresse.is_empty() {
        return Err("BHOST+ arrive bientôt.".into());
    }
    app.opener().open_url(adresse, None::<&str>).map_err(|_| "La page n’a pas pu s’ouvrir.".to_string())
}

// ---------------------------------------------------------------------------------------------
// Vérification régulière
// ---------------------------------------------------------------------------------------------

/// Revérifie la clé au lancement, puis toutes les 6 heures.
pub fn surveiller(app: AppHandle) {
    std::thread::spawn(move || loop {
        let avant = serde_json::to_string(&vue(&app.state::<EtatPlus>())).unwrap_or_default();
        verifier(&app);
        // Prévenir seulement si quelque chose a changé (et chaque jour qui passe sans Internet).
        if serde_json::to_string(&vue(&app.state::<EtatPlus>())).unwrap_or_default() != avant {
            prevenir(&app);
        }
        std::thread::sleep(Duration::from_secs(10));
    });
}

fn verifier(app: &AppHandle) {
    let plus = app.state::<EtatPlus>();
    let a = plus.abonnement.lock().unwrap().clone();
    let (Some(cle), false) = (a.cle.clone(), a.expire) else { return };
    let n = maintenant();
    let intervalle = if cfg!(debug_assertions) && cle.starts_with("TEST-") { 10_000 } else { INTERVALLE };
    {
        let mut tentative = plus.tentative.lock().unwrap();
        if n - *tentative < intervalle {
            return;
        }
        *tentative = n;
    }
    let avant = etat_actuel(&plus);
    // D'abord le service BHOST+ (PC, retard, jeton), sinon Polar directement.
    let service = demander_service(app, &cle, a.activation.as_deref());
    let polar = if service.is_none() {
        let mut corps = json!({ "key": cle });
        if let Some(activation) = &a.activation {
            corps["activation_id"] = json!(activation);
        }
        Some(appeler(app, "validate", corps))
    } else {
        None
    };
    {
        let mut a = plus.abonnement.lock().unwrap();
        if a.cle.as_deref() != Some(cle.as_str()) {
            return; // la clé a changé pendant la vérification
        }
        match (&service, polar) {
            (Some(s), _) => match s.etat.as_str() {
                "actif" | "retard" => {
                    a.verifie = n;
                    a.fin = s.fin.or(a.fin);
                }
                // Clé finie, retirée, ou ce PC libéré depuis un autre.
                _ => a.expire = true,
            },
            (None, Some(Reponse::Ok(v))) => {
                a.verifie = n;
                a.fin = date(&v["expires_at"]).or(a.fin);
                if v["status"].as_str().is_some_and(|s| s != "granted") || a.fin.is_some_and(|f| f < n) {
                    a.expire = true;
                }
            }
            (None, Some(Reponse::Refusee | Reponse::Pleine)) => a.expire = true,
            _ => {}
        }
        let _ = plus.enregistrer(&a);
    }
    if service.is_some() {
        regler_jeton(service.as_ref());
        *plus.service.lock().unwrap() = service;
    }
    let apres = etat_actuel(&plus);
    if apres == "expire" && avant != "expire" {
        *plus.service.lock().unwrap() = None;
        regler_jeton(None);
        rendre_gratuit(app);
    }
}

/// L'abonnement s'arrête : les serveurs avec une icône payante reprennent celle de leur logiciel
/// (l'interface relit la liste à l'event « bhost-plus » ; le thème Ambre y repasse en Azur).
/// Seulement au passage à « expiré » : pas pendant la pause.
fn rendre_gratuit(app: &AppHandle) {
    let etat = app.state::<Etat>();
    let mut liste = etat.liste.lock().unwrap();
    let mut changes = Vec::new();
    for s in liste.iter_mut() {
        if s.couleur.as_deref().is_some_and(|c| !serveurs::icone_gratuite(c)) {
            s.couleur = None;
            changes.push((s.dossier.clone(), icone_jeu::par_defaut(&s.logiciel)));
        }
    }
    if changes.is_empty() {
        return;
    }
    let _ = serveurs::enregistrer(&etat.fichier, &liste);
    drop(liste);
    for (dossier, cle) in changes {
        let _ = icone_jeu::poser(std::path::Path::new(&dossier), cle, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comprend_polar() {
        assert!(matches!(traduire(200, r#"{"id":"a"}"#), Reponse::Ok(_)));
        assert!(matches!(traduire(403, r#"{"detail":"License key activation limit already reached"}"#), Reponse::Pleine));
        assert!(matches!(traduire(403, r#"{"detail":"License key is revoked"}"#), Reponse::Refusee));
        assert!(matches!(traduire(404, ""), Reponse::Refusee));
        assert!(matches!(traduire(500, ""), Reponse::Injoignable));
    }

    #[test]
    fn sept_jours_sans_internet_et_retard() {
        let n = 100 * JOUR;
        let a = |verifie, expire| Abonnement { cle: Some("x".into()), verifie, expire, ..Default::default() };
        let retard = DuService { etat: "retard".into(), ..Default::default() };
        assert_eq!(etat_de(&Abonnement::default(), None, n), "aucun");
        assert_eq!(etat_de(&a(n - 6 * JOUR, false), None, n), "actif");
        assert_eq!(etat_de(&a(n, false), Some(&retard), n), "retard");
        assert_eq!(etat_de(&a(n - 8 * JOUR, false), Some(&retard), n), "pause");
        assert_eq!(etat_de(&a(n, true), None, n), "expire");
    }

    #[test]
    fn lit_le_service() {
        let s: DuService = serde_json::from_str(
            r#"{"etat":"actif","formule":"an","fin":1,"arrete":false,"places":3,"pcs":[{"id":"a","nom":"PC","depuis":5}],"jeton":"9 ab"}"#,
        )
        .unwrap();
        assert_eq!(s.pcs.len(), 1);
        assert_eq!(s.jeton.as_deref(), Some("9 ab"));
    }

    #[test]
    fn masque_la_cle() {
        assert_eq!(masquer("BHOST-1234-ABCD-A1B2"), "••••-••••-A1B2");
    }
}
