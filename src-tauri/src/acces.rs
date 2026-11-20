// Paramètres › Général › Ouvert à tes amis : BHOST ouvre le port du serveur sur la box (UPnP),
// sans que l'utilisateur touche aux réglages de sa box. Un fil s'en occupe : il ouvre, renouvelle
// toutes les 20 minutes, suit l'adresse de la box, et referme quand on coupe ou quand BHOST se ferme.
// Le réveil à la connexion (reveil.rs) écoute sur le même port : il marche aussi depuis Internet.

use igd_next::{AddAnyPortError, AddPortError, Gateway, PortMappingProtocol, SearchOptions};
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use crate::tunnel::{self, Tunnels};
use std::time::{Duration, Instant};

/// Ce que l'interface montre pour un serveur « Ouvert à tes amis ».
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Acces {
    pub id: String,
    /// « ouverture », « ouvert » ou « erreur ».
    pub etat: &'static str,
    /// L'adresse à donner aux amis (« 82.64.1.2 », ou « 82.64.1.2:25570 » si le port n'est pas 25565).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adresse: Option<String>,
    /// La phrase d'erreur.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Joueurs Bedrock : « 82.64.1.2 · 19132 » (l'adresse et le port se tapent dans deux cases du jeu).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bedrock: Option<String>,
    /// Joueurs Bedrock : la phrase d'erreur (le port Bedrock n'a pas pu être ouvert).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bedrock_message: Option<String>,
}

/// Un serveur à ouvrir, lu dans la liste.
#[derive(Clone, Debug)]
pub struct Voulu {
    pub id: String,
    /// Le nom du serveur (pour l'adresse sur le relais : « survie-k3f9.bhost.app »).
    pub nom: String,
    pub port: u16,
    /// Le réglage server-ip de server.properties (souvent vide).
    pub server_ip: String,
    /// Joueurs Bedrock autorisés : leur port (ouvert en UDP, et en TCP pour la connexion NetherNet de Geyser).
    pub bedrock: Option<u16>,
}

/// Un port sur la box : (UDP ?, port du serveur).
type Cle = (bool, u16);

fn protocole(udp: bool) -> PortMappingProtocol {
    if udp { PortMappingProtocol::UDP } else { PortMappingProtocol::TCP }
}

/// Un port ouvert sur la box.
#[derive(Clone, Debug, PartialEq)]
struct Ouvert {
    externe: u16,
    local: SocketAddrV4,
    /// Durée du bail en secondes (0 = sans fin : certaines box n'acceptent que ça).
    bail: u32,
    renouvele: Instant,
}

/// Ce que BHOST garde en mémoire pour l'accès depuis Internet.
#[derive(Default)]
pub struct EtatAcces {
    vues: Mutex<HashMap<String, Acces>>,
    /// (UDP ?, port du serveur) → port ouvert sur la box.
    ouverts: Mutex<HashMap<Cle, Ouvert>>,
    /// La box trouvée, et quand.
    box_trouvee: Mutex<Option<(Gateway, Instant)>>,
    /// Pour réveiller le fil tout de suite : true = tout refaire (interrupteur touché), false = le relais a changé.
    reveil: Mutex<Option<Sender<bool>>>,
    /// Le relais BHOST, quand la box ne s'ouvre pas (tunnel.rs).
    pub tunnels: Tunnels,
    /// Le dernier passage côté box, et ce qui était voulu (pour y ajouter le relais sans tout refaire).
    box_vues: Mutex<Vec<Acces>>,
    voulus: Mutex<Vec<Voulu>>,
    /// %APPDATA%\BHOST\ports-ouverts.json : les ports ouverts, notés pour les refermer
    /// même après un plantage ou une coupure de courant (box à ouvertures sans fin).
    fichier: OnceLock<std::path::PathBuf>,
}

/// Un port noté sur le disque.
#[derive(serde::Serialize, serde::Deserialize)]
struct Note {
    udp: bool,
    port: u16,
    externe: u16,
    local: SocketAddrV4,
    bail: u32,
}

/// L'ouverture `externe` sur la box est-elle bien à nous (vers `local`, avec notre description) ?
/// Si la box ne sait pas lister ses ouvertures, on ne peut pas vérifier : on la croit à nous.
fn est_a_nous(g: &Gateway, udp: bool, externe: u16, local: SocketAddrV4) -> bool {
    for i in 0..512 {
        match g.get_generic_port_mapping_entry(i) {
            Ok(e) if e.external_port == externe && e.protocol == protocole(udp) => {
                return e.internal_client == local.ip().to_string() && e.internal_port == local.port() && e.port_mapping_description.starts_with("BHOST");
            }
            Ok(_) => {}
            Err(igd_next::GetGenericPortMappingEntryError::SpecifiedArrayIndexInvalid) => return false, // fin de la liste : plus là
            Err(_) => return i == 0,
        }
    }
    false
}

/// Retire une ouverture, seulement si elle est à nous (jamais celle d'un autre appareil).
fn retirer(g: &Gateway, udp: bool, o: &Ouvert) {
    if est_a_nous(g, udp, o.externe, o.local) {
        let _ = g.remove_port(protocole(udp), o.externe);
    }
}

/// La passerelle par défaut du PC (vers Internet), d'après Windows.
#[cfg(windows)]
fn passerelle_par_defaut() -> Option<Ipv4Addr> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetBestRoute, MIB_IPFORWARDROW};
    let mut route: MIB_IPFORWARDROW = unsafe { std::mem::zeroed() };
    // 1.1.1.1, en ordre réseau (comme Windows l'attend).
    let destination = u32::from_ne_bytes([1, 1, 1, 1]);
    // SAFETY : `route` est une structure valide, remplie par Windows.
    let code = unsafe { GetBestRoute(destination, 0, &mut route) };
    if code != 0 {
        return None;
    }
    let saut = Ipv4Addr::from(route.dwForwardNextHop.to_ne_bytes());
    (!saut.is_unspecified()).then_some(saut)
}

#[cfg(not(windows))]
fn passerelle_par_defaut() -> Option<Ipv4Addr> {
    None
}

pub const RENOUVELER: Duration = Duration::from_secs(20 * 60);
const BAIL: u32 = 60 * 60;
const CHERCHER_BOX: Duration = Duration::from_secs(4);
const GARDER_BOX: Duration = Duration::from_secs(10 * 60);
const PORT_MINECRAFT: u16 = 25565;

pub const PAS_DE_BOX: &str = "Ta box ne répond pas. Active l'UPnP dans ses réglages.";
pub const REFUS: &str = "Ta box refuse d'ouvrir le port. Active l'UPnP dans ses réglages.";
pub const PARTAGEE: &str = "Ta box partage son adresse avec d'autres (CGNAT). Pas possible pour l'instant.";
pub const DOUBLE: &str = "Deux box à la suite : la première bloque. Pas possible pour l'instant.";
pub const SERVER_IP: &str = "Le réglage server-ip bloque l'accès. Vide-le dans Fichiers.";
pub const PANNE: &str = "Ta box n'a pas répondu. Nouvel essai dans 1 min.";
pub const BEDROCK_RELAIS: &str = "Pas encore possible par le relais BHOST.";

/// Ce qu'est l'adresse que la box a sur Internet.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Genre {
    /// Une vraie adresse : les amis peuvent venir.
    Publique,
    /// Une adresse de réseau privé : une autre box est devant celle-ci.
    Privee,
    /// 100.64.0.0/10 : l'opérateur partage l'adresse entre plusieurs clients (CGNAT).
    Partagee,
}

pub fn genre(ip: Ipv4Addr) -> Genre {
    let o = ip.octets();
    if o[0] == 100 && (64..128).contains(&o[1]) {
        Genre::Partagee
    } else if ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
        Genre::Privee
    } else {
        Genre::Publique
    }
}

/// L'adresse à donner aux amis : sans le port quand c'est celui de Minecraft.
pub fn adresse(ip: Ipv4Addr, port: u16) -> String {
    if port == PORT_MINECRAFT { ip.to_string() } else { format!("{ip}:{port}") }
}

/// Pour les joueurs Bedrock : l'adresse et le port, qu'ils tapent dans deux cases (« 82.64.1.2 · 19132 »).
pub fn adresse_bedrock(ip: Ipv4Addr, port: u16) -> String {
    format!("{ip} · {port}")
}

/// server-ip rempli avec une autre adresse que celle du PC : le serveur n'écoute pas là où la box envoie.
#[allow(clippy::unnecessary_map_or)]
pub fn server_ip_bloque(valeur: &str, locale: Ipv4Addr) -> bool {
    let v = valeur.trim();
    !v.is_empty() && v != "0.0.0.0" && v.parse::<Ipv4Addr>().map_or(true, |ip| ip != locale)
}

fn message_ajout(e: &AddPortError) -> &'static str {
    match e {
        AddPortError::ActionNotAuthorized => REFUS,
        _ => PANNE,
    }
}

fn message_ajout_libre(e: &AddAnyPortError) -> &'static str {
    match e {
        AddAnyPortError::ActionNotAuthorized => REFUS,
        AddAnyPortError::NoPortsAvailable | AddAnyPortError::ExternalPortInUse => "Le port est déjà pris sur ta box. Change le port du serveur.",
        _ => PANNE,
    }
}

impl EtatAcces {
    pub fn vues(&self) -> Vec<Acces> {
        let mut liste: Vec<Acces> = self.vues.lock().unwrap().values().cloned().collect();
        liste.sort_by(|a, b| a.id.cmp(&b.id));
        liste
    }

    /// Réveille le fil (après l'interrupteur). `ouvert` : on montre « Ouverture… » tout de suite.
    pub fn reveiller(&self, id: &str, ouvert: bool) -> Vec<Acces> {
        {
            let mut vues = self.vues.lock().unwrap();
            if ouvert {
                vues.insert(id.to_string(), Acces { id: id.to_string(), etat: "ouverture", adresse: None, message: None, bedrock: None, bedrock_message: None });
            } else {
                vues.remove(id);
            }
        }
        if let Some(envoi) = self.reveil.lock().unwrap().as_ref() {
            let _ = envoi.send(true);
        }
        self.vues()
    }

    /// La box, déjà trouvée il y a moins de 10 minutes, ou cherchée maintenant.
    fn trouver_box(&self) -> Option<Gateway> {
        if let Some((g, quand)) = self.box_trouvee.lock().unwrap().as_ref()
            && quand.elapsed() < GARDER_BOX
        {
            return Some(g.clone());
        }
        let mut options = SearchOptions::default();
        options.timeout = Some(CHERCHER_BOX);
        options.single_search_timeout = Some(CHERCHER_BOX);
        // Avec plusieurs cartes réseau (VirtualBox, WSL…), on cherche depuis celle qui va sur Internet.
        if let Some(ip) = ip_vers(SocketAddr::from(([1, 1, 1, 1], 80))) {
            options.bind_addr = SocketAddr::new(IpAddr::V4(ip), 0);
        }
        let trouvee = igd_next::search_gateway(options).ok().filter(|g| {
            // Seulement la vraie box (la passerelle du PC) : pas un autre appareil du réseau qui se ferait passer pour elle.
            match (passerelle_par_defaut(), g.addr.ip()) {
                (Some(defaut), IpAddr::V4(ip)) => ip == defaut,
                (Some(_), IpAddr::V6(_)) => false,
                (None, _) => true,
            }
        });
        *self.box_trouvee.lock().unwrap() = trouvee.clone().map(|g| (g, Instant::now()));
        trouvee
    }

    /// Un passage : ouvre ce qui doit l'être, ferme le reste. Renvoie les vues si elles ont changé.
    pub fn passer(&self, voulus: &[Voulu]) -> Option<Vec<Acces>> {
        let box_vues = self.calculer(voulus);
        *self.box_vues.lock().unwrap() = box_vues;
        *self.voulus.lock().unwrap() = voulus.to_vec();
        self.recombiner()
    }

    /// Les vues = la box, et le relais là où la box n'a pas marché. Renvoie les vues si elles ont changé.
    fn recombiner(&self) -> Option<Vec<Acces>> {
        let avant = self.vues();
        let voulus = self.voulus.lock().unwrap().clone();
        let box_vues = self.box_vues.lock().unwrap().clone();
        let mut relayes = Vec::new();
        let nouvelles: Vec<Acces> = box_vues
            .into_iter()
            .map(|a| {
                let Some(v) = voulus.iter().find(|v| v.id == a.id) else { return a };
                // server-ip qui bloque : le relais ne joindrait pas le serveur non plus.
                if a.etat != "erreur" || a.message.as_deref() == Some(SERVER_IP) {
                    return a;
                }
                relayes.push(v.id.as_str());
                self.tunnels.vouloir(&v.id, &v.nom, v.port);
                avec_relais(a, self.tunnels.etat(&v.id), v.bedrock.is_some())
            })
            .collect();
        self.tunnels.garder(&relayes);
        *self.vues.lock().unwrap() = nouvelles.into_iter().map(|a| (a.id.clone(), a)).collect();
        let apres = self.vues();
        (apres != avant).then_some(apres)
    }

    fn calculer(&self, voulus: &[Voulu]) -> Vec<Acces> {
        let erreur = |v: &Voulu, m: &str| Acces { id: v.id.clone(), etat: "erreur", adresse: None, message: Some(m.to_string()), bedrock: None, bedrock_message: None };
        if voulus.is_empty() && self.ouverts.lock().unwrap().is_empty() {
            return Vec::new();
        }
        let Some(passerelle) = self.trouver_box() else {
            return voulus.iter().map(|v| erreur(v, PAS_DE_BOX)).collect();
        };
        // Les ports qui ne sont plus voulus (interrupteur coupé, serveur retiré, port changé, Bedrock coupé) : on ferme.
        let voulues: Vec<Cle> = voulus.iter().flat_map(|v| {
            let mut l = vec![(false, v.port)];
            if let Some(b) = v.bedrock {
                l.extend([(true, b), (false, b)]);
            }
            l
        }).collect();
        let fermer: Vec<(Cle, Ouvert)> = {
            let ouverts = self.ouverts.lock().unwrap();
            ouverts.iter().filter(|(cle, _)| !voulues.contains(cle)).map(|(c, o)| (*c, o.clone())).collect()
        };
        for (cle, o) in &fermer {
            retirer(&passerelle, cle.0, o);
            self.ouverts.lock().unwrap().remove(cle);
        }
        if !fermer.is_empty() {
            self.noter_ports();
        }
        if voulus.is_empty() {
            return Vec::new();
        }
        let ip = match passerelle.get_external_ip() {
            Ok(IpAddr::V4(ip)) => ip,
            Ok(IpAddr::V6(_)) => return voulus.iter().map(|v| erreur(v, PANNE)).collect(),
            Err(igd_next::GetExternalIpError::ActionNotAuthorized) => return voulus.iter().map(|v| erreur(v, REFUS)).collect(),
            Err(_) => {
                // La box a peut-être changé (redémarrage) : on la recherchera au prochain passage.
                *self.box_trouvee.lock().unwrap() = None;
                return voulus.iter().map(|v| erreur(v, PANNE)).collect();
            }
        };
        match genre(ip) {
            Genre::Partagee => return voulus.iter().map(|v| erreur(v, PARTAGEE)).collect(),
            Genre::Privee => return voulus.iter().map(|v| erreur(v, DOUBLE)).collect(),
            Genre::Publique => {}
        }
        let Some(locale) = ip_vers(passerelle.addr) else {
            return voulus.iter().map(|v| erreur(v, PANNE)).collect();
        };
        let mut resultat = Vec::new();
        // Deux serveurs sur le même port : un seul peut tourner, on ouvre le port une fois.
        let mut faits: HashMap<Cle, Result<u16, &'static str>> = HashMap::new();
        for v in voulus {
            if server_ip_bloque(&v.server_ip, locale) {
                resultat.push(erreur(v, SERVER_IP));
                continue;
            }
            let externe = *faits.entry((false, v.port)).or_insert_with(|| self.ouvrir(&passerelle, false, v.port, SocketAddrV4::new(locale, v.port)));
            let mut acces = match externe {
                Ok(externe) => Acces { id: v.id.clone(), etat: "ouvert", adresse: Some(adresse(ip, externe)), message: None, bedrock: None, bedrock_message: None },
                Err(m) => erreur(v, m),
            };
            // Joueurs Bedrock : UDP (le jeu) et TCP (la mise en relation de Geyser), sur le même port.
            if let (Some(b), "ouvert") = (v.bedrock, acces.etat) {
                let udp = *faits.entry((true, b)).or_insert_with(|| self.ouvrir(&passerelle, true, b, SocketAddrV4::new(locale, b)));
                let _ = *faits.entry((false, b)).or_insert_with(|| self.ouvrir(&passerelle, false, b, SocketAddrV4::new(locale, b)));
                match udp {
                    Ok(externe) => acces.bedrock = Some(adresse_bedrock(ip, externe)),
                    Err(m) => acces.bedrock_message = Some(m.to_string()),
                }
            }
            resultat.push(acces);
        }
        resultat
    }

    /// Ouvre (ou renouvelle) un port sur la box, en TCP ou en UDP. Renvoie le port vu depuis Internet.
    fn ouvrir(&self, passerelle: &Gateway, udp: bool, port: u16, local: SocketAddrV4) -> Result<u16, &'static str> {
        let proto = protocole(udp);
        let deja = self.ouverts.lock().unwrap().get(&(udp, port)).cloned();
        if let Some(o) = &deja {
            let a_jour = o.local == local && (o.bail == 0 || o.renouvele.elapsed() < RENOUVELER);
            if a_jour {
                return Ok(o.externe);
            }
        }
        let description = if udp { format!("BHOST {port} UDP") } else { format!("BHOST {port}") };
        // L'adresse du PC a changé (box qui en donne une autre) : on retire l'ancienne ouverture,
        // sinon elle resterait tournée vers un autre appareil du réseau.
        if let Some(o) = &deja
            && o.local != local
        {
            retirer(passerelle, udp, o);
        }
        let externe_voulu = deja.as_ref().map_or(port, |o| o.externe);
        let mut bail = deja.as_ref().map_or(BAIL, |o| o.bail);
        let mut essai = passerelle.add_port(proto, externe_voulu, SocketAddr::V4(local), bail, &description);
        if matches!(essai, Err(AddPortError::OnlyPermanentLeasesSupported)) {
            bail = 0;
            essai = passerelle.add_port(proto, externe_voulu, SocketAddr::V4(local), bail, &description);
        }
        let externe = match essai {
            Ok(()) => externe_voulu,
            // Le port est déjà ouvert vers un autre appareil : on en prend un autre sur la box.
            Err(AddPortError::PortInUse) => {
                let mut libre = passerelle.add_any_port(proto, SocketAddr::V4(local), bail, &description);
                if matches!(libre, Err(AddAnyPortError::OnlyPermanentLeasesSupported)) {
                    bail = 0;
                    libre = passerelle.add_any_port(proto, SocketAddr::V4(local), bail, &description);
                }
                libre.map_err(|e| message_ajout_libre(&e))?
            }
            Err(e) => return Err(message_ajout(&e)),
        };
        let nouveau = Ouvert { externe, local, bail, renouvele: Instant::now() };
        let change = deja.as_ref().is_none_or(|o| o.externe != externe || o.local != local || o.bail != bail);
        self.ouverts.lock().unwrap().insert((udp, port), nouveau);
        if change {
            self.noter_ports();
        }
        Ok(externe)
    }

    /// Note les ports ouverts sur le disque.
    fn noter_ports(&self) {
        let Some(fichier) = self.fichier.get() else { return };
        let notes: Vec<Note> =
            self.ouverts.lock().unwrap().iter().map(|((udp, port), o)| Note { udp: *udp, port: *port, externe: o.externe, local: o.local, bail: o.bail }).collect();
        if let Ok(json) = serde_json::to_vec(&notes) {
            let _ = crate::garde::ecrire_atomique(fichier, &json);
        }
    }

    /// Au lancement : les ports notés la dernière fois (BHOST a pu planter sans les fermer).
    /// Le premier passage referme ceux qui ne sont plus voulus et renouvelle les autres.
    fn reprendre_ports(&self, fichier: std::path::PathBuf) {
        let notes: Vec<Note> = std::fs::read(&fichier).ok().and_then(|o| serde_json::from_slice(&o).ok()).unwrap_or_default();
        let vieux = Instant::now().checked_sub(RENOUVELER * 2).unwrap_or_else(Instant::now);
        let mut ouverts = self.ouverts.lock().unwrap();
        for n in notes {
            ouverts.entry((n.udp, n.port)).or_insert(Ouvert { externe: n.externe, local: n.local, bail: n.bail, renouvele: vieux });
        }
        drop(ouverts);
        let _ = self.fichier.set(fichier);
    }

    /// BHOST se ferme : on referme tous les ports ouverts (sans chercher la box si on ne l'a pas).
    pub fn tout_fermer(&self) {
        self.tunnels.tout_arreter();
        let ouverts: Vec<(Cle, Ouvert)> = self.ouverts.lock().unwrap().drain().collect();
        if ouverts.is_empty() {
            return;
        }
        let Some((passerelle, _)) = self.box_trouvee.lock().unwrap().clone() else { return };
        for ((udp, _), o) in &ouverts {
            retirer(&passerelle, *udp, o);
        }
        self.noter_ports();
    }

    /// Lance le fil. `voulus` lit la liste des serveurs ouverts ; `prevenir` envoie les vues à l'interface.
    /// `dossier` : %APPDATA%\BHOST (la clé du relais y est gardée).
    pub fn demarrer(
        self: &Arc<Self>,
        dossier: std::path::PathBuf,
        voulus: impl Fn() -> Vec<Voulu> + Send + 'static,
        prevenir: impl Fn(Vec<Acces>) + Send + 'static,
    ) {
        let (envoi, reception) = channel::<bool>();
        self.reprendre_ports(dossier.join("ports-ouverts.json"));
        self.tunnels.regler(dossier, envoi.clone());
        *self.reveil.lock().unwrap() = Some(envoi);
        let moi = Arc::clone(self);
        std::thread::spawn(move || {
            let mut tout = true;
            loop {
                let vues = if tout { moi.passer(&voulus()) } else { moi.recombiner() };
                if let Some(vues) = vues {
                    prevenir(vues);
                }
                // Un passage par minute, tout de suite si on touche l'interrupteur, et le relais dès qu'il change.
                tout = match reception.recv_timeout(Duration::from_secs(60)) {
                    Ok(t) => t,
                    Err(RecvTimeoutError::Timeout) => true,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                while let Ok(t) = reception.try_recv() {
                    tout |= t;
                }
            }
        });
    }
}

/// La box n'a pas marché : ce que montre le relais (en ligne → son adresse ; en route → « Ouverture… » ;
/// sinon le message de la box reste).
fn avec_relais(a: Acces, etat: Option<tunnel::Etat>, bedrock: bool) -> Acces {
    match etat {
        Some(tunnel::Etat::EnLigne(adresse)) => Acces {
            etat: "ouvert",
            adresse: Some(adresse),
            message: None,
            bedrock: None,
            bedrock_message: bedrock.then(|| BEDROCK_RELAIS.to_string()),
            ..a
        },
        Some(tunnel::Etat::Connexion | tunnel::Etat::Reconnexion) => Acces { etat: "ouverture", adresse: None, message: None, ..a },
        _ => a,
    }
}

/// L'adresse de ce PC sur le réseau qui mène à `vers` (aucun paquet n'est envoyé).
fn ip_vers(vers: SocketAddr) -> Option<Ipv4Addr> {
    let prise = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    prise.connect(vers).ok()?;
    match prise.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_les_adresses_de_box() {
        assert_eq!(genre(Ipv4Addr::new(82, 64, 1, 2)), Genre::Publique);
        assert_eq!(genre(Ipv4Addr::new(100, 64, 0, 1)), Genre::Partagee);
        assert_eq!(genre(Ipv4Addr::new(100, 127, 255, 255)), Genre::Partagee);
        assert_eq!(genre(Ipv4Addr::new(100, 128, 0, 1)), Genre::Publique);
        assert_eq!(genre(Ipv4Addr::new(192, 168, 1, 1)), Genre::Privee);
        assert_eq!(genre(Ipv4Addr::new(10, 0, 0, 1)), Genre::Privee);
        assert_eq!(genre(Ipv4Addr::new(172, 16, 0, 1)), Genre::Privee);
        assert_eq!(genre(Ipv4Addr::new(0, 0, 0, 0)), Genre::Privee);
    }

    #[test]
    fn adresse_sans_le_port_de_minecraft() {
        assert_eq!(adresse(Ipv4Addr::new(82, 64, 1, 2), 25565), "82.64.1.2");
        assert_eq!(adresse(Ipv4Addr::new(82, 64, 1, 2), 25570), "82.64.1.2:25570");
        assert_eq!(adresse_bedrock(Ipv4Addr::new(82, 64, 1, 2), 19132), "82.64.1.2 · 19132");
    }

    #[test]
    fn server_ip_qui_bloque() {
        let pc = Ipv4Addr::new(192, 168, 1, 20);
        assert!(!server_ip_bloque("", pc));
        assert!(!server_ip_bloque("  ", pc));
        assert!(!server_ip_bloque("0.0.0.0", pc));
        assert!(!server_ip_bloque("192.168.1.20", pc));
        assert!(server_ip_bloque("127.0.0.1", pc));
        assert!(server_ip_bloque("192.168.1.21", pc));
        assert!(server_ip_bloque("mon.serveur", pc));
    }

    #[test]
    fn rien_a_faire_sans_serveur_ouvert() {
        let etat = EtatAcces::default();
        // Aucun serveur voulu, aucun port ouvert : on ne cherche même pas la box.
        assert_eq!(etat.passer(&[]), None);
        assert!(etat.vues().is_empty());
    }

    #[test]
    fn le_relais_prend_le_relais_de_la_box() {
        let erreur = Acces { id: "a".into(), etat: "erreur", adresse: None, message: Some(PARTAGEE.into()), bedrock: None, bedrock_message: None };
        let en_ligne = avec_relais(erreur.clone(), Some(tunnel::Etat::EnLigne("survie-k3f9.bhost.app".into())), true);
        assert_eq!((en_ligne.etat, en_ligne.adresse.as_deref(), en_ligne.message), ("ouvert", Some("survie-k3f9.bhost.app"), None));
        assert_eq!(en_ligne.bedrock_message.as_deref(), Some(BEDROCK_RELAIS));
        assert_eq!(avec_relais(erreur.clone(), Some(tunnel::Etat::Reconnexion), false).etat, "ouverture");
        assert_eq!(avec_relais(erreur.clone(), Some(tunnel::Etat::Echec), false), erreur);
        assert_eq!(avec_relais(erreur.clone(), None, false), erreur);
    }

    #[test]
    fn interrupteur_montre_ouverture_puis_rien() {
        let etat = EtatAcces::default();
        let vues = etat.reveiller("a", true);
        assert_eq!(vues, vec![Acces { id: "a".into(), etat: "ouverture", adresse: None, message: None, bedrock: None, bedrock_message: None }]);
        assert!(etat.reveiller("a", false).is_empty());
    }

    /// Avec la fausse box (essai/fausse_box.py) : SCENARIO=publique|cgnat|refus|permanent|pris cargo test -- --ignored box
    #[test]
    #[ignore]
    fn avec_la_fausse_box() {
        let scenario = std::env::var("SCENARIO").unwrap_or_else(|_| "publique".into());
        let etat = EtatAcces::default();
        let v = vec![Voulu { id: "a".into(), nom: "A".into(), port: 25565, server_ip: String::new(), bedrock: Some(19132) }];
        let vues = etat.passer(&v).expect("des vues");
        println!("VUES {vues:?}");
        let attendu = match scenario.as_str() {
            "publique" | "permanent" => ("ouvert", Some("82.64.1.2".to_string()), None),
            "pris" => ("ouvert", Some("82.64.1.2:40001".to_string()), None),
            "cgnat" => ("erreur", None, Some(PARTAGEE.to_string())),
            "refus" => ("erreur", None, Some(REFUS.to_string())),
            _ => unreachable!(),
        };
        assert_eq!((vues[0].etat, vues[0].adresse.clone(), vues[0].message.clone()), attendu);
        // Deuxième passage : rien ne change (pas de renouvellement avant 20 min).
        assert_eq!(etat.passer(&v), None);
        if scenario == "permanent" {
            assert_eq!(etat.ouverts.lock().unwrap()[&(false, 25565)].bail, 0);
        }
        // server-ip qui bloque (sauf si la box elle-même empêche tout).
        if scenario == "cgnat" {
            return;
        }
        let bloque = vec![Voulu { id: "b".into(), nom: "B".into(), port: 25570, server_ip: "127.0.0.1".into(), bedrock: None }];
        let vues = etat.passer(&bloque).expect("des vues");
        assert_eq!(vues[0].message.as_deref(), Some(SERVER_IP));
        // Plus rien de voulu : le port est refermé.
        etat.passer(&[]);
        assert!(etat.ouverts.lock().unwrap().is_empty());
    }
}
