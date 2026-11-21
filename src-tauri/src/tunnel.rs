// Le relais BHOST, côté PC : quand la box ne s'ouvre pas (UPnP coupé, CGNAT, 5G…), BHOST se connecte
// au relais (relais/ dans ce dépôt) et les amis passent par lui : « survie-k3f9.bhost.app ».
// Un fil par serveur : il se présente (nom + clé ed25519), garde la connexion (PING toutes les 20 s),
// et à chaque « JOUEUR » ouvre une connexion « DONNEES » qu'il branche sur le serveur du PC.

#[allow(dead_code)]
#[path = "../../relais/src/protocole.rs"]
pub(crate) mod protocole;

use ed25519_dalek::{Signer, SigningKey};
use protocole::{DeBhost, DuRelais};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// L'adresse du relais (pour les essais : variable d'environnement BHOST_RELAIS, ex. « 127.0.0.1:4443 »).
const RELAIS: &str = "relais.bhost.app:4443";
/// Les adresses données aux amis : « nom.bhost.app ».
const DOMAINE: &str = "bhost.app";
const PING: Duration = Duration::from_secs(20);
const SILENCE: Duration = Duration::from_secs(70);
const DELAI: Duration = Duration::from_secs(10);
/// Joueurs branchés en même temps, tous serveurs confondus.
const MAX_JOUEURS: usize = 200;

/// Où en est le relais pour un serveur.
#[derive(Clone, Debug, PartialEq)]
pub enum Etat {
    /// Premier essai en cours.
    Connexion,
    /// En ligne : l'adresse à donner aux amis.
    EnLigne(String),
    /// Coupé, on se reconnecte (il était en ligne).
    Reconnexion,
    /// Le relais ne répond pas (ou refuse) : on réessaie en silence.
    Echec,
}

struct Tunnel {
    port: u16,
    nom_serveur: String,
    arret: Arc<AtomicBool>,
    etat: Arc<Mutex<Etat>>,
    /// La connexion de contrôle : on la coupe pour arrêter le fil sans attendre.
    prise: Arc<Mutex<Option<TcpStream>>>,
}

#[derive(Default)]
pub struct Tunnels {
    liste: Mutex<HashMap<String, Tunnel>>,
    /// %APPDATA%\BHOST (la clé et les noms y sont gardés).
    dossier: OnceLock<PathBuf>,
    /// Prévenu quand un état change (le fil de acces.rs refait les vues).
    changement: Mutex<Option<Sender<bool>>>,
    joueurs: Arc<AtomicUsize>,
    /// Dit si un serveur tourne (ou si BHOST écoute pour le réveiller) : sinon, on ne branche personne sur son port.
    actif: OnceLock<Actif>,
}

/// « Ce serveur peut-il recevoir un joueur ? » (id du serveur).
pub type Actif = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// La clé de ce PC (créée au premier besoin) : elle prouve au relais que les noms sont à nous.
/// Elle n'est créée que si le fichier n'existe pas : une clé illisible n'est jamais remplacée
/// (sinon toutes les adresses des serveurs changeraient).
pub(crate) fn cle(dossier: &Path) -> io::Result<SigningKey> {
    use std::io::Write;
    let fichier = dossier.join("relais-cle.txt");
    match std::fs::read_to_string(&fichier) {
        Ok(t) => {
            let t = t.trim_start_matches('\u{feff}').trim();
            let octets = protocole::depuis_hexa(t).and_then(|o| <[u8; 32]>::try_from(o).ok()).ok_or_else(|| io::Error::other("relais-cle.txt abîmé"))?;
            Ok(SigningKey::from_bytes(&octets))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let mut graine = [0u8; 32];
            getrandom::fill(&mut graine).map_err(|_| io::Error::other("pas de hasard"))?;
            std::fs::create_dir_all(dossier)?;
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&fichier)?;
            f.write_all(protocole::hexa(&graine).as_bytes())?;
            f.sync_all()?;
            Ok(SigningKey::from_bytes(&graine))
        }
        // Fichier bloqué un instant (antivirus…) : on réessaiera, sans rien écraser.
        Err(e) => Err(e),
    }
}

/// « survie-k3f9m2xa » : le nom du serveur + 8 signes au hasard (pour qu'il soit libre et difficile à deviner).
fn nouveau_nom(nom_serveur: &str) -> String {
    let mut h = [0u8; 8];
    let _ = getrandom::fill(&mut h);
    let signes = b"abcdefghijkmnpqrstuvwxyz23456789";
    let fin: String = h.iter().map(|o| signes[*o as usize % signes.len()] as char).collect();
    let nom = format!("{}-{fin}", protocole::slug(nom_serveur));
    // Un nom de serveur que le relais refuse (« BHOST officiel »…) : un nom neutre.
    if protocole::nom_valide(&nom) { nom } else { format!("serveur-{fin}") }
}

/// Les noms déjà pris par ce PC (relais.json : id du serveur → nom), pour garder la même adresse.
fn noms(dossier: &Path) -> HashMap<String, String> {
    std::fs::read(dossier.join("relais.json")).ok().and_then(|o| serde_json::from_slice(&o).ok()).unwrap_or_default()
}

fn noter_nom(dossier: &Path, id: &str, nom: &str) {
    let mut n = noms(dossier);
    n.insert(id.to_string(), nom.to_string());
    if let Ok(json) = serde_json::to_vec_pretty(&n) {
        let _ = crate::garde::ecrire_atomique(&dossier.join("relais.json"), &json);
    }
}

impl Tunnels {
    /// Qui peut recevoir des joueurs (donné par lib.rs).
    pub fn regler_actif(&self, actif: Actif) {
        let _ = self.actif.set(actif);
    }

    pub fn regler(&self, dossier: PathBuf, changement: Sender<bool>) {
        let _ = self.dossier.set(dossier);
        *self.changement.lock().unwrap() = Some(changement);
    }

    pub fn etat(&self, id: &str) -> Option<Etat> {
        self.liste.lock().unwrap().get(id).map(|t| t.etat.lock().unwrap().clone())
    }

    /// Ce serveur passe par le relais (port du serveur sur le PC, et son nom pour l'adresse).
    pub fn vouloir(&self, id: &str, nom_serveur: &str, port: u16) {
        let mut liste = self.liste.lock().unwrap();
        if liste.get(id).is_some_and(|t| t.port == port) {
            return;
        }
        if let Some(ancien) = liste.remove(id) {
            ancien.arreter();
        }
        let Some(dossier) = self.dossier.get().cloned() else { return };
        let tunnel = Tunnel { port, nom_serveur: nom_serveur.to_string(), arret: Arc::default(), etat: Arc::new(Mutex::new(Etat::Connexion)), prise: Arc::default() };
        let fil = Fil {
            id: id.to_string(),
            nom_serveur: nom_serveur.to_string(),
            port,
            dossier,
            arret: tunnel.arret.clone(),
            etat: tunnel.etat.clone(),
            prise: tunnel.prise.clone(),
            changement: self.changement.lock().unwrap().clone(),
            joueurs: self.joueurs.clone(),
            actif: self.actif.get().cloned(),
        };
        // Pas de fil possible (mémoire pleine) : ce serveur ne passe pas par le relais, BHOST ne plante pas.
        if std::thread::Builder::new().spawn(move || fil.tourner()).is_err() {
            *tunnel.etat.lock().unwrap() = Etat::Echec;
        }
        liste.insert(id.to_string(), tunnel);
    }

    /// Ce serveur ne passe plus par le relais.
    #[cfg(test)]
    pub fn lacher(&self, id: &str) {
        if let Some(t) = self.liste.lock().unwrap().remove(id) {
            t.arreter();
        }
    }

    /// Garde seulement ces serveurs.
    pub fn garder(&self, ids: &[&str]) {
        let mut liste = self.liste.lock().unwrap();
        let partis: Vec<String> = liste.keys().filter(|k| !ids.contains(&k.as_str())).cloned().collect();
        for id in partis {
            if let Some(t) = liste.remove(&id) {
                t.arreter();
            }
        }
    }

    /// « Ce mot est-il libre pour l'adresse de ce serveur ? » (Accueil, case Adresse).
    pub fn adresse_libre(&self, id: &str, mot: &str) -> Libre {
        let Some(dossier) = self.dossier.get() else { return Libre::Injoignable };
        if noms(dossier).get(id).is_some_and(|n| n == mot) {
            return Libre::Actuelle;
        }
        if !protocole::forme_valide(mot) {
            return Libre::Invalide;
        }
        let cle = match cle(dossier) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[BHOST] Clé du relais illisible : {e}");
                return Libre::Injoignable;
            }
        };
        // Le premier numéro libre : survie2, survie3…
        let proposer = || {
            let base = mot.trim_end_matches(|c: char| c.is_ascii_digit());
            let depart = mot[base.len()..].parse::<u32>().unwrap_or(1).max(1);
            (depart + 1..depart + 7)
                .map(|n| protocole::nom_numero(base, n))
                .filter(|n| protocole::nom_valide(n))
                .find(|n| matches!(demander_libre(&cle, n), Ok(DuRelais::Ok { .. })))
        };
        // Un mot gardé pour BHOST (« test », « panel »…) : comme s'il était pris.
        if !protocole::nom_valide(mot) {
            return Libre::Pris(proposer());
        }
        match demander_libre(&cle, mot) {
            Ok(DuRelais::Ok { .. }) => Libre::Libre,
            Ok(DuRelais::Non { raison }) if raison == "pris" => Libre::Pris(proposer()),
            Ok(DuRelais::Non { raison }) if raison == "nom" => Libre::Invalide,
            Ok(DuRelais::Non { raison }) if raison == "trop" => Libre::Trop,
            autre => {
                // Dans le terminal de BHOST : pourquoi le relais n'a pas répondu (adresse, refus, délai…).
                eprintln!("[BHOST] Relais {:?} muet pour « {mot} » : {autre:?}", adresse_relais().ok());
                Libre::Injoignable
            }
        }
    }

    /// Change l'adresse d'un serveur (« survie » → survie.bhost.app). Le relais reprend tout de suite ;
    /// l'ancien nom est rendu (les autres pourront le prendre).
    pub fn changer_adresse(&self, id: &str, mot: &str) -> Result<(), &'static str> {
        let dossier = self.dossier.get().ok_or("Le relais n'est pas prêt.")?.clone();
        match self.adresse_libre(id, mot) {
            Libre::Actuelle => return Ok(()),
            Libre::Libre => {}
            Libre::Pris(_) => return Err("Déjà pris."),
            Libre::Invalide => return Err("Lettres, chiffres et tirets (3 à 32)."),
            Libre::Trop => return Err("Trop d'essais. Réessaie dans 10 minutes."),
            Libre::Injoignable => return Err("Le relais ne répond pas. Réessaie."),
        }
        let ancien = noms(&dossier).get(id).cloned();
        noter_nom(&dossier, id, mot);
        // Le fil du serveur repart avec le nouveau nom.
        let reprise = self.liste.lock().unwrap().remove(id).map(|t| {
            t.arreter();
            (t.nom_serveur.clone(), t.port)
        });
        if let Some((nom_serveur, port)) = reprise {
            self.vouloir(id, &nom_serveur, port);
        }
        // L'ancien nom est rendu quand le relais a vu partir l'ancienne connexion.
        if let Some(ancien) = ancien.filter(|a| a != mot) {
            std::thread::spawn(move || {
                let Ok(cle) = cle(&dossier) else { return };
                for _ in 0..5 {
                    std::thread::sleep(Duration::from_secs(3));
                    if lacher(&cle, &ancien).unwrap_or(false) {
                        return;
                    }
                }
            });
        }
        Ok(())
    }

    pub fn tout_arreter(&self) {
        for (_, t) in self.liste.lock().unwrap().drain() {
            t.arreter();
        }
    }
}

impl Tunnel {
    fn arreter(&self) {
        self.arret.store(true, Ordering::Relaxed);
        if let Some(p) = self.prise.lock().unwrap().take() {
            let _ = p.shutdown(Shutdown::Both);
        }
    }
}

/// Ce que le fil d'un serveur emporte.
struct Fil {
    id: String,
    nom_serveur: String,
    port: u16,
    dossier: PathBuf,
    arret: Arc<AtomicBool>,
    etat: Arc<Mutex<Etat>>,
    prise: Arc<Mutex<Option<TcpStream>>>,
    changement: Option<Sender<bool>>,
    joueurs: Arc<AtomicUsize>,
    actif: Option<Actif>,
}

/// Comment une session s'est finie.
enum Fin {
    /// Le nom est à quelqu'un d'autre : on en prend un nouveau.
    Pris,
    /// Coupée (après avoir été en ligne, ou pas).
    Coupee,
}

/// Les lignes d'une connexion, lues par morceaux (avec un délai de lecture sur la prise).
#[derive(Default)]
struct Lignes {
    tampon: Vec<u8>,
}

impl Lignes {
    /// Une ligne si elle est là ; None si le délai de lecture passe sans ligne complète.
    /// Une seule lecture par appel : un relais qui envoie un octet à la fois ne fait pas oublier les délais.
    fn lire(&mut self, flux: &mut TcpStream) -> io::Result<Option<String>> {
        let ligne = |t: &mut Vec<u8>| t.iter().position(|&o| o == b'\n').map(|i| String::from_utf8_lossy(&t.drain(..=i).collect::<Vec<u8>>()).into_owned());
        if let Some(l) = ligne(&mut self.tampon) {
            return Ok(Some(l));
        }
        if self.tampon.len() > 4096 {
            return Err(io::Error::other("ligne trop longue"));
        }
        let mut morceau = [0u8; 512];
        match flux.read(&mut morceau) {
            Ok(0) => Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                self.tampon.extend_from_slice(&morceau[..n]);
                Ok(ligne(&mut self.tampon))
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Attend une ligne au plus `delai`.
    fn attendre(&mut self, flux: &mut TcpStream, delai: Duration) -> io::Result<String> {
        let debut = Instant::now();
        loop {
            if let Some(l) = self.lire(flux)? {
                return Ok(l);
            }
            if debut.elapsed() > delai {
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
    }
}

pub(crate) fn adresse_relais() -> io::Result<SocketAddr> {
    // BHOST_RELAIS : un autre relais, seulement pour les essais (jamais dans la version publiée).
    #[cfg(debug_assertions)]
    let texte = std::env::var("BHOST_RELAIS").unwrap_or_else(|_| RELAIS.to_string());
    #[cfg(not(debug_assertions))]
    let texte = RELAIS.to_string();
    texte.to_socket_addrs()?.next().ok_or_else(|| io::ErrorKind::NotFound.into())
}

pub(crate) fn connecter(adresse: SocketAddr) -> io::Result<TcpStream> {
    let flux = TcpStream::connect_timeout(&adresse, Duration::from_secs(5))?;
    let _ = flux.set_nodelay(true);
    Ok(flux)
}

/// La réponse à « ce nom est-il libre ? ».
#[derive(Debug, PartialEq)]
pub enum Libre {
    /// C'est déjà l'adresse de ce serveur.
    Actuelle,
    Libre,
    /// Pris par quelqu'un d'autre ; le premier numéro libre (« survie2 »), si on l'a trouvé.
    Pris(Option<String>),
    /// Réservé, trop court, ou des caractères refusés.
    Invalide,
    /// Le relais limite les questions.
    Trop,
    Injoignable,
}

/// Une question au relais (une connexion, une ligne, une réponse).
fn question(m: &DeBhost) -> io::Result<(TcpStream, Lignes, DuRelais)> {
    let mut flux = connecter(adresse_relais()?)?;
    flux.set_read_timeout(Some(Duration::from_secs(2)))?;
    flux.write_all(format!("BHOST {}\n{}", protocole::VERSION, protocole::ecrire_de_bhost(m)).as_bytes())?;
    let mut lignes = Lignes::default();
    let reponse = protocole::lire_du_relais(&lignes.attendre(&mut flux, DELAI)?).ok_or_else(|| io::Error::other("réponse illisible"))?;
    Ok((flux, lignes, reponse))
}

/// « LIBRE <nom> <clé> » : libre (ou déjà à nous), ou NON pris / nom / trop.
fn demander_libre(cle: &SigningKey, nom: &str) -> io::Result<DuRelais> {
    question(&DeBhost::Libre { nom: nom.to_string(), cle: protocole::hexa(cle.verifying_key().as_bytes()) }).map(|(_, _, r)| r)
}

/// « LACHER » : rend un nom au relais (signé). Vrai si c'est fait.
fn lacher(cle: &SigningKey, nom: &str) -> io::Result<bool> {
    let (mut flux, mut lignes, reponse) = question(&DeBhost::Lacher { nom: nom.to_string(), cle: protocole::hexa(cle.verifying_key().as_bytes()) })?;
    let DuRelais::Defi { defi } = reponse else { return Ok(false) };
    let signature = protocole::hexa(&cle.sign(protocole::a_signer_lacher(nom, &defi).as_bytes()).to_bytes());
    flux.write_all(protocole::ecrire_de_bhost(&DeBhost::Signe { signature }).as_bytes())?;
    Ok(matches!(protocole::lire_du_relais(&lignes.attendre(&mut flux, DELAI)?), Some(DuRelais::Ok { .. })))
}

impl Fil {
    fn changer(&self, e: Etat) {
        let mut etat = self.etat.lock().unwrap();
        if *etat != e {
            *etat = e;
            drop(etat);
            if let Some(c) = &self.changement {
                let _ = c.send(false);
            }
        }
    }

    fn tourner(self) {
        // La clé : si elle est bloquée un instant, on réessaie (jamais de nouvelle clé à la place).
        let cle = loop {
            match cle(&self.dossier) {
                Ok(c) => break c,
                Err(_) => {
                    self.changer(Etat::Echec);
                    for _ in 0..120 {
                        if self.arret.load(Ordering::Relaxed) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
            }
        };
        // Le nom gardé, sinon celui du serveur : « survie », puis « survie2 », « survie3 »… s'il est pris.
        let mut nom = noms(&self.dossier).get(&self.id).cloned().unwrap_or_else(|| protocole::nom_numero(&protocole::slug(&self.nom_serveur), 1));
        let base = nom.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
        let mut numero = 1;
        let mut attente = Duration::from_secs(2);
        let mut pris_de_suite = 0;
        while !self.arret.load(Ordering::Relaxed) {
            match self.session(&cle, &nom) {
                // Nom pris : le numéro suivant, mais pas en boucle sans fin (au-delà : un nom au hasard).
                Ok(Fin::Pris) if pris_de_suite < 25 => {
                    pris_de_suite += 1;
                    numero += 1;
                    nom = protocole::nom_numero(&base, numero);
                    continue;
                }
                Ok(Fin::Pris) => nom = nouveau_nom(&self.nom_serveur),
                Ok(Fin::Coupee) => {
                    pris_de_suite = 0;
                    attente = Duration::from_secs(2)
                }
                Err(_) => {}
            }
            if self.arret.load(Ordering::Relaxed) {
                break;
            }
            if matches!(*self.etat.lock().unwrap(), Etat::EnLigne(_)) {
                attente = Duration::from_secs(2);
            }
            // Il était en ligne : « reconnexion » ; sinon : échec (l'interface garde le message de la box).
            let suivant = if matches!(*self.etat.lock().unwrap(), Etat::EnLigne(_) | Etat::Reconnexion) && attente <= Duration::from_secs(8) {
                Etat::Reconnexion
            } else {
                Etat::Echec
            };
            self.changer(suivant);
            let fin = Instant::now() + attente;
            while Instant::now() < fin && !self.arret.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
            }
            attente = (attente * 2).min(Duration::from_secs(60));
        }
    }

    /// Une connexion au relais : présentation, puis les joueurs jusqu'à la coupure.
    fn session(&self, cle: &SigningKey, nom: &str) -> io::Result<Fin> {
        let relais = adresse_relais()?;
        let mut flux = connecter(relais)?;
        *self.prise.lock().unwrap() = flux.try_clone().ok();
        if self.arret.load(Ordering::Relaxed) {
            return Ok(Fin::Coupee);
        }
        flux.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut lignes = Lignes::default();
        let bonjour = protocole::ecrire_de_bhost(&DeBhost::Bonjour { nom: nom.to_string(), cle: protocole::hexa(cle.verifying_key().as_bytes()) });
        flux.write_all(format!("BHOST {}\n{bonjour}", protocole::VERSION).as_bytes())?;
        let defi = match protocole::lire_du_relais(&lignes.attendre(&mut flux, DELAI)?) {
            Some(DuRelais::Defi { defi }) => defi,
            Some(DuRelais::Non { raison }) if raison == "pris" => return Ok(Fin::Pris),
            _ => return Err(io::Error::other("refusé")),
        };
        let signature = protocole::hexa(&cle.sign(protocole::a_signer(nom, &defi).as_bytes()).to_bytes());
        flux.write_all(protocole::ecrire_de_bhost(&DeBhost::Signe { signature }).as_bytes())?;
        let adresse = match protocole::lire_du_relais(&lignes.attendre(&mut flux, DELAI)?) {
            // Seulement l'adresse attendue : un faux relais ne peut pas faire afficher autre chose.
            Some(DuRelais::Ok { adresse }) if adresse == format!("{nom}.{DOMAINE}") || (cfg!(debug_assertions) && adresse.starts_with(&format!("{nom}."))) => adresse,
            Some(DuRelais::Ok { .. }) => return Err(io::Error::other("adresse inattendue")),
            Some(DuRelais::Non { raison }) if raison == "pris" => return Ok(Fin::Pris),
            _ => return Err(io::Error::other("refusé")),
        };
        noter_nom(&self.dossier, &self.id, nom);
        self.changer(Etat::EnLigne(adresse));

        let mut dernier_ping = Instant::now();
        let mut dernier_mot = Instant::now();
        while !self.arret.load(Ordering::Relaxed) {
            if dernier_ping.elapsed() >= PING {
                flux.write_all(protocole::ecrire_de_bhost(&DeBhost::Ping).as_bytes())?;
                dernier_ping = Instant::now();
            }
            if dernier_mot.elapsed() >= SILENCE {
                return Ok(Fin::Coupee);
            }
            let Some(ligne) = lignes.lire(&mut flux).or_else(|e| if self.arret.load(Ordering::Relaxed) { Ok(None) } else { Err(e) })? else { continue };
            dernier_mot = Instant::now();
            match protocole::lire_du_relais(&ligne) {
                Some(DuRelais::Pong) => {}
                Some(DuRelais::Joueur { id, jeton }) => self.brancher(relais, id, jeton),
                _ => return Ok(Fin::Coupee),
            }
        }
        Ok(Fin::Coupee)
    }

    /// Un joueur arrive : une connexion vers le relais, une vers le serveur du PC, et on recolle.
    fn brancher(&self, relais: SocketAddr, id: u64, jeton: String) {
        // Serveur arrêté (et pas en veille) : personne n'est branché sur son port (un autre programme pourrait y écouter).
        if self.actif.as_ref().is_some_and(|actif| !actif(&self.id)) {
            return;
        }
        if self.joueurs.fetch_add(1, Ordering::Relaxed) >= MAX_JOUEURS {
            self.joueurs.fetch_sub(1, Ordering::Relaxed);
            return;
        }
        let port = self.port;
        let joueurs = self.joueurs.clone();
        let rendre = joueurs.clone();
        let fil = std::thread::Builder::new().spawn(move || {
            let essai = || -> io::Result<()> {
                let mut distant = connecter(relais)?;
                distant.write_all(format!("BHOST {}\n{}", protocole::VERSION, protocole::ecrire_de_bhost(&DeBhost::Donnees { id, jeton })).as_bytes())?;
                // Le serveur (ou le réveil de reveil.rs, qui écoute sur le même port quand il dort).
                let local = connecter(SocketAddr::from(([127, 0, 0, 1], port)))?;
                relier(distant, local)
            };
            let _ = essai();
            joueurs.fetch_sub(1, Ordering::Relaxed);
        });
        if fil.is_err() {
            rendre.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Copie dans les deux sens jusqu'à ce qu'un des deux bouts coupe.
fn relier(a: TcpStream, b: TcpStream) -> io::Result<()> {
    let (mut a_lu, mut b_ecrit) = (a.try_clone()?, b.try_clone()?);
    let aller = std::thread::Builder::new().spawn(move || {
        let _ = io::copy(&mut a_lu, &mut b_ecrit);
        let _ = b_ecrit.shutdown(Shutdown::Write);
    })?;
    let (mut b_lu, mut a_ecrit) = (b, a);
    let _ = io::copy(&mut b_lu, &mut a_ecrit);
    let _ = a_ecrit.shutdown(Shutdown::Write);
    let _ = aller.join();
    let _ = b_lu.shutdown(Shutdown::Both);
    let _ = a_ecrit.shutdown(Shutdown::Both);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_et_cle_gardes_sur_le_disque() {
        let dossier = std::env::temp_dir().join(format!("bhost-tunnel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        let a = cle(&dossier).unwrap();
        let b = cle(&dossier).unwrap();
        assert_eq!(a.to_bytes(), b.to_bytes(), "la même clé à chaque lancement");
        let n = nouveau_nom("Survie entre amis !");
        assert!(n.starts_with("survie-entre-amis-") && protocole::nom_valide(&n), "{n}");
        assert_ne!(n, nouveau_nom("Survie entre amis !"));
        noter_nom(&dossier, "s1", &n);
        assert_eq!(noms(&dossier).get("s1"), Some(&n));
        let _ = std::fs::remove_dir_all(&dossier);
    }

    /// Le vrai relais (crate relais/) sur la machine, un faux serveur Minecraft, et un joueur.
    #[test]
    fn de_bout_en_bout() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let local: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (joueurs, agents, _relais) = rt
            .block_on(bhost_relais::demarrer(bhost_relais::Reglages { domaine: "bhost.test".into(), joueurs: local, agents: local, registre: None }))
            .unwrap();
        // SAFETY : seul ce test lit BHOST_RELAIS.
        unsafe { std::env::set_var("BHOST_RELAIS", agents.to_string()) };
        // Le faux serveur Minecraft renvoie tout.
        let serveur = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = serveur.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for f in serveur.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut r = f.try_clone().unwrap();
                    let mut w = f;
                    let _ = io::copy(&mut r, &mut w);
                });
            }
        });
        let dossier = std::env::temp_dir().join(format!("bhost-bout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        let tunnels = Tunnels::default();
        let (envoi, changements) = std::sync::mpsc::channel();
        tunnels.regler(dossier.clone(), envoi);
        tunnels.vouloir("s1", "Survie", port);
        let adresse = loop {
            changements.recv_timeout(Duration::from_secs(10)).expect("le relais n'a pas répondu");
            if let Some(Etat::EnLigne(a)) = tunnels.etat("s1") {
                break a;
            }
        };
        // Le nom du serveur, tout simplement.
        assert_eq!(adresse, "survie.bhost.test");
        // Un joueur tape l'adresse.
        let mut poignee = vec![0x00];
        protocole::ecrire_varint(&mut poignee, 769);
        protocole::ecrire_varint(&mut poignee, adresse.len() as i32);
        poignee.extend_from_slice(adresse.as_bytes());
        poignee.extend(25565u16.to_be_bytes());
        poignee.push(2);
        let mut envoi = Vec::new();
        protocole::ecrire_varint(&mut envoi, poignee.len() as i32);
        envoi.extend(poignee);
        envoi.extend_from_slice(b"KOTORO_YT arrive");
        let mut j = TcpStream::connect(joueurs).unwrap();
        j.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        j.write_all(&envoi).unwrap();
        let mut recu = vec![0u8; envoi.len()];
        j.read_exact(&mut recu).unwrap();
        assert_eq!(recu, envoi);
        // Même nom au prochain lancement.
        let nom = adresse.trim_end_matches(".bhost.test").to_string();
        assert_eq!(noms(&dossier).get("s1"), Some(&nom));
        // Un autre PC (une autre clé) avec un serveur « Survie » : survie2.
        let dossier2 = std::env::temp_dir().join(format!("bhost-bout2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier2);
        let autres = Tunnels::default();
        let (envoi2, changements2) = std::sync::mpsc::channel();
        autres.regler(dossier2.clone(), envoi2);
        autres.vouloir("t1", "Survie", port);
        let attendre = |t: &Tunnels, c: &std::sync::mpsc::Receiver<bool>, id: &str, voulu: &str| loop {
            c.recv_timeout(Duration::from_secs(10)).expect("le relais n'a pas répondu");
            if t.etat(id) == Some(Etat::EnLigne(voulu.to_string())) {
                break;
            }
        };
        attendre(&autres, &changements2, "t1", "survie2.bhost.test");
        // La case Adresse : le sien, pris (avec une proposition), libre, refusé.
        assert_eq!(tunnels.adresse_libre("s1", "survie"), Libre::Actuelle);
        assert_eq!(tunnels.adresse_libre("s1", "survie2"), Libre::Pris(Some("survie3".into())));
        assert_eq!(tunnels.adresse_libre("s1", "creatif"), Libre::Libre);
        assert_eq!(tunnels.adresse_libre("s1", "panel"), Libre::Pris(Some("panel2".into())));
        assert_eq!(tunnels.adresse_libre("s1", "a--b"), Libre::Invalide);
        assert_eq!(tunnels.changer_adresse("s1", "survie2"), Err("Déjà pris."));
        // Nouvelle adresse : en ligne tout de suite, et « survie » est rendu aux autres.
        tunnels.changer_adresse("s1", "creatif").unwrap();
        attendre(&tunnels, &changements, "s1", "creatif.bhost.test");
        let mut rendu = false;
        for _ in 0..40 {
            if autres.adresse_libre("t1", "survie") == Libre::Libre {
                rendu = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(rendu, "l'ancien nom n'a pas été rendu");
        autres.tout_arreter();
        let _ = std::fs::remove_dir_all(&dossier2);
        let adresse = "creatif.bhost.test".to_string();
        let envoi = {
            let mut poignee = vec![0x00];
            protocole::ecrire_varint(&mut poignee, 769);
            protocole::ecrire_varint(&mut poignee, adresse.len() as i32);
            poignee.extend_from_slice(adresse.as_bytes());
            poignee.extend(25565u16.to_be_bytes());
            poignee.push(2);
            let mut e = Vec::new();
            protocole::ecrire_varint(&mut e, poignee.len() as i32);
            e.extend(poignee);
            e.extend_from_slice(b"KOTORO_YT arrive");
            e
        };
        // On coupe : le relais dit « éteint ».
        tunnels.lacher("s1");
        std::thread::sleep(Duration::from_millis(300));
        let mut j = TcpStream::connect(joueurs).unwrap();
        j.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        j.write_all(&envoi[..envoi.len() - b"KOTORO_YT arrive".len()]).unwrap();
        let mut recu = Vec::new();
        let _ = j.read_to_end(&mut recu);
        assert!(String::from_utf8_lossy(&recu).contains("éteint"));
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[test]
    fn lit_les_lignes_par_morceaux() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = l.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut f, _) = l.accept().unwrap();
            f.write_all(b"PO").unwrap();
            std::thread::sleep(Duration::from_millis(50));
            f.write_all(b"NG\nJOUEUR 3 ").unwrap();
            f.write_all(format!("{}\n", "ab".repeat(16)).as_bytes()).unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let mut f = TcpStream::connect(adresse).unwrap();
        f.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let mut lignes = Lignes::default();
        assert_eq!(protocole::lire_du_relais(&lignes.attendre(&mut f, DELAI).unwrap()), Some(DuRelais::Pong));
        assert_eq!(protocole::lire_du_relais(&lignes.attendre(&mut f, DELAI).unwrap()), Some(DuRelais::Joueur { id: 3, jeton: "ab".repeat(16) }));
        // Plus rien : None au délai, puis la fin.
        assert!(matches!(lignes.lire(&mut f), Ok(None) | Err(_)));
    }
}
