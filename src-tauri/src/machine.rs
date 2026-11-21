//! Machine dédiée (BHOST OS) : les clés USB (tutoriel) et les machines reliées par leur code.
//! L'écriture de la clé et le pilotage des serveurs de la machine viendront avec BHOST OS.

use serde::Serialize;

/// Une clé USB branchée sur ce PC.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CleUsb {
    /// Son numéro de disque chez Windows (c'est lui qu'on écrit).
    pub disque: u32,
    /// Ses lettres de lecteur, par exemple « E: » (vide si Windows ne sait pas la lire).
    pub lecteur: String,
    /// Le nom de la clé (son modèle), ou « Clé USB ».
    pub nom: String,
    /// Taille en Go, arrondie au dixième.
    pub total_go: f64,
}

/// Les clés USB branchées, pour l'étape « Prépare la clé USB » (même une clé illisible pour Windows).
#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn lister_cles_usb() -> Vec<CleUsb> {
    crate::hote::async_runtime::spawn_blocking(|| if cfg!(windows) { crate::cle::cles_windows() } else { Vec::new() }).await.unwrap_or_default()
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
use crate::hote::{AppHandle, Emitter, State};

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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MachineLiee {
    /// L'id donné par le relais (début de la clé publique de la machine).
    pub id: String,
    pub nom: String,
    pub code: String,
    /// Sa clé publique entière (hexa), retenue à la première connexion chiffrée : ensuite, seule elle est acceptée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cle: Option<String>,
    /// Sa dernière adresse connue (réseau de la maison, ou adresse publique d'un VPS) : BHOST y vient d'abord en direct.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
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
    // Une vraie machine : en ligne si BHOST la pilote en ce moment ; ses mesures viennent d'« etat_machine ».
    if lien_vivant(&m.id) {
        v.etat = "en-ligne";
        if let Some(e) = MESURES.lock().unwrap().get(&m.id) {
            let f = |x: &Value, k: &str| x.get(k).and_then(Value::as_f64);
            v.disque = e.get("disque").and_then(|d| Some(Jauge { utilise_go: f(d, "utiliseGo"), reservee_go: None, total_go: f(d, "totalGo")? }));
            v.memoire = e.get("memoire").and_then(|d| Some(Jauge { utilise_go: None, reservee_go: f(d, "reserveeGo"), total_go: f(d, "totalGo")? }));
            v.processeur = f(e, "processeur");
            v.temperature = f(e, "temperature");
            v.serveurs = e.get("serveurs").and_then(Value::as_u64).unwrap_or(0) as u32;
            v.ip = e.get("ip").and_then(Value::as_str).map(str::to_string);
        }
    }
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
    let m = MachineLiee { id: id.to_string(), nom, code: code.to_string(), ..Default::default() };
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

/// Les machines reliées à ce PC. BHOST en profite pour se (re)connecter à celles qui ne le sont pas
/// (au plus une fois toutes les 15 s chacune) et pour relire leurs mesures. Hors du fil de l'interface.
#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn machines_liees(app: AppHandle, etat: State<'_, crate::Etat>) -> Result<Vec<MachineVue>, String> {
    let dossier = etat.dossier.clone();
    crate::hote::async_runtime::spawn_blocking(move || {
        let liste = lire(&dossier);
        for m in liste.iter().filter(|m| machine_de_test(&m.code).is_none()) {
            if let Ok(l) = lien(&app, m) {
                if let Ok(e) = l.appeler("etat_machine", Value::Null, Duration::from_secs(5)) {
                    // Son adresse a changé : BHOST y viendra en direct la prochaine fois.
                    if let Some(ip) = e.get("ip").and_then(Value::as_str).filter(|ip| m.ip.as_deref() != Some(*ip)) {
                        noter(&dossier, &m.id, |x| x.ip = Some(ip.to_string()));
                    }
                    MESURES.lock().unwrap().insert(m.id.clone(), e);
                }
            }
        }
        liste.iter().map(vue).collect()
    })
    .await
    .map_err(|e| e.to_string())
}

// ---- Piloter une machine (en direct, sinon par le relais) ----
// BHOST essaie d'abord la machine en direct (sa dernière adresse, port 4446 : même réseau, ou VPS).
// Sinon, il envoie « PILOTER <code> <id> » au relais, qui le branche sur la machine.
// Dans les deux cas, la connexion est chiffrée (TLS, chiffre.rs) et BHOST donne le code de la machine.
// Ensuite, les commandes partent en JSON (une ligne : {"n", "cmd", "args"}) et les réponses reviennent
// ({"n", "ok"|"err"}), mêlées aux nouvelles de la machine ({"evt", "data"}) que BHOST renvoie à l'interface
// (« statut », « console »… : les mêmes que pour les serveurs de ce PC).

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

/// Une connexion ouverte vers une machine.
struct Lien {
    canal: crate::chiffre::Canal,
    attentes: Mutex<HashMap<u64, mpsc::Sender<Result<Value, Value>>>>,
    compteur: AtomicU64,
    vivant: AtomicBool,
}

static LIENS: Mutex<Option<HashMap<String, Arc<Lien>>>> = Mutex::new(None);
/// Le dernier essai de connexion à chaque machine (on ne réessaie pas plus d'une fois toutes les 15 s).
static ESSAIS: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
/// Les dernières mesures de chaque machine (« etat_machine »).
static MESURES: std::sync::LazyLock<Mutex<HashMap<String, Value>>> = std::sync::LazyLock::new(Default::default);
/// Les machines dont le code a changé (Régénérer le code) : on arrête d'essayer, il faut la relier à nouveau.
static CODE_CHANGE: std::sync::LazyLock<Mutex<HashSet<String>>> = std::sync::LazyLock::new(Default::default);

fn lien_vivant(id: &str) -> bool {
    LIENS.lock().unwrap().as_ref().and_then(|l| l.get(id)).is_some_and(|l| l.vivant.load(Ordering::Relaxed))
}

impl Lien {
    /// Envoie une commande et attend sa réponse (au plus `delai`).
    fn appeler(&self, commande: &str, args: Value, delai: Duration) -> Result<Value, Value> {
        let hors_ligne = || Value::String("La machine ne répond pas. Vérifie qu'elle est allumée.".into());
        if !self.vivant.load(Ordering::Relaxed) {
            return Err(hors_ligne());
        }
        let n = self.compteur.fetch_add(1, Ordering::Relaxed);
        let (envoi, reception) = mpsc::channel();
        self.attentes.lock().unwrap().insert(n, envoi);
        let ligne = serde_json::json!({ "n": n, "cmd": commande, "args": args }).to_string();
        if self.canal.ecrire_ligne(&ligne).is_err() {
            self.attentes.lock().unwrap().remove(&n);
            return Err(hors_ligne());
        }
        let reponse = reception.recv_timeout(delai);
        self.attentes.lock().unwrap().remove(&n);
        reponse.unwrap_or_else(|_| Err(hors_ligne()))
    }
}

/// La connexion à cette machine : celle qui est ouverte, ou une nouvelle.
fn lien(app: &AppHandle, m: &MachineLiee) -> Result<Arc<Lien>, String> {
    if let Some(l) = LIENS.lock().unwrap().get_or_insert_with(HashMap::new).get(&m.id).filter(|l| l.vivant.load(Ordering::Relaxed)) {
        return Ok(l.clone());
    }
    if CODE_CHANGE.lock().unwrap().contains(&m.id) {
        return Err(CODE_A_CHANGE.into());
    }
    {
        let mut essais = ESSAIS.lock().unwrap();
        let essais = essais.get_or_insert_with(HashMap::new);
        if essais.get(&m.id).is_some_and(|t| t.elapsed() < Duration::from_secs(15)) {
            return Err("La machine ne répond pas. Vérifie qu'elle est allumée.".into());
        }
        essais.insert(m.id.clone(), Instant::now());
    }
    let l = ouvrir_lien(app, m)?;
    LIENS.lock().unwrap().get_or_insert_with(HashMap::new).insert(m.id.clone(), l.clone());
    Ok(l)
}

const MACHINE_MUETTE: &str = "La machine ne répond pas. Vérifie qu'elle est allumée.";
const CODE_A_CHANGE: &str = "Le code de cette machine a changé. Retire-la, puis ajoute-la avec son nouveau code.";
const PAS_LA_MEME: &str = "Ce n'est plus la même machine. Retire-la, puis ajoute-la avec son code.";

/// Une connexion chiffrée et acceptée par la machine : de quoi écrire, de quoi lire, sa clé, et « en direct ? ».
type Connexion = (crate::chiffre::Canal, BufReader<crate::chiffre::Lecteur>, [u8; 32], bool);

/// Donne le code à la machine (dans le tunnel chiffré) et attend « OK <id> ».
fn presenter(c: crate::chiffre::Chiffree, m: &MachineLiee) -> Result<(crate::chiffre::Canal, BufReader<crate::chiffre::Lecteur>), String> {
    c.canal.ecrire_ligne(&format!("CODE {}", m.code)).map_err(|_| MACHINE_MUETTE.to_string())?;
    let mut lecteur = BufReader::new(c.lecteur);
    let mut ligne = String::new();
    std::io::Read::take(&mut lecteur, 300).read_line(&mut ligne).map_err(|_| MACHINE_MUETTE.to_string())?;
    match ligne.trim().split(' ').collect::<Vec<_>>()[..] {
        ["OK", id] if id == m.id => {}
        ["NON", "code"] => return Err(CODE_A_CHANGE.into()),
        ["NON", "bloque"] => return Err("Trop de codes faux. Réessaie dans 15 minutes.".into()),
        _ => return Err(MACHINE_MUETTE.into()),
    }
    // Les commandes peuvent être longues (création d'un serveur) : plus de délai de lecture.
    lecteur.get_ref().delai(None).map_err(|_| MACHINE_MUETTE.to_string())?;
    Ok((c.canal, lecteur))
}

/// La poignée de main chiffrée ; une clé qui n'est pas la bonne n'est pas « machine éteinte ».
fn chiffrer(flux: std::net::TcpStream, m: &MachineLiee, connue: Option<[u8; 32]>) -> Result<(crate::chiffre::Chiffree, [u8; 32]), String> {
    crate::chiffre::connecter(flux, &m.id, connue).map_err(|e| {
        let texte = e.to_string();
        if texte.contains("certificate") || texte.contains("Certificate") { PAS_LA_MEME.to_string() } else { MACHINE_MUETTE.to_string() }
    })
}

/// En direct : la machine sur sa dernière adresse connue (même réseau, ou VPS), port 4446.
fn en_direct(m: &MachineLiee, connue: Option<[u8; 32]>) -> Result<Connexion, String> {
    let ip: std::net::IpAddr = m.ip.as_deref().and_then(|ip| ip.parse().ok()).ok_or(MACHINE_MUETTE)?;
    let adresse = std::net::SocketAddr::new(ip, crate::chiffre::port_direct());
    let flux = std::net::TcpStream::connect_timeout(&adresse, Duration::from_millis(1500)).map_err(|_| MACHINE_MUETTE.to_string())?;
    let _ = flux.set_nodelay(true);
    let _ = flux.set_read_timeout(Some(Duration::from_secs(5)));
    let (c, cle) = chiffrer(flux, m, connue)?;
    let (canal, lecteur) = presenter(c, m)?;
    Ok((canal, lecteur, cle, true))
}

/// Par le relais : « PILOTER <code> <id> », puis le même tunnel chiffré.
fn par_le_relais(m: &MachineLiee, connue: Option<[u8; 32]>) -> Result<Connexion, String> {
    let injoignable = || "Le relais ne répond pas. Vérifie ta connexion à Internet.".to_string();
    let adresse = crate::tunnel::adresse_relais().map_err(|_| injoignable())?;
    let mut flux = crate::tunnel::connecter(adresse).map_err(|_| injoignable())?;
    let _ = flux.set_read_timeout(Some(Duration::from_secs(15)));
    let piloter = protocole::ecrire_de_bhost(&protocole::DeBhost::Piloter { code: m.code.clone(), id: m.id.clone() });
    flux.write_all(format!("BHOST {}\n{piloter}", protocole::VERSION).as_bytes()).map_err(|_| injoignable())?;
    // Octet par octet jusqu'à la fin de la ligne : ce qui suit est déjà le tunnel chiffré.
    let mut ligne = Vec::new();
    let mut octet = [0u8; 1];
    while ligne.len() < 300 && !ligne.ends_with(b"\n") {
        match std::io::Read::read(&mut flux, &mut octet) {
            Ok(1) => ligne.push(octet[0]),
            _ => return Err(MACHINE_MUETTE.into()),
        }
    }
    match protocole::lire_du_relais(&String::from_utf8_lossy(&ligne)) {
        Some(DuRelais::Ok { adresse }) if adresse == m.id => {}
        Some(DuRelais::Non { raison }) if raison == "code" => return Err(CODE_A_CHANGE.into()),
        Some(DuRelais::Non { raison }) if raison == "bloque" => return Err("Trop de codes faux. Réessaie dans 15 minutes.".into()),
        Some(DuRelais::Non { raison }) if raison == "version" => return Err("Mets BHOST à jour pour piloter cette machine.".into()),
        _ => return Err(MACHINE_MUETTE.into()),
    }
    let (c, cle) = chiffrer(flux, m, connue)?;
    let (canal, lecteur) = presenter(c, m)?;
    Ok((canal, lecteur, cle, false))
}

/// En direct si possible, sinon par le relais.
fn connexion(m: &MachineLiee) -> Result<Connexion, String> {
    let connue = m.cle.as_deref().and_then(protocole::depuis_hexa).and_then(|o| <[u8; 32]>::try_from(o).ok());
    if m.ip.is_some() {
        match en_direct(m, connue) {
            Ok(c) => return Ok(c),
            // La machine a répondu (mauvais code, autre clé) : pas la peine d'essayer le relais.
            Err(e) if e != MACHINE_MUETTE => return Err(e),
            Err(_) => {}
        }
    }
    par_le_relais(m, connue)
}

/// Change une machine dans machines.json (sa clé, son adresse…).
fn noter(dossier: &Path, id: &str, changer: impl FnOnce(&mut MachineLiee)) {
    let _verrou = VERROU.lock().unwrap();
    let mut liste = lire(dossier);
    if let Some(m) = liste.iter_mut().find(|m| m.id == id) {
        changer(m);
        let _ = ecrire(dossier, &liste);
    }
}

fn ouvrir_lien(app: &AppHandle, m: &MachineLiee) -> Result<Arc<Lien>, String> {
    let (canal, lecteur, cle, direct) = match connexion(m) {
        Ok(c) => c,
        Err(e) => {
            if e == CODE_A_CHANGE {
                CODE_CHANGE.lock().unwrap().insert(m.id.clone());
            }
            return Err(e);
        }
    };
    eprintln!("[bhost] machine {} : connectée {}", m.id, if direct { "en direct" } else { "par le relais" });
    // Première connexion chiffrée : on retient sa clé (ensuite, seule elle est acceptée).
    if m.cle.is_none()
        && let Some(etat) = crate::hote::Manager::try_state::<crate::Etat>(app)
    {
        noter(&etat.dossier, &m.id, |x| x.cle = Some(protocole::hexa(&cle)));
    }
    let (app, id) = (app.clone(), m.id.clone());
    let l = demarrer_lien(canal, lecteur, move |evt, data| match evt {
        // La machine n'a pas d'écran : c'est ce PC qui affiche ses notifications.
        "notification" => {
            let texte = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            crate::hote::notifier(&app, &texte("titre"), &texte("texte"), false);
        }
        // « Ouvert à tes amis » : la liste de la machine, à part de celle de ce PC ([] : elle est partie).
        "acces" => {
            let _ = app.emit("acces-machine", serde_json::json!({ "machine": id, "liste": data }));
        }
        // Internet de la machine : sans lui, elle ne répondrait pas ; celui de ce PC reste affiché.
        "internet" => {}
        // La connexion est tombée : l'interface voit la machine hors ligne.
        "fin" => {
            let _ = app.emit("acces-machine", serde_json::json!({ "machine": id, "liste": [] }));
            if let Some(etat) = crate::hote::Manager::try_state::<crate::Etat>(&app) {
                prevenir(&app, &lire(&etat.dossier));
            }
        }
        _ => {
            let _ = app.emit(evt, data.clone());
        }
    });
    Ok(l)
}

/// Le lien, et le fil qui lit la machine : les réponses vont aux commandes qui attendent,
/// les nouvelles à `nouvelle` (et « fin » quand la connexion tombe).
fn demarrer_lien(canal: crate::chiffre::Canal, mut lecteur: BufReader<crate::chiffre::Lecteur>, nouvelle: impl Fn(&str, &Value) + Send + 'static) -> Arc<Lien> {
    let l = Arc::new(Lien { canal, attentes: Mutex::new(HashMap::new()), compteur: AtomicU64::new(1), vivant: AtomicBool::new(true) });
    let lu = l.clone();
    std::thread::spawn(move || {
        // 64 Mo au plus par ligne (une réponse avec un gros fichier).
        loop {
            let mut ligne = String::new();
            match std::io::Read::take(&mut lecteur, 64 * 1024 * 1024).read_line(&mut ligne) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let Ok(v) = serde_json::from_str::<Value>(&ligne) else { continue };
            if let Some(n) = v.get("n").and_then(Value::as_u64) {
                let reponse = match v.get("err") {
                    Some(e) => Err(e.clone()),
                    None => Ok(v.get("ok").cloned().unwrap_or(Value::Null)),
                };
                if let Some(attente) = lu.attentes.lock().unwrap().remove(&n) {
                    let _ = attente.send(reponse);
                }
            } else if let (Some(evt), Some(data)) = (v.get("evt").and_then(Value::as_str), v.get("data")) {
                nouvelle(evt, data);
            }
        }
        // La connexion est tombée : les commandes en attente échouent.
        lu.vivant.store(false, Ordering::Relaxed);
        lu.attentes.lock().unwrap().clear();
        lu.canal.fermer();
        nouvelle("fin", &Value::Null);
    });
    l
}

/// Une commande pour une machine dédiée (les mêmes que pour ce PC) : `api.ts` l'envoie ici
/// quand le serveur est sur une machine.
#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn machine_appel(app: AppHandle, machine: String, commande: String, args: Value) -> Result<Value, Value> {
    crate::hote::async_runtime::spawn_blocking(move || {
        let etat = crate::hote::Manager::state::<crate::Etat>(&app);
        let m = lire(&etat.dossier).into_iter().find(|m| m.id == machine).ok_or_else(|| Value::String("Cette machine n'est plus reliée.".into()))?;
        let l = lien(&app, &m).map_err(Value::String)?;
        // Une création de serveur ou une sauvegarde peut durer : jusqu'à 30 minutes.
        let r = l.appeler(&commande, args, Duration::from_secs(30 * 60));
        // Nouveau code : ce PC le garde ; la machine coupe ses connexions et revient avec lui.
        if let (Ok(Value::String(code)), "changer_code") = (&r, commande.as_str()) {
            noter_nouveau_code(&app, &etat.dossier, &m.id, code);
        }
        if !l.vivant.load(Ordering::Relaxed) {
            prevenir(&app, &lire(&etat.dossier));
        }
        r
    })
    .await
    .map_err(|e| Value::String(e.to_string()))?
}

// ---- Envoyer des fichiers de ce PC vers la machine (mondes, plugins, mods, icône) ----
// Un seul fichier part tel quel ; plusieurs (ou un dossier, comme un monde) partent dans une archive .zip
// faite ici. Par morceaux de 1 Mo (base64, une ligne chacun), puis la machine les range (« importer_envoi »).

const MORCEAU: usize = 1024 * 1024;

/// Met les sources dans une archive .zip (un dossier garde son nom et tout son contenu).
fn emballer(sources: &[PathBuf], archive: &Path) -> Result<(), String> {
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;
    let erreur = |e: &dyn std::fmt::Display| format!("Préparation impossible ({e}).");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(archive).map_err(|e| erreur(&e))?);
    fn ajouter(zip: &mut zip::ZipWriter<std::fs::File>, chemin: &Path, nom: &str, profondeur: usize) -> Result<(), String> {
        let erreur = |e: &dyn std::fmt::Display| format!("Préparation impossible ({e}).");
        if profondeur > crate::garde::PROFONDEUR_MAX {
            return Err("Dossier trop profond.".into());
        }
        let meta = std::fs::symlink_metadata(chemin).map_err(|e| erreur(&e))?;
        if meta.is_dir() {
            zip.add_directory(format!("{nom}/"), SimpleFileOptions::default()).map_err(|e| erreur(&e))?;
            let mut enfants: Vec<_> = std::fs::read_dir(chemin).map_err(|e| erreur(&e))?.flatten().collect();
            enfants.sort_by_key(|e| e.file_name());
            for e in enfants {
                ajouter(zip, &e.path(), &format!("{nom}/{}", e.file_name().to_string_lossy()), profondeur + 1)?;
            }
        } else if meta.is_file() {
            // Les régions d'un monde sont déjà compressées : compression rapide.
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(1))
                .large_file(meta.len() >= u32::MAX as u64);
            zip.start_file(nom, options).map_err(|e| erreur(&e))?;
            let mut f = std::fs::File::open(chemin).map_err(|e| erreur(&e))?;
            std::io::copy(&mut f, zip).map_err(|e| erreur(&e))?;
        }
        Ok(())
    }
    for source in sources {
        let Some(nom) = source.file_name() else { continue };
        ajouter(&mut zip, source, &nom.to_string_lossy(), 0)?;
    }
    zip.finish().map_err(|e| erreur(&e))?.flush().map_err(|e| erreur(&e))?;
    Ok(())
}

/// Envoie un fichier par morceaux. `avance` reçoit le pourcentage.
fn envoyer_fichier(l: &Lien, envoi: &str, fichier: &Path, nom: &str, zip: bool, avance: &mut dyn FnMut(u8)) -> Result<(), Value> {
    use base64::Engine;
    let illisible = |e: std::io::Error| Value::String(format!("Fichier illisible ({e})."));
    let total = std::fs::metadata(fichier).map_err(illisible)?.len();
    let mut f = std::fs::File::open(fichier).map_err(illisible)?;
    let mut tampon = vec![0u8; MORCEAU];
    let mut position = 0u64;
    loop {
        // Remplit le morceau (une lecture peut en donner moins).
        let mut n = 0;
        while n < MORCEAU {
            let lu = std::io::Read::read(&mut f, &mut tampon[n..]).map_err(illisible)?;
            if lu == 0 {
                break;
            }
            n += lu;
        }
        let fin = position + n as u64 >= total;
        let donnees = base64::engine::general_purpose::STANDARD.encode(&tampon[..n]);
        let args = serde_json::json!({ "envoi": envoi, "nom": nom, "position": position, "donnees": donnees, "fin": fin, "zip": zip });
        // L'archive est ouverte à la fin, sur la machine : ça peut prendre un moment.
        l.appeler("recevoir_envoi", args, Duration::from_secs(if fin { 30 * 60 } else { 120 }))?;
        position += n as u64;
        avance(if total == 0 { 100 } else { (position * 100 / total) as u8 });
        if fin {
            return Ok(());
        }
    }
}

/// Envoie des fichiers de ce PC vers un serveur de la machine, puis les range :
/// `quoi` = « fichiers » (dans `dossier` du serveur), « monde » (`actuel` : il devient le monde joué) ou « icone ».
/// Renvoie la même chose que la commande de ce PC (nombre de fichiers, liste des mondes, rien).
#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn envoyer_a_machine(app: AppHandle, machine: String, id: String, sources: Vec<String>, quoi: String, dossier: Option<String>, actuel: Option<bool>) -> Result<Value, Value> {
    crate::hote::async_runtime::spawn_blocking(move || {
        let etat = crate::hote::Manager::state::<crate::Etat>(&app);
        let m = lire(&etat.dossier).into_iter().find(|m| m.id == machine).ok_or_else(|| Value::String("Cette machine n'est plus reliée.".into()))?;
        let l = lien(&app, &m).map_err(Value::String)?;
        let sources: Vec<PathBuf> = sources.into_iter().map(PathBuf::from).filter(|p| p.exists()).collect();
        if sources.is_empty() {
            return Err(Value::String("Rien à envoyer.".into()));
        }
        let mut hasard = [0u8; 8];
        getrandom::fill(&mut hasard).map_err(|_| Value::String("Ça n'a pas marché.".into()))?;
        let envoi = protocole::hexa(&hasard);
        let dire = |etape: &str, pourcent: u8| {
            let _ = app.emit("travail", serde_json::json!({ "id": id, "etape": etape, "pourcent": pourcent }));
        };
        // Un seul fichier : tel quel. Sinon (ou un dossier) : une archive faite ici.
        let seul = sources.len() == 1 && sources[0].is_file();
        let (fichier, nom, archive) = if seul {
            (sources[0].clone(), sources[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "fichier".into()), None)
        } else {
            dire("Préparation", 0);
            let archive = std::env::temp_dir().join(format!("bhost-envoi-{envoi}.zip"));
            emballer(&sources, &archive).map_err(Value::String)?;
            (archive.clone(), "envoi.zip".to_string(), Some(archive))
        };
        let resultat = envoyer_fichier(&l, &envoi, &fichier, &nom, archive.is_some(), &mut |p| dire("Envoi", p)).and_then(|_| {
            dire("Rangement", 100);
            l.appeler("importer_envoi", serde_json::json!({ "id": id, "envoi": envoi, "quoi": quoi, "dossier": dossier, "actuel": actuel }), Duration::from_secs(30 * 60))
        });
        if let Some(archive) = archive {
            let _ = std::fs::remove_file(archive);
        }
        resultat
    })
    .await
    .map_err(|e| Value::String(e.to_string()))?
}

/// Le code de la machine `id` a changé depuis ce PC : on le garde, puis on oublie la connexion (la machine la coupe).
fn noter_nouveau_code(app: &AppHandle, dossier: &Path, id: &str, code: &str) {
    let Some(code) = nettoyer_code(code) else { return };
    let _verrou = VERROU.lock().unwrap();
    let mut liste = lire(dossier);
    if let Some(m) = liste.iter_mut().find(|m| m.id == id) {
        m.code = code;
    }
    if ecrire(dossier, &liste).is_ok() {
        CODE_CHANGE.lock().unwrap().remove(id);
        if let Some(l) = LIENS.lock().unwrap().as_mut().and_then(|l| l.remove(id)) {
            l.vivant.store(false, Ordering::Relaxed);
            l.canal.fermer();
        }
        // La machine revient sur le relais dans quelques secondes : on attend 15 s avant de la rejoindre
        // (avant, le relais la connaît encore avec l'ancien code, et l'essai compterait comme un code faux).
        ESSAIS.lock().unwrap().get_or_insert_with(HashMap::new).insert(id.to_string(), Instant::now());
        prevenir(app, &liste);
    }
}

/// Relie une machine avec son code. Le relais est contacté hors du fil de l'interface.
#[cfg_attr(feature = "bureau", tauri::command)]
pub async fn lier_machine(app: AppHandle, code: String, etat: State<'_, crate::Etat>) -> Result<MachineVue, String> {
    let code = nettoyer_code(&code).ok_or("Ce code n'est pas valide. Vérifie-le.")?;
    let dossier = etat.dossier.clone();
    let id = match machine_de_test(&code) {
        Some(id) => id.to_string(),
        None => {
            let c = code.clone();
            crate::hote::async_runtime::spawn_blocking(move || demander_au_relais(&c)).await.map_err(|_| "Ça n'a pas marché.".to_string())??
        }
    };
    let _une_seule = VERROU.lock().unwrap();
    let mut liste = lire(&dossier);
    let m = ajouter(&mut liste, &id, &code);
    ecrire(&dossier, &liste)?;
    // Reliée à nouveau (nouveau code) : BHOST peut la rejoindre tout de suite.
    CODE_CHANGE.lock().unwrap().remove(&id);
    ESSAIS.lock().unwrap().get_or_insert_with(HashMap::new).remove(&id);
    prevenir(&app, &liste);
    Ok(vue(&m))
}

/// Retire une machine de BHOST (ses serveurs continuent de tourner dessus).
#[cfg_attr(feature = "bureau", tauri::command)]
pub fn delier_machine(app: AppHandle, id: String, etat: State<crate::Etat>) -> Result<(), String> {
    let _une_seule = VERROU.lock().unwrap();
    let mut liste = lire(&etat.dossier);
    liste.retain(|m| m.id != id);
    ecrire(&etat.dossier, &liste)?;
    if let Some(l) = LIENS.lock().unwrap().as_mut().and_then(|l| l.remove(&id)) {
        l.vivant.store(false, Ordering::Relaxed);
        l.canal.fermer();
    }
    let _ = app.emit("acces-machine", serde_json::json!({ "machine": id, "liste": [] }));
    prevenir(&app, &liste);
    Ok(())
}

/// Donne un autre nom à une machine (« Salon », « Grenier »…).
#[cfg_attr(feature = "bureau", tauri::command)]
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
        let l = vec![MachineLiee { id: "ab".repeat(8), nom: "Salon".into(), code: "K7M2Q9XR".into(), cle: Some("cd".repeat(32)), ip: Some("192.168.1.42".into()) }];
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
        let en_ligne = vue(&MachineLiee { id: TEST_EN_LIGNE.1.into(), nom: "Test".into(), code: "TESTTEST".into(), ..Default::default() });
        assert_eq!(en_ligne.etat, "en-ligne");
        assert!(en_ligne.temperature.is_some_and(|t| (54.0..=86.0).contains(&t)));
        let json = serde_json::to_string(&en_ligne).unwrap();
        assert!(json.contains("\"utiliseGo\":168.0") && json.contains("\"reserveeGo\":") && json.contains("\"ip\":\"192.168.1.42\""));
        assert_eq!(vue(&MachineLiee { id: TEST_HORS_LIGNE.1.into(), nom: "Test".into(), code: "TEST2222".into(), ..Default::default() }).etat, "hors-ligne");
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
    fn archive_d_envoi() {
        let d = std::env::temp_dir().join(format!("bhost-envoi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("monde/region")).unwrap();
        std::fs::write(d.join("monde/level.dat"), b"niveau").unwrap();
        std::fs::write(d.join("monde/region/r.0.0.mca"), vec![7u8; 100_000]).unwrap();
        std::fs::write(d.join("plugin.jar"), b"jar").unwrap();
        let archive = d.join("envoi.zip");
        emballer(&[d.join("monde"), d.join("plugin.jar")], &archive).unwrap();
        let sortie = d.join("sortie");
        crate::java::extraire(&archive, &sortie).unwrap();
        assert_eq!(std::fs::read(sortie.join("monde/level.dat")).unwrap(), b"niveau");
        assert_eq!(std::fs::read(sortie.join("monde/region/r.0.0.mca")).unwrap().len(), 100_000);
        assert_eq!(std::fs::read(sortie.join("plugin.jar")).unwrap(), b"jar");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Essai de bout en bout, à la main : un relais local, l'agent de BHOST OS, puis
    /// `BHOST_RELAIS=127.0.0.1:4499 ESSAI_CODE=… ESSAI_ID=… ESSAI_IP=127.0.0.1 [ESSAI_SERVEUR=id] cargo test -- --ignored pilotage_reel`
    #[test]
    #[ignore]
    fn pilotage_reel() {
        let var = |n: &str| std::env::var(n).ok();
        let m = MachineLiee { id: var("ESSAI_ID").unwrap(), code: var("ESSAI_CODE").unwrap(), nom: "Essai".into(), ..Default::default() };
        // 1. Par le relais (pas d'adresse connue).
        let (canal, lecteur, cle, direct) = connexion(&m).unwrap();
        assert!(!direct);
        let l = demarrer_lien(canal, lecteur, |_, _| {});
        let e = l.appeler("etat_machine", Value::Null, Duration::from_secs(10)).unwrap();
        assert!(e.get("memoire").is_some(), "{e}");
        // 2. En direct, avec la clé retenue.
        if let Some(ip) = var("ESSAI_IP") {
            let m2 = MachineLiee { ip: Some(ip), cle: Some(protocole::hexa(&cle)), ..m.clone() };
            let (canal, lecteur, _, direct) = connexion(&m2).unwrap();
            assert!(direct);
            let l2 = demarrer_lien(canal, lecteur, |_, _| {});
            assert!(l2.appeler("lister_serveurs", Value::Null, Duration::from_secs(10)).unwrap().is_array());
            // Une autre clé que celle retenue : refusée (même en direct).
            let faux = MachineLiee { cle: Some("ab".repeat(32)), ..m2.clone() };
            assert_eq!(connexion(&faux).err().as_deref(), Some(PAS_LA_MEME));
            // Un code faux : refusé par la machine.
            let mauvais = MachineLiee { code: "ZZZZZZZZ".into(), ..m2.clone() };
            assert_eq!(connexion(&mauvais).err().as_deref(), Some(CODE_A_CHANGE));
        }
        // 3. Envoyer des fichiers vers un serveur de la machine (un dossier et un fichier : une archive).
        if let Some(serveur) = var("ESSAI_SERVEUR") {
            let d = std::env::temp_dir().join(format!("bhost-essai-envoi-{}", std::process::id()));
            std::fs::create_dir_all(d.join("config")).unwrap();
            std::fs::write(d.join("config/reglages.yml"), "a: 1").unwrap();
            std::fs::write(d.join("gros.jar"), vec![3u8; 3 * MORCEAU + 17]).unwrap();
            let archive = d.join("envoi.zip");
            emballer(&[d.join("config"), d.join("gros.jar")], &archive).unwrap();
            let mut vus = Vec::new();
            envoyer_fichier(&l, "0123456789abcdef", &archive, "envoi.zip", true, &mut |p| vus.push(p)).unwrap();
            assert_eq!(vus.last(), Some(&100));
            let r = l.appeler("importer_envoi", serde_json::json!({ "id": serveur, "envoi": "0123456789abcdef", "quoi": "fichiers", "dossier": "" }), Duration::from_secs(60)).unwrap();
            assert_eq!(r, 2);
            // Un seul fichier : tel quel.
            envoyer_fichier(&l, "fedcba9876543210", &d.join("gros.jar"), "gros.jar", false, &mut |_| {}).unwrap();
            let r = l.appeler("importer_envoi", serde_json::json!({ "id": serveur, "envoi": "fedcba9876543210", "quoi": "fichiers", "dossier": "" }), Duration::from_secs(60)).unwrap();
            assert_eq!(r, 1);
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn installeur_vps_a_jour() {
        // Le script pour VPS embarque les mêmes scripts que BHOST OS (os/fichiers).
        let vps = include_str!("../../os/installer-vps.sh").replace("\r\n", "\n");
        for script in [include_str!("../../os/fichiers/usr/local/sbin/bhost-systeme"), include_str!("../../os/fichiers/usr/local/sbin/bhost-performances")] {
            assert!(vps.contains(&script.replace("\r\n", "\n")), "os/installer-vps.sh : recopie les scripts de os/fichiers");
        }
        // La commande donnée par l'interface pointe sur ce script.
        assert!(include_str!("../../src/api.ts").contains("releases/latest/download/installer-vps.sh"));
        assert!(vps.contains("releases/latest/download/installer-vps.sh"));
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
