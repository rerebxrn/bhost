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
    #[serde(skip_serializing_if = "Option::is_none")]
    disque: Option<Jauge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memoire: Option<Jauge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    processeur: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    /// Son adresse dans la maison (« 192.168.1.42 »), pour l'Accueil d'un serveur.
    #[serde(skip_serializing_if = "Option::is_none")]
    ip: Option<String>,
}

/// Une jauge de la machine (Go). Les noms suivent `Machine` de src/types.ts.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Jauge {
    #[serde(skip_serializing_if = "Option::is_none")]
    utilise_go: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reservee_go: Option<f64>,
    total_go: f64,
}

// ---- Machines de test (version de dev seulement) ----
// Sans machine dédiée, Thomas tape « TEST-TEST » (en ligne) ou « TEST-2222 » (hors ligne) pour voir la barre de gauche.
// La version publiée ne connaît pas ces codes (cfg!(debug_assertions) est faux).
const TEST_EN_LIGNE: (&str, &str) = ("TESTTEST", "7e57000000000001");
const TEST_HORS_LIGNE: (&str, &str) = ("TEST2222", "7e57000000000002");

fn machine_de_test(code: &str) -> Option<&'static str> {
    if !cfg!(debug_assertions) {
        return None;
    }
    [TEST_EN_LIGNE, TEST_HORS_LIGNE].iter().find(|(c, _)| *c == code).map(|(_, id)| *id)
}

fn vue(m: &MachineLiee) -> MachineVue {
    let mut v = MachineVue {
        id: m.id.clone(),
        nom: m.nom.clone(),
        code: m.code.clone(),
        etat: "hors-ligne",
        serveurs: 0,
        disque: None,
        memoire: None,
        processeur: None,
        temperature: None,
        ip: None,
    };
    if cfg!(debug_assertions) && m.id == TEST_EN_LIGNE.1 {
        // La température monte et descend toute seule (54 → 86 °C) pour voir le gris, l'orange et le rouge.
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let t = 54.0 + 32.0 * (((s / 10) % 9) as f64 / 8.0);
        v.etat = "en-ligne";
        v.disque = Some(Jauge { utilise_go: Some(168.0), reservee_go: None, total_go: 476.0 });
        // La mémoire réservée suit les serveurs de test allumés.
        let reservee = crate::ressources::reservee_mo(&allumes_de_test()) as f64 / 1024.0;
        v.memoire = Some(Jauge { utilise_go: None, reservee_go: Some((reservee * 10.0).round() / 10.0), total_go: TEST_RAM_GO as f64 });
        v.processeur = Some(18.0);
        v.temperature = Some(t.round());
        v.serveurs = SERVEURS_DE_TEST.len() as u32;
        v.ip = Some("192.168.1.42".into());
    }
    v
}

// Les serveurs de la machine de test (16 Go) : de quoi voir « Pas assez de mémoire » et la case Stockage.
// Rien ne tourne vraiment : Démarrer et Arrêter changent seulement leur état.
const TEST_RAM_GO: u64 = 16;
/// (id, nom, logiciel, icône, RAM en Go, stockage pris en Mo, stockage max en Go, allumé au départ)
const SERVEURS_DE_TEST: [(&str, &str, &str, &str, u64, u64, u64, bool); 3] = [
    ("test-public", "Serveur public", "Paper", "serveur-acier", 8, 6554, 30, true),
    ("test-amis", "Entre amis", "Purpur", "gateau", 4, 2150, 20, false),
    ("test-121", "Test 1.21", "Paper", "terminal", 2, 19354, 20, true),
];

/// Les serveurs de test allumés, et depuis quand (ms). Absent : l'état du départ.
static ALLUMES_TEST: Mutex<Option<Vec<(String, u64)>>> = Mutex::new(None);

fn avec_allumes<T>(f: impl FnOnce(&mut Vec<(String, u64)>) -> T) -> T {
    let mut g = ALLUMES_TEST.lock().unwrap();
    let liste = g.get_or_insert_with(|| {
        let il_y_a_2h = crate::maintenant_ms().saturating_sub(2 * 3600 * 1000);
        SERVEURS_DE_TEST.iter().filter(|s| s.7).map(|s| (s.0.to_string(), il_y_a_2h)).collect()
    });
    f(liste)
}

fn allumes_de_test() -> Vec<crate::ressources::Allume> {
    let allumes = avec_allumes(|l| l.clone());
    SERVEURS_DE_TEST
        .iter()
        .filter(|s| allumes.iter().any(|(id, _)| id == s.0))
        .map(|s| crate::ressources::Allume { id: s.0.into(), nom: s.1.into(), ram_mo: s.4 * 1024 })
        .collect()
}

/// Un id de serveur de la machine de test (version de dev seulement).
pub fn est_de_test(id: &str) -> bool {
    cfg!(debug_assertions) && SERVEURS_DE_TEST.iter().any(|s| s.0 == id)
}

/// Les serveurs de test, si la machine « TEST-TEST » est reliée : (serveur, statut, en ligne depuis, stockage pris en Mo).
pub fn serveurs_de_test(dossier: &Path) -> Vec<(crate::serveurs::Serveur, &'static str, Option<u64>, u64)> {
    if !cfg!(debug_assertions) || !lire(dossier).iter().any(|m| m.id == TEST_EN_LIGNE.1) {
        return Vec::new();
    }
    let allumes = avec_allumes(|l| l.clone());
    SERVEURS_DE_TEST
        .iter()
        .map(|s| {
            let depuis = allumes.iter().find(|(id, _)| id == s.0).map(|(_, t)| *t);
            let serveur = crate::serveurs::Serveur {
                id: s.0.into(),
                nom: s.1.into(),
                logiciel: s.2.into(),
                version: "1.21.4".into(),
                couleur: Some(s.3.into()),
                ram_mo: (s.4 * 1024) as u32,
                machine: Some(TEST_EN_LIGNE.1.into()),
                disque_max_mo: Some(s.6 * 1024),
                ..Default::default()
            };
            (serveur, if depuis.is_some() { "en-ligne" } else { "arrete" }, depuis, s.5)
        })
        .collect()
}

/// Démarre un serveur de test, si la mémoire de la machine le permet. Renvoie « en ligne depuis ».
pub fn demarrer_test(id: &str) -> Result<Option<u64>, crate::ressources::Manque> {
    let ram = SERVEURS_DE_TEST.iter().find(|s| s.0 == id).map(|s| s.4 * 1024).unwrap_or(0);
    let deja: Vec<_> = allumes_de_test().into_iter().filter(|a| a.id != id).collect();
    crate::ressources::verifier_demarrage(TEST_RAM_GO * 1024, &deja, ram)?;
    let maintenant = crate::maintenant_ms();
    avec_allumes(|l| {
        if !l.iter().any(|(x, _)| x == id) {
            l.push((id.to_string(), maintenant));
        }
    });
    Ok(Some(maintenant))
}

pub fn arreter_test(id: &str) {
    avec_allumes(|l| l.retain(|(x, _)| x != id));
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
    let id = match machine_de_test(&code) {
        Some(id) => id.to_string(),
        None => {
            let c = code.clone();
            tauri::async_runtime::spawn_blocking(move || demander_au_relais(&c)).await.map_err(|_| "Ça n'a pas marché.".to_string())??
        }
    };
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
    fn machines_de_test() {
        // Les tests tournent en version de dev : les deux codes de test existent.
        assert_eq!(machine_de_test("TESTTEST"), Some("7e57000000000001"));
        assert_eq!(machine_de_test("K7M2Q9XR"), None);
        let en_ligne = vue(&MachineLiee { id: TEST_EN_LIGNE.1.into(), nom: "Test".into(), code: "TESTTEST".into() });
        assert_eq!(en_ligne.etat, "en-ligne");
        assert!(en_ligne.temperature.is_some_and(|t| (54.0..=86.0).contains(&t)));
        let json = serde_json::to_string(&en_ligne).unwrap();
        assert!(json.contains("\"utiliseGo\":168.0") && json.contains("\"reserveeGo\":") && json.contains("\"ip\":\"192.168.1.42\""));
        assert_eq!(vue(&MachineLiee { id: TEST_HORS_LIGNE.1.into(), nom: "Test".into(), code: "TEST2222".into() }).etat, "hors-ligne");
    }

    #[test]
    fn serveurs_de_la_machine_de_test() {
        // Au départ : Serveur public (8 Go) et Test 1.21 (2 Go) tournent ; Entre amis (4 Go) ne rentre pas.
        let manque = demarrer_test("test-amis").unwrap_err();
        assert_eq!(manque.allumes.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["test-public", "test-121"]);
        arreter_test("test-121");
        assert!(demarrer_test("test-amis").is_ok_and(|d| d.is_some()));
        assert!(est_de_test("test-amis") && !est_de_test("survie"));
        // Remis comme au départ pour les autres tests.
        arreter_test("test-amis");
        demarrer_test("test-121").unwrap();
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
