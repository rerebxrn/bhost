// Réveil à la connexion : quand le serveur dort (arrêté), BHOST écoute à sa place sur son port.
// - Dans la liste des serveurs du jeu, il répond « Le serveur dort… » (avec l'icône du serveur).
// - Quand quelqu'un essaie d'entrer, BHOST lui dit de revenir dans une minute, libère le port et allume le serveur.
// Écrit pour BHOST à partir de la description publique du protocole de Minecraft Java
// (poignée de main, état, connexion). Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// Ce que BHOST montre pendant que le serveur dort.
#[derive(Clone, Debug)]
pub struct Infos {
    /// Le message de la liste des serveurs.
    pub motd: String,
    /// « Paper 1.21.11 ».
    pub version: String,
    pub joueurs_max: u32,
    /// L'icône (server-icon.png), en « data:image/png;base64,… ».
    pub icone: Option<String>,
    /// Ce que voit celui qui essaie d'entrer.
    pub message: String,
}

/// Ce qu'a fait un visiteur.
#[derive(Debug, PartialEq)]
pub enum Visite {
    /// Il a regardé la liste des serveurs.
    Statut,
    /// Il a essayé d'entrer (son pseudo).
    Connexion(String),
    /// Autre chose (vieux client, connexion coupée…).
    Autre,
}

const PAQUET_MAX: i32 = 4096;

fn invalide(texte: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, texte.to_string())
}

/// Un entier « VarInt » : 7 bits par octet, le bit du haut dit qu'il y a une suite.
pub fn lire_varint(r: &mut impl Read) -> io::Result<i32> {
    let mut valeur: u32 = 0;
    for i in 0..5 {
        let mut octet = [0u8];
        r.read_exact(&mut octet)?;
        valeur |= ((octet[0] & 0x7F) as u32) << (7 * i);
        if octet[0] & 0x80 == 0 {
            return Ok(valeur as i32);
        }
    }
    Err(invalide("VarInt trop long"))
}

pub fn ecrire_varint(sortie: &mut Vec<u8>, valeur: i32) {
    let mut v = valeur as u32;
    loop {
        if v & !0x7F == 0 {
            sortie.push(v as u8);
            return;
        }
        sortie.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
}

/// Une chaîne : sa longueur en octets (VarInt), puis l'UTF-8.
fn ecrire_chaine(sortie: &mut Vec<u8>, texte: &str) {
    ecrire_varint(sortie, texte.len() as i32);
    sortie.extend_from_slice(texte.as_bytes());
}

fn lire_chaine(r: &mut impl Read, max: usize) -> io::Result<String> {
    let n = lire_varint(r)?;
    if n < 0 || n as usize > max {
        return Err(invalide("chaîne trop longue"));
    }
    let mut octets = vec![0u8; n as usize];
    r.read_exact(&mut octets)?;
    String::from_utf8(octets).map_err(|_| invalide("UTF-8"))
}

/// Un paquet : sa longueur, son numéro, ses données.
pub fn paquet(numero: i32, donnees: &[u8]) -> Vec<u8> {
    let mut corps = Vec::new();
    ecrire_varint(&mut corps, numero);
    corps.extend_from_slice(donnees);
    let mut tout = Vec::new();
    ecrire_varint(&mut tout, corps.len() as i32);
    tout.extend(corps);
    tout
}

/// Lit un paquet entier : (numéro, données).
fn lire_paquet(r: &mut impl Read) -> io::Result<(i32, Vec<u8>)> {
    let longueur = lire_varint(r)?;
    if !(1..=PAQUET_MAX).contains(&longueur) {
        return Err(invalide("paquet de taille invalide"));
    }
    let mut corps = vec![0u8; longueur as usize];
    r.read_exact(&mut corps)?;
    let mut curseur = io::Cursor::new(corps);
    let numero = lire_varint(&mut curseur)?;
    let debut = curseur.position() as usize;
    Ok((numero, curseur.into_inner()[debut..].to_vec()))
}

/// La réponse pour la liste des serveurs du jeu.
pub fn reponse_statut(infos: &Infos, protocole: i32) -> String {
    let mut json = serde_json::json!({
        "version": { "name": infos.version, "protocol": protocole },
        "players": { "max": infos.joueurs_max, "online": 0, "sample": [] },
        "description": { "text": infos.motd },
    });
    if let Some(icone) = &infos.icone {
        json["favicon"] = serde_json::Value::String(icone.clone());
    }
    json.to_string()
}

/// Répond à un visiteur, du début à la fin de sa visite.
pub fn accueillir(flux: &mut TcpStream, infos: &Infos) -> io::Result<Visite> {
    // Les très vieux clients (avant 1.7) commencent par 0xFE : on ne leur répond pas.
    let mut premier = [0u8];
    if flux.peek(&mut premier)? == 0 || premier[0] == 0xFE {
        return Ok(Visite::Autre);
    }
    let (numero, donnees) = lire_paquet(flux)?;
    if numero != 0 {
        return Ok(Visite::Autre);
    }
    // Poignée de main : protocole, adresse, port, et ce que veut le client.
    let mut c = io::Cursor::new(donnees);
    let protocole = lire_varint(&mut c)?;
    let _adresse = lire_chaine(&mut c, 1024)?;
    let mut port = [0u8; 2];
    c.read_exact(&mut port)?;
    match lire_varint(&mut c)? {
        // 1 : la liste des serveurs.
        1 => {
            let (numero, _) = lire_paquet(flux)?;
            if numero != 0 {
                return Ok(Visite::Autre);
            }
            let mut corps = Vec::new();
            ecrire_chaine(&mut corps, &reponse_statut(infos, protocole));
            flux.write_all(&paquet(0, &corps))?;
            // Puis le « ping » : on renvoie le même nombre.
            if let Ok((1, nombre)) = lire_paquet(flux) {
                let _ = flux.write_all(&paquet(1, &nombre));
            }
            Ok(Visite::Statut)
        }
        // 2 : entrer dans le jeu (3 : transfert depuis un autre serveur).
        2 | 3 => {
            let (_, donnees) = lire_paquet(flux)?;
            let pseudo = lire_chaine(&mut io::Cursor::new(donnees), 64).unwrap_or_default();
            // Refus poli : le message est un texte JSON.
            let mut corps = Vec::new();
            ecrire_chaine(&mut corps, &serde_json::json!({ "text": infos.message }).to_string());
            flux.write_all(&paquet(0, &corps))?;
            let _ = flux.flush();
            Ok(Visite::Connexion(pseudo))
        }
        _ => Ok(Visite::Autre),
    }
}

/// BHOST qui écoute sur le port d'un serveur endormi.
pub struct Veilleur {
    arret: Arc<AtomicBool>,
    fil: Option<JoinHandle<()>>,
    /// Le port écouté (utile quand on a demandé le port 0).
    #[cfg_attr(not(test), allow(dead_code))]
    pub port: u16,
}

impl Veilleur {
    /// Écoute sur `port` (0 : un port libre, pour les tests). `reveil` est appelé (une fois)
    /// quand quelqu'un essaie d'entrer, après avoir libéré le port.
    pub fn lancer(port: u16, infos: Infos, reveil: impl FnOnce(String) + Send + 'static) -> io::Result<Veilleur> {
        let ecoute = TcpListener::bind(("0.0.0.0", port))?;
        ecoute.set_nonblocking(true)?;
        let port = ecoute.local_addr()?.port();
        let arret = Arc::new(AtomicBool::new(false));
        let arret2 = arret.clone();
        let fil = std::thread::spawn(move || {
            let mut qui = None;
            while !arret2.load(Ordering::SeqCst) {
                match ecoute.accept() {
                    Ok((mut flux, _)) => {
                        let _ = flux.set_nonblocking(false);
                        // Délais courts : un visiteur qui ne dit rien ne bloque pas l'écoute longtemps.
                        let _ = flux.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = flux.set_write_timeout(Some(Duration::from_secs(2)));
                        if let Ok(Visite::Connexion(pseudo)) = accueillir(&mut flux, &infos) {
                            qui = Some(pseudo);
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
            drop(ecoute); // le port est libre pour le vrai serveur
            if let Some(pseudo) = qui {
                arret2.store(true, Ordering::SeqCst);
                reveil(pseudo);
            }
        });
        Ok(Veilleur { arret, fil: Some(fil), port })
    }

    /// Le veilleur s'est arrêté tout seul (quelqu'un a voulu entrer).
    pub fn fini(&self) -> bool {
        self.arret.load(Ordering::SeqCst)
    }

    /// Arrête d'écouter et attend que le port soit libre.
    pub fn arreter(mut self) {
        self.arret.store(true, Ordering::SeqCst);
        if let Some(fil) = self.fil.take() {
            if fil.thread().id() != std::thread::current().id() {
                let _ = fil.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn infos() -> Infos {
        Infos {
            motd: "Le serveur dort.".into(),
            version: "Paper 1.21.11".into(),
            joueurs_max: 20,
            icone: None,
            message: "Il se réveille. Reviens dans 1 min !".into(),
        }
    }

    fn poignee(port: u16, etat: i32) -> Vec<u8> {
        let mut d = Vec::new();
        ecrire_varint(&mut d, 774);
        ecrire_chaine(&mut d, "localhost");
        d.extend_from_slice(&port.to_be_bytes());
        ecrire_varint(&mut d, etat);
        paquet(0, &d)
    }

    #[test]
    fn varint_aller_retour() {
        for n in [0, 1, 127, 128, 255, 25565, 2_097_151, i32::MAX, -1] {
            let mut v = Vec::new();
            ecrire_varint(&mut v, n);
            assert_eq!(lire_varint(&mut io::Cursor::new(v)).unwrap(), n);
        }
        let mut v = Vec::new();
        ecrire_varint(&mut v, 300);
        assert_eq!(v, [0xAC, 0x02]);
    }

    #[test]
    fn repond_a_la_liste_puis_reveille() {
        let (envoi, reception) = mpsc::channel();
        let veilleur = Veilleur::lancer(0, infos(), move |pseudo| envoi.send(pseudo).unwrap()).unwrap();
        let port = veilleur.port;

        // La liste des serveurs.
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.write_all(&poignee(port, 1)).unwrap();
        client.write_all(&paquet(0, &[])).unwrap();
        let (n, d) = lire_paquet(&mut client).unwrap();
        assert_eq!(n, 0);
        let json: serde_json::Value = serde_json::from_str(&lire_chaine(&mut io::Cursor::new(d), 100_000).unwrap()).unwrap();
        assert_eq!(json["description"]["text"], "Le serveur dort.");
        assert_eq!(json["version"]["protocol"], 774);
        assert_eq!(json["players"]["max"], 20);
        client.write_all(&paquet(1, &42i64.to_be_bytes())).unwrap();
        assert_eq!(lire_paquet(&mut client).unwrap(), (1, 42i64.to_be_bytes().to_vec()));
        assert!(reception.try_recv().is_err(), "regarder la liste ne réveille pas");

        // Quelqu'un veut entrer.
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.write_all(&poignee(port, 2)).unwrap();
        let mut d = Vec::new();
        ecrire_chaine(&mut d, "Hugo");
        d.extend_from_slice(&[0u8; 16]);
        client.write_all(&paquet(0, &d)).unwrap();
        let (n, d) = lire_paquet(&mut client).unwrap();
        assert_eq!(n, 0);
        assert!(lire_chaine(&mut io::Cursor::new(d), 10_000).unwrap().contains("Reviens dans 1 min"));
        assert_eq!(reception.recv_timeout(Duration::from_secs(5)).unwrap(), "Hugo");
        assert!(veilleur.fini());
        // Le port est libéré.
        std::thread::sleep(Duration::from_millis(300));
        assert!(TcpListener::bind(("0.0.0.0", port)).is_ok());
        veilleur.arreter();
    }

    #[test]
    fn s_arrete_et_libere_le_port() {
        let veilleur = Veilleur::lancer(0, infos(), |_| panic!("personne n'est entré")).unwrap();
        let port = veilleur.port;
        veilleur.arreter();
        assert!(TcpListener::bind(("0.0.0.0", port)).is_ok());
    }
}
