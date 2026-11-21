// Le relais BHOST : il reçoit les joueurs sur le port de Minecraft et les envoie au bon PC.
// Voir protocole.rs pour le dialogue. Ici : les connexions, le registre des noms, les limites.
//
// Deux ports :
//   - joueurs (25565) : le jeu s'y connecte ; on lit le nom dans la poignée de main ;
//   - agents (4443) : BHOST s'y connecte (une connexion de contrôle par serveur, puis une par joueur).

pub mod protocole;

use protocole::{DeBhost, DuRelais, Poignee};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

/// Le temps laissé pour la poignée de main d'un joueur ou les premières lignes de BHOST.
const DELAI_DEBUT: Duration = Duration::from_secs(10);
/// Le temps laissé à BHOST pour ouvrir la connexion d'un joueur.
const DELAI_DONNEES: Duration = Duration::from_secs(10);
/// Sans nouvelles de BHOST pendant ce temps (il envoie PING toutes les 20 s) : il est parti.
const DELAI_SILENCE: Duration = Duration::from_secs(70);
/// Un nom pas utilisé depuis 180 jours est libéré.
const OUBLI: u64 = 180 * 24 * 3600;
/// Connexions en même temps par adresse IP.
const MAX_JOUEURS_PAR_IP: u32 = 30;
const MAX_AGENTS_PAR_IP: u32 = 100;
/// Serveurs en ligne en même temps par adresse IP (connexions de contrôle).
const MAX_SERVEURS_PAR_IP: u32 = 10;
/// Nouveaux noms par adresse IP et par heure.
const MAX_NOUVEAUX_PAR_HEURE: u32 = 5;
/// Une ligne de BHOST ne dépasse jamais ça (« BONJOUR » + nom + clé ≈ 110 caractères).
const LIGNE_MAX: usize = 300;
/// Connexions en même temps, tous ports confondus (sous la limite de fichiers ouverts du système).
const CONNEXIONS_MAX: usize = 20_000;
/// Noms enregistrés au plus.
const NOMS_MAX: usize = 200_000;
/// Machines dédiées : codes faux permis par adresse IP avant d'attendre.
const ESSAIS_MAX: u32 = 5;
/// … et l'attente après le 5ᵉ code faux.
const ATTENTE_ESSAIS: Duration = Duration::from_secs(15 * 60);

pub struct Reglages {
    /// « bhost.app » : les serveurs sont « nom.bhost.app ».
    pub domaine: String,
    pub joueurs: SocketAddr,
    pub agents: SocketAddr,
    /// Le registre des noms (JSON). None : en mémoire seulement (tests).
    pub registre: Option<PathBuf>,
}

/// Un nom pris : sa clé publique (hexa) et la dernière fois qu'on l'a vu (secondes depuis 1970).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Proprio {
    cle: String,
    vu: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct Registre {
    noms: HashMap<String, Proprio>,
}

/// Un BHOST en ligne : on lui parle par ce canal (sa tâche écrit les lignes).
struct Agent {
    session: u64,
    envoi: mpsc::Sender<DuRelais>,
    /// Retiré ou remplacé : ce bout tombe, et la session le voit (elle coupe la connexion).
    _fin: oneshot::Sender<()>,
}

/// Une machine dédiée (BHOST OS) en ligne, rangée par son code.
struct MachineEnLigne {
    id: String,
    session: u64,
    /// Remplacée par une connexion plus récente : ce bout tombe, et la session le voit.
    _fin: oneshot::Sender<()>,
}

/// Un joueur qui attend que BHOST ouvre sa connexion.
struct Attente {
    jeton: String,
    nom: String,
    remise: oneshot::Sender<TcpStream>,
}

pub struct Relais {
    domaine: String,
    fichier: Option<PathBuf>,
    registre: Mutex<Registre>,
    /// Le registre a changé depuis le dernier enregistrement.
    modifie: AtomicBool,
    /// Une seule écriture du fichier à la fois (minuterie et arrêt).
    ecriture: Mutex<()>,
    /// Les places de connexion qui restent.
    places: Arc<tokio::sync::Semaphore>,
    agents: Mutex<HashMap<String, Agent>>,
    attentes: Mutex<HashMap<u64, Attente>>,
    joueurs_ip: Mutex<HashMap<IpAddr, u32>>,
    agents_ip: Mutex<HashMap<IpAddr, u32>>,
    serveurs_ip: Mutex<HashMap<IpAddr, u32>>,
    nouveaux_ip: Mutex<HashMap<IpAddr, (Instant, u32)>>,
    machines: Mutex<HashMap<String, MachineEnLigne>>,
    /// Codes de machine faux, par adresse IP : (depuis quand, combien).
    essais: Mutex<HashMap<IpAddr, (Instant, u32)>>,
    compteur: AtomicU64,
}

fn maintenant() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn hasard<const N: usize>() -> [u8; N] {
    let mut o = [0u8; N];
    getrandom::fill(&mut o).expect("pas de hasard sur ce système");
    o
}

/// Compte une connexion d'une adresse IP ; rendue toute seule quand le jeton disparaît.
struct Place<'a> {
    table: &'a Mutex<HashMap<IpAddr, u32>>,
    ip: IpAddr,
}

/// En IPv6, une personne a tout un bloc /64 : on compte par bloc, pas par adresse.
fn bloc(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(std::net::Ipv6Addr::from(u128::from(v6) & !((1u128 << 64) - 1))),
        },
        v4 => v4,
    }
}

impl<'a> Place<'a> {
    fn prendre(table: &'a Mutex<HashMap<IpAddr, u32>>, ip: IpAddr, max: u32) -> Option<Place<'a>> {
        let ip = bloc(ip);
        let mut t = table.lock().unwrap();
        let n = t.entry(ip).or_insert(0);
        if *n >= max {
            return None;
        }
        *n += 1;
        Some(Place { table, ip })
    }
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        let mut t = self.table.lock().unwrap();
        if let Some(n) = t.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                t.remove(&self.ip);
            }
        }
    }
}

impl Relais {
    fn nouveau(domaine: &str, fichier: Option<PathBuf>) -> io::Result<Relais> {
        // Un registre abîmé ne doit jamais devenir un registre vide (tous les noms seraient libres) :
        // le relais refuse de démarrer, il faut le réparer (ou reprendre noms.bak).
        let mut registre: Registre = match fichier.as_ref().map(std::fs::read) {
            None => Registre::default(),
            Some(Err(e)) if e.kind() == io::ErrorKind::NotFound => Registre::default(),
            Some(Err(e)) => return Err(e),
            Some(Ok(o)) => serde_json::from_slice(&o).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
        };
        let limite = maintenant().saturating_sub(OUBLI);
        registre.noms.retain(|_, p| p.vu >= limite);
        Ok(Relais {
            domaine: domaine.trim_matches('.').to_ascii_lowercase(),
            fichier,
            registre: Mutex::new(registre),
            modifie: AtomicBool::new(false),
            ecriture: Mutex::new(()),
            places: Arc::new(tokio::sync::Semaphore::new(CONNEXIONS_MAX)),
            agents: Mutex::default(),
            attentes: Mutex::default(),
            joueurs_ip: Mutex::default(),
            agents_ip: Mutex::default(),
            serveurs_ip: Mutex::default(),
            nouveaux_ip: Mutex::default(),
            machines: Mutex::default(),
            essais: Mutex::default(),
            compteur: AtomicU64::new(1),
        })
    }

    /// Écrit le registre s'il a changé (fichier temporaire puis renommage : jamais à moitié écrit).
    pub fn enregistrer(&self) {
        let Some(fichier) = &self.fichier else { return };
        let _une_seule = self.ecriture.lock().unwrap();
        if !self.modifie.swap(false, Ordering::AcqRel) {
            return;
        }
        self.nouveaux_ip.lock().unwrap().retain(|_, (debut, _)| debut.elapsed() < Duration::from_secs(3600));
        let limite = maintenant().saturating_sub(OUBLI);
        // Les noms en ligne en ce moment ne sont jamais oubliés.
        let en_ligne: Vec<String> = self.agents.lock().unwrap().keys().cloned().collect();
        let noms = {
            let mut r = self.registre.lock().unwrap();
            for nom in &en_ligne {
                if let Some(p) = r.noms.get_mut(nom) {
                    p.vu = maintenant();
                }
            }
            r.noms.retain(|_, p| p.vu >= limite);
            r.noms.clone()
        }; // le registre est rendu ici : la mise en JSON et l'écriture ne bloquent personne
        let json = serde_json::to_vec(&Registre { noms }).unwrap_or_default();
        if ecrire_atomique(fichier, &json).is_err() {
            self.modifie.store(true, Ordering::Relaxed); // on réessaiera dans une minute
        }
    }

    pub fn en_ligne(&self) -> usize {
        self.agents.lock().unwrap().len()
    }

    /// Le nom est à cette clé (nouveau ou revu). `ip` : pour limiter les nouveaux noms.
    fn noter(&self, nom: &str, cle: &str, ip: IpAddr) -> Result<(), &'static str> {
        let mut r = self.registre.lock().unwrap();
        match r.noms.get_mut(nom) {
            Some(p) if p.cle != cle => return Err("pris"),
            Some(p) => p.vu = maintenant(),
            None => {
                let mut n = self.nouveaux_ip.lock().unwrap();
                let (debut, compte) = n.entry(bloc(ip)).or_insert((Instant::now(), 0));
                if debut.elapsed() > Duration::from_secs(3600) {
                    *debut = Instant::now();
                    *compte = 0;
                }
                if *compte >= MAX_NOUVEAUX_PAR_HEURE || r.noms.len() >= NOMS_MAX {
                    return Err("trop");
                }
                *compte += 1;
                r.noms.insert(nom.to_string(), Proprio { cle: cle.to_string(), vu: maintenant() });
            }
        }
        self.modifie.store(true, Ordering::Relaxed);
        Ok(())
    }
}

/// Écrit un fichier pour de bon : fichier temporaire synchronisé sur le disque, copie de l'ancien en .bak, renommage.
fn ecrire_atomique(fichier: &std::path::Path, octets: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let tmp = fichier.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(octets)?;
    f.sync_all()?;
    drop(f);
    if fichier.exists() {
        std::fs::copy(fichier, fichier.with_extension("bak"))?;
    }
    std::fs::rename(&tmp, fichier)?;
    if let Some(d) = fichier.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::File::open(d)?.sync_all()?;
    }
    Ok(())
}

/// Accepte les connexions d'un port, dans la limite des places, et lance `servir` pour chacune.
fn ecouter<F, Fut>(ecoute: TcpListener, rel: Arc<Relais>, servir: F)
where
    F: Fn(Arc<Relais>, TcpStream, IpAddr) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            let (flux, adresse) = match ecoute.accept().await {
                Ok(x) => x,
                // Plus de descripteurs libres, etc. : on souffle un peu au lieu de tourner à vide.
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            // Relais plein : la connexion est fermée tout de suite.
            let Ok(permis) = rel.places.clone().try_acquire_owned() else { continue };
            let _ = flux.set_nodelay(true);
            let tache = servir(rel.clone(), flux, adresse.ip());
            tokio::spawn(async move {
                tache.await;
                drop(permis);
            });
        }
    });
}

/// Ouvre les deux ports et lance le relais. Rend les vraies adresses (utile avec le port 0).
pub async fn demarrer(r: Reglages) -> io::Result<(SocketAddr, SocketAddr, Arc<Relais>)> {
    let relais = Arc::new(Relais::nouveau(&r.domaine, r.registre)?);
    let joueurs = TcpListener::bind(r.joueurs).await?;
    let agents = TcpListener::bind(r.agents).await?;
    let adresses = (joueurs.local_addr()?, agents.local_addr()?);

    ecouter(joueurs, relais.clone(), |rel, flux, ip| async move {
        let _ = joueur(&rel, flux, ip).await;
    });
    ecouter(agents, relais.clone(), |rel, flux, ip| async move {
        let _ = agent(&rel, flux, ip).await;
    });
    // Le registre est écrit chaque minute s'il a changé.
    let rel = relais.clone();
    tokio::spawn(async move {
        let mut tic = tokio::time::interval(Duration::from_secs(60));
        loop {
            tic.tick().await;
            let r = rel.clone();
            let _ = tokio::task::spawn_blocking(move || r.enregistrer()).await;
        }
    });
    Ok((adresses.0, adresses.1, relais))
}

// ---------------------------------------------------------------------------------------------
// Les joueurs
// ---------------------------------------------------------------------------------------------

async fn joueur(rel: &Relais, mut flux: TcpStream, ip: IpAddr) -> io::Result<()> {
    let Some(_place) = Place::prendre(&rel.joueurs_ip, ip, MAX_JOUEURS_PAR_IP) else { return Ok(()) };
    // La poignée de main (et souvent la suite, collée derrière).
    let mut lu = Vec::with_capacity(512);
    // Un délai pour toute la poignée de main (pas pour chaque morceau) : un visiteur trop lent est coupé.
    let fin = tokio::time::Instant::now() + DELAI_DEBUT;
    let (hote, protocole, suite, taille) = loop {
        match protocole::lire_poignee(&lu) {
            Poignee::Lue { hote, protocole, suite, taille } => break (hote, protocole, suite, taille),
            Poignee::Incomplete => {}
            Poignee::Invalide | Poignee::Ancienne => return Ok(()),
        }
        let mut morceau = [0u8; 1024];
        let n = tokio::time::timeout_at(fin, flux.read(&mut morceau)).await.map_err(|_| io::ErrorKind::TimedOut)??;
        if n == 0 {
            return Ok(());
        }
        lu.extend_from_slice(&morceau[..n]);
        if lu.len() > protocole::POIGNEE_MAX + 2048 {
            return Ok(());
        }
    };
    let Some(nom) = protocole::sous_domaine(&hote, &rel.domaine) else {
        return absent(&mut flux, lu, taille, protocole, suite, "Adresse inconnue. Demande-la à ton ami.").await;
    };
    // Le BHOST de ce serveur est-il là ?
    let envoi = rel.agents.lock().unwrap().get(&nom).map(|a| a.envoi.clone());
    let Some(envoi) = envoi else {
        return absent(&mut flux, lu, taille, protocole, suite, "Ce serveur est éteint.").await;
    };
    let id = rel.compteur.fetch_add(1, Ordering::Relaxed);
    let jeton = protocole::hexa(&hasard::<16>());
    let (remise, reception) = oneshot::channel();
    rel.attentes.lock().unwrap().insert(id, Attente { jeton: jeton.clone(), nom: nom.clone(), remise });
    let donnees = if matches!(timeout(DELAI_DONNEES, envoi.send(DuRelais::Joueur { id, jeton })).await, Ok(Ok(()))) {
        timeout(DELAI_DONNEES, reception).await.ok().and_then(Result::ok)
    } else {
        None
    };
    rel.attentes.lock().unwrap().remove(&id);
    let Some(mut donnees) = donnees else {
        return absent(&mut flux, lu, taille, protocole, suite, "Ce serveur ne répond pas.").await;
    };
    // On recolle les deux bouts : d'abord ce qu'on a déjà lu, puis tout le reste dans les deux sens.
    donnees.write_all(&lu).await?;
    tokio::io::copy_bidirectional(&mut flux, &mut donnees).await?;
    Ok(())
}

/// Le serveur n'est pas là : on répond nous-mêmes (liste des serveurs, ou écran « Déconnecté »).
async fn absent(flux: &mut TcpStream, lu: Vec<u8>, taille: usize, protocole: i32, suite: i32, message: &str) -> io::Result<()> {
    if suite != 1 {
        flux.write_all(&protocole::refus_connexion(message)).await?;
        return flux.shutdown().await;
    }
    flux.write_all(&protocole::statut_absent(protocole, message)).await?;
    // Puis le « ping » : on renvoie les mêmes 8 octets (le jeu affiche le délai).
    let mut reste = lu[taille..].to_vec();
    let _ = timeout(DELAI_DEBUT, async {
        loop {
            // La demande de statut (paquet vide n° 0) passe avant le ping.
            if reste.starts_with(&[1, 0]) {
                reste.drain(..2);
            }
            if let Some(p) = protocole::lire_ping(&reste) {
                flux.write_all(&protocole::pong(&p)).await?;
                return Ok::<_, io::Error>(());
            }
            if reste.len() > 64 {
                return Ok(());
            }
            let mut morceau = [0u8; 64];
            let n = flux.read(&mut morceau).await?;
            if n == 0 {
                return Ok(());
            }
            reste.extend_from_slice(&morceau[..n]);
        }
    })
    .await;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// BHOST (les agents)
// ---------------------------------------------------------------------------------------------

/// Lit une ligne octet par octet (pour ne rien prendre de ce qui suit), avec un délai.
async fn lire_ligne<L: AsyncReadExt + Unpin>(flux: &mut L, delai: Duration) -> io::Result<Option<String>> {
    timeout(delai, async {
        let mut ligne = Vec::new();
        loop {
            let mut o = [0u8; 1];
            if flux.read(&mut o).await? == 0 {
                return Ok(None);
            }
            if o[0] == b'\n' {
                return Ok(String::from_utf8(ligne).ok());
            }
            if ligne.len() >= LIGNE_MAX {
                return Ok(None);
            }
            ligne.push(o[0]);
        }
    })
    .await
    .unwrap_or(Ok(None))
}

async fn ecrire<E: AsyncWriteExt + Unpin>(flux: &mut E, m: &DuRelais) -> io::Result<()> {
    flux.write_all(protocole::ecrire_du_relais(m).as_bytes()).await
}

async fn agent(rel: &Relais, mut flux: TcpStream, ip: IpAddr) -> io::Result<()> {
    let Some(_place) = Place::prendre(&rel.agents_ip, ip, MAX_AGENTS_PAR_IP) else { return Ok(()) };
    let Some(premiere) = lire_ligne(&mut flux, DELAI_DEBUT).await? else { return Ok(()) };
    if premiere.trim_end() != format!("BHOST {}", protocole::VERSION) {
        if premiere.starts_with("BHOST ") {
            ecrire(&mut flux, &DuRelais::Non { raison: "version".into() }).await?;
        }
        return Ok(());
    }
    let Some(ligne) = lire_ligne(&mut flux, DELAI_DEBUT).await? else { return Ok(()) };
    match protocole::lire_de_bhost(&ligne) {
        Some(DeBhost::Donnees { id, jeton }) => {
            // La connexion d'un joueur : on la remet à sa tâche, qui fait le reste.
            // Le jeton sert une seule fois : bon ou mauvais, l'attente est retirée.
            let attente = rel.attentes.lock().unwrap().remove(&id);
            if let Some(a) = attente
                && egal_temps_constant(a.jeton.as_bytes(), jeton.as_bytes())
            {
                let _ = a.remise.send(flux);
            }
            Ok(())
        }
        Some(DeBhost::Bonjour { nom, cle }) => controle(rel, flux, ip, nom, cle).await,
        Some(DeBhost::Machine { cle, code }) => machine(rel, flux, ip, cle, code).await,
        Some(DeBhost::Lier { code }) => {
            let reponse = rel.lier(&code, ip);
            ecrire(&mut flux, &reponse).await
        }
        _ => Ok(()),
    }
}

/// La connexion de contrôle d'un serveur : preuve de la clé, puis « JOUEUR » à chaque arrivée.
async fn controle(rel: &Relais, mut flux: TcpStream, ip: IpAddr, nom: String, cle: String) -> io::Result<()> {
    // D'abord la preuve de la clé : on ne dit rien d'un nom (pris ou libre) à qui ne l'a pas donnée.
    let defi = protocole::hexa(&hasard::<16>());
    ecrire(&mut flux, &DuRelais::Defi { defi: defi.clone() }).await?;
    let Some(ligne) = lire_ligne(&mut flux, DELAI_DEBUT).await? else { return Ok(()) };
    let Some(DeBhost::Signe { signature }) = protocole::lire_de_bhost(&ligne) else { return Ok(()) };
    if !signature_valide(&cle, &signature, &protocole::a_signer(&nom, &defi)) {
        return ecrire(&mut flux, &DuRelais::Non { raison: "signature".into() }).await;
    }
    // La place « serveur » n'est prise qu'après une signature valide.
    let Some(_place) = Place::prendre(&rel.serveurs_ip, ip, MAX_SERVEURS_PAR_IP) else {
        return ecrire(&mut flux, &DuRelais::Non { raison: "trop".into() }).await;
    };
    if let Err(raison) = rel.noter(&nom, &cle, ip) {
        return ecrire(&mut flux, &DuRelais::Non { raison: raison.into() }).await;
    }
    // Ce serveur est en ligne (s'il l'était déjà, l'ancienne connexion est remplacée).
    let session = rel.compteur.fetch_add(1, Ordering::Relaxed);
    let (envoi, mut reception) = mpsc::channel::<DuRelais>(64);
    let (fin, mut remplace) = oneshot::channel::<()>();
    rel.agents.lock().unwrap().insert(nom.clone(), Agent { session, envoi: envoi.clone(), _fin: fin });
    ecrire(&mut flux, &DuRelais::Ok { adresse: format!("{nom}.{}", rel.domaine) }).await?;

    let (mut lecture, mut ecriture) = flux.into_split();
    // Les lignes de BHOST : PING (on répond PONG par le même canal), sinon on coupe. Rien pendant 70 s : parti.
    let mut lecteur = tokio::spawn(async move {
        while let Ok(Some(ligne)) = lire_ligne(&mut lecture, DELAI_SILENCE).await {
            if protocole::lire_de_bhost(&ligne) != Some(DeBhost::Ping) || envoi.send(DuRelais::Pong).await.is_err() {
                break;
            }
        }
    });
    let resultat: io::Result<()> = async {
        loop {
            tokio::select! {
                m = reception.recv() => match m {
                    // Un BHOST qui ne lit plus ne bloque pas le relais.
                    Some(m) => timeout(DELAI_DONNEES, ecrire(&mut ecriture, &m)).await.map_err(|_| io::Error::from(io::ErrorKind::TimedOut))??,
                    None => return Ok(()),
                },
                // BHOST s'est tu ou a coupé.
                _ = &mut lecteur => return Ok(()),
                // Une connexion plus récente a pris le nom.
                _ = &mut remplace => return Ok(()),
            }
        }
    }
    .await;
    lecteur.abort();
    // On ne retire que sa propre session (une plus récente a pu prendre la place).
    let mut agents = rel.agents.lock().unwrap();
    if agents.get(&nom).is_some_and(|a| a.session == session) {
        agents.remove(&nom);
        drop(agents);
        // Les joueurs qui attendaient ce serveur n'attendent pas 10 s pour rien.
        rel.attentes.lock().unwrap().retain(|_, a| a.nom != nom);
    }
    resultat
}

impl Relais {
    /// Un PC tape le code d'une machine. Après 5 codes faux, l'adresse IP attend 15 minutes
    /// (même avec le bon code) : deviner un code de 8 caractères devient impossible.
    fn lier(&self, code: &str, ip: IpAddr) -> DuRelais {
        let ip = bloc(ip);
        let mut essais = self.essais.lock().unwrap();
        essais.retain(|_, (depuis, _)| depuis.elapsed() < ATTENTE_ESSAIS);
        if essais.get(&ip).is_some_and(|(_, n)| *n >= ESSAIS_MAX) {
            return DuRelais::Non { raison: "bloque".into() };
        }
        if let Some(m) = self.machines.lock().unwrap().get(code) {
            return DuRelais::Liee { id: m.id.clone() };
        }
        let (depuis, n) = essais.entry(ip).or_insert((Instant::now(), 0));
        *n += 1;
        if *n >= ESSAIS_MAX {
            *depuis = Instant::now(); // l'attente part du dernier essai
        }
        DuRelais::Non { raison: "code".into() }
    }
}

/// La connexion d'une machine dédiée : preuve de la clé, puis elle reste joignable par son code.
async fn machine(rel: &Relais, mut flux: TcpStream, ip: IpAddr, cle: String, code: String) -> io::Result<()> {
    let defi = protocole::hexa(&hasard::<16>());
    ecrire(&mut flux, &DuRelais::Defi { defi: defi.clone() }).await?;
    let Some(ligne) = lire_ligne(&mut flux, DELAI_DEBUT).await? else { return Ok(()) };
    let Some(DeBhost::Signe { signature }) = protocole::lire_de_bhost(&ligne) else { return Ok(()) };
    if !signature_valide(&cle, &signature, &protocole::a_signer_machine(&code, &defi)) {
        return ecrire(&mut flux, &DuRelais::Non { raison: "signature".into() }).await;
    }
    let Some(_place) = Place::prendre(&rel.serveurs_ip, ip, MAX_SERVEURS_PAR_IP) else {
        return ecrire(&mut flux, &DuRelais::Non { raison: "trop".into() }).await;
    };
    let id = protocole::id_machine(&cle);
    let session = rel.compteur.fetch_add(1, Ordering::Relaxed);
    let (fin, mut remplace) = oneshot::channel::<()>();
    // Deux machines avec le même code (très rare) : la 2ᵉ doit en tirer un autre.
    let pris = {
        let mut machines = rel.machines.lock().unwrap();
        let pris = machines.get(&code).is_some_and(|m| m.id != id);
        if !pris {
            machines.insert(code.clone(), MachineEnLigne { id: id.clone(), session, _fin: fin });
        }
        pris
    };
    if pris {
        return ecrire(&mut flux, &DuRelais::Non { raison: "pris".into() }).await;
    }
    ecrire(&mut flux, &DuRelais::Ok { adresse: id }).await?;

    let (mut lecture, mut ecriture) = flux.into_split();
    let resultat: io::Result<()> = async {
        loop {
            tokio::select! {
                ligne = lire_ligne(&mut lecture, DELAI_SILENCE) => match ligne? {
                    Some(l) if protocole::lire_de_bhost(&l) == Some(DeBhost::Ping) => ecrire(&mut ecriture, &DuRelais::Pong).await?,
                    _ => return Ok(()),
                },
                _ = &mut remplace => return Ok(()),
            }
        }
    }
    .await;
    let mut machines = rel.machines.lock().unwrap();
    if machines.get(&code).is_some_and(|m| m.session == session) {
        machines.remove(&code);
    }
    resultat
}

/// Compare deux jetons sans que le temps de réponse dise où ils diffèrent.
fn egal_temps_constant(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn signature_valide(cle: &str, signature: &str, message: &str) -> bool {
    let (Some(cle), Some(signature)) = (protocole::depuis_hexa(cle), protocole::depuis_hexa(signature)) else { return false };
    let (Ok(cle), Ok(signature)) = (<[u8; 32]>::try_from(cle), <[u8; 64]>::try_from(signature)) else { return false };
    let Ok(cle) = ed25519_dalek::VerifyingKey::from_bytes(&cle) else { return false };
    cle.verify_strict(message.as_bytes(), &ed25519_dalek::Signature::from_bytes(&signature)).is_ok()
}
