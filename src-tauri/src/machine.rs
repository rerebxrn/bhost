//! Machine dédiée (BHOST OS) : les clés USB (tutoriel) et les machines reliées par leur code.
//! L'écriture de la clé et le pilotage des serveurs de la machine viendront avec BHOST OS.

use serde::Serialize;

/// Une clé USB branchée sur ce PC.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleUsb {
    /// Lettre du lecteur, par exemple « E: ».
    pub lecteur: String,
    /// Nom de la clé (son étiquette), ou « Clé USB » si elle n'en a pas.
    pub nom: String,
    /// Taille en Go, arrondie au dixième.
    pub total_go: f64,
}

/// Les disques amovibles branchés (clés USB), pour l'étape « Prépare la clé USB ».
#[tauri::command]
pub fn lister_cles_usb() -> Vec<CleUsb> {
    let go = |octets: u64| (octets as f64 / 1_000_000_000.0 * 10.0).round() / 10.0;
    let Ok(disques) = sysinfo::Disks::new_with_refreshed_list() else {
        return Vec::new();
    };
    let mut cles: Vec<CleUsb> = disques
        .iter()
        .filter(|d| d.is_removable() && d.total_space() > 0)
        .map(|d| {
            let nom = d.name().to_string_lossy().trim().to_string();
            CleUsb {
                lecteur: d.mount_point().to_string_lossy().trim_end_matches(['\\', '/']).to_string(),
                nom: if nom.is_empty() { "Clé USB".into() } else { nom },
                total_go: go(d.total_space()),
            }
        })
        .collect();
    cles.sort_by(|a, b| a.lecteur.cmp(&b.lecteur));
    cles
}

// ---- Machines dédiées reliées à ce PC (code de 8 caractères) ----
// La liste est gardée dans %APPDATA%\BHOST\machines.json. Relier : BHOST demande au relais
// « LIER <code> » ; le relais répond l'id de la machine si elle est en ligne avec ce code.
// Après 5 codes faux, le relais fait attendre 15 minutes (anti-devinette).
// Le pilotage des serveurs de la machine (par le relais) arrive avec BHOST OS.

use crate::tunnel::protocole::{self, DuRelais};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

/// Les caractères possibles d'un code : pas de 0, O, 1 ni I (on les confond en recopiant).
pub const ALPHABET: &str = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
pub const LONGUEUR: usize = 8;

/// « k7m2-q9xr » → « K7M2Q9XR » ; `None` si le code n'a pas 8 caractères valides.
pub fn nettoyer_code(texte: &str) -> Option<String> {
    let mut code = String::new();
    for c in texte.chars() {
        if c == '-' || c.is_whitespace() {
            continue;
        }
        let c = c.to_ascii_uppercase();
        if !ALPHABET.contains(c) {
            return None;
        }
        code.push(c);
    }
    (code.chars().count() == LONGUEUR).then_some(code)
}

/// Une machine reliée, telle que gardée sur le disque.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MachineLiee {
    /// L'id donné par le relais (début de la clé publique de la machine).
    pub id: String,
    pub nom: String,
    pub code: String,
}

/// Ce que l'interface reçoit (type `Machine` de src/types.ts).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MachineVue {
    id: String,
    nom: String,
    code: String,
    /// « hors-ligne » tant que le pilotage par le relais n'existe pas (BHOST OS).
    etat: &'static str,
    serveurs: u32,
}

fn vue(m: &MachineLiee) -> MachineVue {
    MachineVue { id: m.id.clone(), nom: m.nom.clone(), code: m.code.clone(), etat: "hors-ligne", serveurs: 0 }
}

/// Une seule écriture du fichier à la fois.
static VERROU: Mutex<()> = Mutex::new(());

fn fichier(dossier: &Path) -> PathBuf {
    dossier.join("machines.json")
}

pub fn lire(dossier: &Path) -> Vec<MachineLiee> {
    std::fs::read(fichier(dossier)).ok().and_then(|o| serde_json::from_slice(&o).ok()).unwrap_or_default()
}

fn ecrire(dossier: &Path, liste: &[MachineLiee]) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(liste).map_err(|e| e.to_string())?;
    let temporaire = fichier(dossier).with_extension("json.tmp");
    std::fs::write(&temporaire, json).and_then(|_| std::fs::rename(&temporaire, fichier(dossier))).map_err(|_| "Impossible d'enregistrer la liste des machines.".to_string())
}

/// Ajoute (ou met à jour) une machine reliée. Nouvelle : « Machine dédiée », « Machine dédiée 2 »…
pub fn ajouter(liste: &mut Vec<MachineLiee>, id: &str, code: &str) -> MachineLiee {
    if let Some(m) = liste.iter_mut().find(|m| m.id == id) {
        m.code = code.to_string();
        return m.clone();
    }
    let mut nom = "Machine dédiée".to_string();
    let mut n = 2;
    while liste.iter().any(|m| m.nom == nom) {
        nom = format!("Machine dédiée {n}");
        n += 1;
    }
    let m = MachineLiee { id: id.to_string(), nom, code: code.to_string() };
    liste.push(m.clone());
    m
}

/// Un nom de machine : pas vide, 40 caractères au plus.
pub fn nom_valide(nom: &str) -> Result<String, String> {
    let nom = nom.trim();
    if nom.is_empty() {
        return Err("Donne-lui un nom.".into());
    }
    if nom.chars().count() > 40 {
        return Err("40 caractères au plus.".into());
    }
    Ok(nom.to_string())
}

/// La réponse du relais à « LIER », en phrase pour l'interface.
pub fn reponse_liaison(ligne: &str) -> Result<String, String> {
    match protocole::lire_du_relais(ligne) {
        Some(DuRelais::Liee { id }) => Ok(id),
        Some(DuRelais::Non { raison }) if raison == "bloque" => Err("Trop de codes faux. Réessaie dans 15 minutes.".into()),
        Some(DuRelais::Non { raison }) if raison == "code" => Err("Aucune machine avec ce code. Vérifie qu'elle est allumée.".into()),
        Some(DuRelais::Non { raison }) if raison == "version" => Err("Mets BHOST à jour pour relier une machine.".into()),
        _ => Err("Le relais a mal répondu. Réessaie.".into()),
    }
}

/// Demande au relais l'id de la machine qui a ce code.
fn demander_au_relais(code: &str) -> Result<String, String> {
    let injoignable = || "Le relais ne répond pas. Vérifie ta connexion à Internet.".to_string();
    let adresse = crate::tunnel::adresse_relais().map_err(|_| injoignable())?;
    let mut flux = crate::tunnel::connecter(adresse).map_err(|_| injoignable())?;
    let _ = flux.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = flux.set_write_timeout(Some(Duration::from_secs(10)));
    let lier = protocole::ecrire_de_bhost(&protocole::DeBhost::Lier { code: code.to_string() });
    flux.write_all(format!("BHOST {}\n{lier}", protocole::VERSION).as_bytes()).map_err(|_| injoignable())?;
    let mut ligne = String::new();
    // Une ligne du relais : 300 octets au plus.
    BufReader::new(std::io::Read::take(flux, 300)).read_line(&mut ligne).map_err(|_| injoignable())?;
    reponse_liaison(&ligne)
}

fn prevenir(app: &AppHandle, liste: &[MachineLiee]) {
    let _ = app.emit("machines", liste.iter().map(vue).collect::<Vec<_>>());
}

/// Les machines reliées à ce PC.
#[tauri::command]
pub fn machines_liees(etat: State<crate::Etat>) -> Vec<MachineVue> {
    lire(&etat.dossier).iter().map(vue).collect()
}

/// Relie une machine avec son code. Le relais est contacté hors du fil de l'interface.
#[tauri::command]
pub async fn lier_machine(app: AppHandle, code: String, etat: State<'_, crate::Etat>) -> Result<MachineVue, String> {
    let code = nettoyer_code(&code).ok_or("Ce code n'est pas valide. Vérifie-le.")?;
    let dossier = etat.dossier.clone();
    let c = code.clone();
    let id = tauri::async_runtime::spawn_blocking(move || demander_au_relais(&c)).await.map_err(|_| "Ça n'a pas marché.".to_string())??;
    let _une_seule = VERROU.lock().unwrap();
    let mut liste = lire(&dossier);
    let m = ajouter(&mut liste, &id, &code);
    ecrire(&dossier, &liste)?;
    prevenir(&app, &liste);
    Ok(vue(&m))
}

/// Retire une machine de BHOST (ses serveurs continuent de tourner dessus).
#[tauri::command]
pub fn delier_machine(app: AppHandle, id: String, etat: State<crate::Etat>) -> Result<(), String> {
    let _une_seule = VERROU.lock().unwrap();
    let mut liste = lire(&etat.dossier);
    liste.retain(|m| m.id != id);
    ecrire(&etat.dossier, &liste)?;
    prevenir(&app, &liste);
    Ok(())
}

/// Donne un autre nom à une machine (« Salon », « Grenier »…).
#[tauri::command]
pub fn renommer_machine(app: AppHandle, id: String, nom: String, etat: State<crate::Etat>) -> Result<(), String> {
    let nom = nom_valide(&nom)?;
    let _une_seule = VERROU.lock().unwrap();
    let mut liste = lire(&etat.dossier);
    let m = liste.iter_mut().find(|m| m.id == id).ok_or("Cette machine n'est plus reliée.")?;
    m.nom = nom;
    ecrire(&etat.dossier, &liste)?;
    prevenir(&app, &liste);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_nettoye() {
        assert_eq!(nettoyer_code("k7m2-q9xr").as_deref(), Some("K7M2Q9XR"));
        assert_eq!(nettoyer_code(" K7M2 Q9XR ").as_deref(), Some("K7M2Q9XR"));
        assert_eq!(nettoyer_code("K7M2Q9X"), None); // trop court
        assert_eq!(nettoyer_code("K7M2Q9XRA"), None); // trop long
        assert_eq!(nettoyer_code("K0M2Q9XR"), None); // 0 interdit
        assert_eq!(nettoyer_code("KIM2Q9XR"), None); // I interdit
    }

    #[test]
    fn noms_par_defaut_et_mise_a_jour() {
        let mut l = Vec::new();
        assert_eq!(ajouter(&mut l, "aaaa", "K7M2Q9XR").nom, "Machine dédiée");
        assert_eq!(ajouter(&mut l, "bbbb", "ABCD2345").nom, "Machine dédiée 2");
        // La même machine (code régénéré) : pas de doublon, le nom reste.
        l[0].nom = "Salon".into();
        let m = ajouter(&mut l, "aaaa", "ZZZZ2345");
        assert_eq!((m.nom.as_str(), m.code.as_str(), l.len()), ("Salon", "ZZZZ2345", 2));
    }

    #[test]
    fn liste_sur_le_disque() {
        let d = std::env::temp_dir().join(format!("bhost-machines-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(lire(&d).is_empty());
        let l = vec![MachineLiee { id: "ab".repeat(8), nom: "Salon".into(), code: "K7M2Q9XR".into() }];
        ecrire(&d, &l).unwrap();
        assert_eq!(lire(&d), l);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn reponses_du_relais() {
        assert_eq!(reponse_liaison(&format!("LIEE {}\n", "ab".repeat(8))), Ok("ab".repeat(8)));
        assert!(reponse_liaison("NON code\n").unwrap_err().contains("Aucune machine"));
        assert!(reponse_liaison("NON bloque\n").unwrap_err().contains("15 minutes"));
        assert!(reponse_liaison("n'importe quoi").is_err());
    }

    #[test]
    fn nom_de_machine() {
        assert_eq!(nom_valide("  Salon ").as_deref(), Ok("Salon"));
        assert!(nom_valide("   ").is_err());
        assert!(nom_valide(&"a".repeat(41)).is_err());
    }

    #[test]
    fn meme_alphabet_que_l_interface() {
        let ts = include_str!("../../src/code.ts");
        assert!(ts.contains(&format!("ALPHABET = \"{ALPHABET}\"")));
        assert!(ts.contains(&format!("LONGUEUR = {LONGUEUR};")));
        assert_eq!(ALPHABET, protocole::ALPHABET_CODE);
    }
}
