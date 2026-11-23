//! Le pilotage d'une machine dédiée est chiffré (TLS 1.3), par le relais comme en direct sur le réseau de la maison.
//!
//! - La machine est le « serveur » TLS. Son certificat est fait avec sa clé ed25519, celle qui donne son id
//!   (les 16 premiers caractères de la clé publique). Le relais ne voit passer que du chiffré.
//! - BHOST (le PC) vérifie que la clé du certificat donne bien l'id de la machine reliée, puis la retient :
//!   ensuite, seule cette clé est acceptée (une autre machine qui se ferait passer pour elle est refusée).
//! - Dans le tunnel chiffré, BHOST et la machine se prouvent qu'ils connaissent le même code, sans jamais l'envoyer
//!   (SPAKE2, voir appairage.rs) ; la machine répond « OK <id> » ou « NON code ». Puis les lignes JSON habituelles (agent.rs).
//!
//! Tout est bloquant (des fils, comme le reste du pilotage) : un fil lit, un autre écrit sur la prise ;
//! la connexion TLS est partagée derrière un verrou, et ce qui part sur la prise garde l'ordre des écritures.
// Le côté machine (accepter…) ne sert qu'à l'agent de BHOST OS et aux essais.
#![cfg_attr(feature = "bureau", allow(dead_code))]

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

/// Le port où un PC pilote la machine en direct (réseau de la maison, VPS), sans le relais.
pub const PORT_DIRECT: u16 = 4446;

/// Le port direct (essais : variable BHOST_PORT_DIRECT ; 0 = pas d'écoute sur la machine).
pub fn port_direct() -> u16 {
    std::env::var("BHOST_PORT_DIRECT").ok().and_then(|p| p.parse().ok()).unwrap_or(PORT_DIRECT)
}

/// Le nom mis dans le certificat (BHOST ne regarde que la clé).
const NOM: &str = "machine.bhost";

/// Le début d'une clé publique ed25519 dans un certificat (SubjectPublicKeyInfo), juste avant ses 32 octets.
const DEBUT_CLE: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];

fn fournisseur() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// La clé ed25519 au format PKCS#8 (v1) : un en-tête fixe, puis les 32 octets secrets.
fn pkcs8(secret: &[u8; 32]) -> Vec<u8> {
    let mut der = vec![0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];
    der.extend_from_slice(secret);
    der
}

/// La clé publique ed25519 d'un certificat. Elle doit y être une seule fois (sinon : refusé).
pub fn cle_du_certificat(der: &[u8]) -> Option<[u8; 32]> {
    let mut trouvee = None;
    for i in 0..der.len().saturating_sub(DEBUT_CLE.len() + 32 - 1) {
        if der[i..].starts_with(&DEBUT_CLE) {
            if trouvee.is_some() {
                return None;
            }
            let debut = i + DEBUT_CLE.len();
            trouvee = der.get(debut..debut + 32).and_then(|c| <[u8; 32]>::try_from(c).ok());
        }
    }
    trouvee
}

/// Côté machine : la configuration TLS, faite avec sa clé (le certificat est refait à chaque démarrage).
pub fn config_machine(secret: &[u8; 32]) -> Result<Arc<rustls::ServerConfig>, String> {
    let cle = PrivatePkcs8KeyDer::from(pkcs8(secret));
    let paire = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&cle, &rcgen::PKCS_ED25519).map_err(|e| e.to_string())?;
    let certificat = rcgen::CertificateParams::new(vec![NOM.to_string()]).and_then(|p| p.self_signed(&paire)).map_err(|e| e.to_string())?;
    let mut config = rustls::ServerConfig::builder_with_provider(fournisseur())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(vec![certificat.der().clone()], PrivateKeyDer::Pkcs8(cle))
        .map_err(|e| e.to_string())?;
    // Pas de reprise de session : une connexion dure longtemps, et rien ne reste en mémoire.
    config.send_tls13_tickets = 0;
    Ok(Arc::new(config))
}

/// Côté PC : vérifie que la machine est bien celle qui a cet id (et cette clé, si on la connaît déjà).
#[derive(Debug)]
struct Verif {
    id: String,
    connue: Option<[u8; 32]>,
    vue: Arc<Mutex<Option<[u8; 32]>>>,
    algos: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Verif {
    fn verify_server_cert(&self, certificat: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        let refus = || rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure);
        let cle = cle_du_certificat(certificat).ok_or_else(refus)?;
        let bonne = match self.connue {
            Some(connue) => connue == cle,
            None => crate::tunnel::protocole::id_machine(&crate::tunnel::protocole::hexa(&cle)) == self.id,
        };
        if !bonne {
            return Err(refus());
        }
        *self.vue.lock().unwrap() = Some(cle);
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(rustls::PeerIncompatible::Tls13RequiredForQuic))
    }

    fn verify_tls13_signature(&self, message: &[u8], certificat: &CertificateDer<'_>, signature: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, certificat, signature, &self.algos)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

/// Une connexion chiffrée, prête : `Canal` pour écrire (partageable), `Lecteur` pour lire (un seul fil).
pub struct Chiffree {
    pub canal: Canal,
    pub lecteur: Lecteur,
}

struct Interne {
    tls: Mutex<rustls::Connection>,
    /// Ce qui part sur la prise, dans l'ordre : un fil à part l'écrit (une écriture ne bloque jamais le lecteur).
    sortie: Sender<Vec<u8>>,
    prise: TcpStream,
}

/// Pour écrire sur la connexion chiffrée (depuis plusieurs fils).
#[derive(Clone)]
pub struct Canal(Arc<Interne>);

/// Pour lire la connexion chiffrée (un seul fil à la fois).
pub struct Lecteur {
    interne: Arc<Interne>,
    prise: TcpStream,
    tampon: Vec<u8>,
    lu: usize,
}

/// Fait passer ce que rustls veut envoyer vers le fil d'écriture (sous le verrou : l'ordre est gardé).
fn vider(tls: &mut rustls::Connection, sortie: &Sender<Vec<u8>>) -> io::Result<()> {
    if !tls.wants_write() {
        return Ok(());
    }
    let mut octets = Vec::new();
    while tls.wants_write() {
        tls.write_tls(&mut octets)?;
    }
    sortie.send(octets).map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))
}

impl Canal {
    pub fn ecrire(&self, octets: &[u8]) -> io::Result<()> {
        let mut tls = self.0.tls.lock().unwrap();
        // Par morceaux : le tampon de rustls ne se remplit jamais (une ligne peut faire plusieurs Mo).
        for morceau in octets.chunks(16 * 1024) {
            tls.writer().write_all(morceau)?;
            vider(&mut tls, &self.0.sortie)?;
        }
        Ok(())
    }

    /// Une ligne (le « \n » est ajouté).
    pub fn ecrire_ligne(&self, ligne: &str) -> io::Result<()> {
        let mut tout = Vec::with_capacity(ligne.len() + 1);
        tout.extend_from_slice(ligne.as_bytes());
        tout.push(b'\n');
        self.ecrire(&tout)
    }

    /// 32 octets propres à cette session TLS (RFC 5705) : la preuve du code y est attachée,
    /// elle ne vaut pour aucune autre connexion.
    pub fn empreinte_session(&self) -> io::Result<[u8; 32]> {
        let tls = self.0.tls.lock().unwrap();
        tls.export_keying_material([0u8; 32], b"bhost-appairage", None).map_err(io::Error::other)
    }

    /// Coupe la connexion (les deux sens).
    pub fn fermer(&self) {
        let _ = self.0.prise.shutdown(std::net::Shutdown::Both);
    }
}

impl Lecteur {
    /// Le délai de lecture de la prise (`None` : attendre sans fin).
    pub fn delai(&self, delai: Option<std::time::Duration>) -> io::Result<()> {
        self.prise.set_read_timeout(delai)
    }
}

impl Read for Lecteur {
    fn read(&mut self, sortie: &mut [u8]) -> io::Result<usize> {
        loop {
            {
                let mut tls = self.interne.tls.lock().unwrap();
                // 1. Ce qui est déjà déchiffré.
                match tls.reader().read(sortie) {
                    Ok(n) => return Ok(n),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e),
                }
                // 2. Ce qui reste de la prise : on le donne à rustls, jusqu'à avoir du texte à rendre.
                let mut texte = false;
                while self.lu < self.tampon.len() && !texte {
                    let mut reste = &self.tampon[self.lu..];
                    let n = tls.read_tls(&mut reste)?;
                    self.lu += n;
                    let etat = tls.process_new_packets().map_err(io::Error::other)?;
                    vider(&mut tls, &self.interne.sortie)?;
                    texte = etat.plaintext_bytes_to_read() > 0 || etat.peer_has_closed();
                    if n == 0 {
                        break;
                    }
                }
                if texte {
                    continue;
                }
            }
            // 3. Lire la prise (sans le verrou : les écritures continuent pendant l'attente).
            self.tampon.resize(16 * 1024, 0);
            let n = self.prise.read(&mut self.tampon)?;
            self.tampon.truncate(n);
            self.lu = 0;
            if n == 0 {
                return Ok(0);
            }
        }
    }
}

/// La poignée de main TLS (bloquante, avec le délai de lecture de la prise), puis les deux fils.
fn finir(mut prise: TcpStream, mut tls: rustls::Connection) -> io::Result<Chiffree> {
    while tls.is_handshaking() {
        tls.complete_io(&mut prise)?;
    }
    let (sortie, attente) = mpsc::channel::<Vec<u8>>();
    let mut ecriture = prise.try_clone()?;
    std::thread::spawn(move || {
        for octets in attente {
            if ecriture.write_all(&octets).is_err() {
                break;
            }
        }
        let _ = ecriture.shutdown(std::net::Shutdown::Both);
    });
    let lecture = prise.try_clone()?;
    let interne = Arc::new(Interne { tls: Mutex::new(tls), sortie, prise });
    // Ce que la poignée de main a laissé à envoyer.
    vider(&mut interne.tls.lock().unwrap(), &interne.sortie)?;
    Ok(Chiffree { canal: Canal(interne.clone()), lecteur: Lecteur { interne, prise: lecture, tampon: Vec::new(), lu: 0 } })
}

/// TCP garde la connexion vivante : un PC ou une machine partis (veille, Wi-Fi coupé) sont vus en moins de 2 minutes.
pub fn garder_vivante(prise: &TcpStream) {
    let regle = socket2::TcpKeepalive::new().with_time(std::time::Duration::from_secs(60)).with_interval(std::time::Duration::from_secs(15));
    let _ = socket2::SockRef::from(prise).set_tcp_keepalive(&regle);
}

/// Côté machine : accepte un PC sur cette prise.
pub fn accepter(prise: TcpStream, config: Arc<rustls::ServerConfig>) -> io::Result<Chiffree> {
    let tls = rustls::ServerConnection::new(config).map_err(io::Error::other)?;
    finir(prise, rustls::Connection::Server(tls))
}

/// Côté PC : se connecte à la machine `id` sur cette prise. Renvoie aussi la clé vue (à retenir).
pub fn connecter(prise: TcpStream, id: &str, connue: Option<[u8; 32]>) -> io::Result<(Chiffree, [u8; 32])> {
    let vue = Arc::new(Mutex::new(None));
    let fournisseur = fournisseur();
    let verif = Verif { id: id.to_string(), connue, vue: vue.clone(), algos: fournisseur.signature_verification_algorithms };
    let config = rustls::ClientConfig::builder_with_provider(fournisseur)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verif))
        .with_no_client_auth();
    let nom = ServerName::try_from(NOM).map_err(io::Error::other)?;
    let tls = rustls::ClientConnection::new(Arc::new(config), nom).map_err(io::Error::other)?;
    let chiffree = finir(prise, rustls::Connection::Client(tls))?;
    let cle = vue.lock().unwrap().ok_or_else(|| io::Error::other("clé de la machine absente"))?;
    Ok((chiffree, cle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn cle_de_test(n: u8) -> ([u8; 32], [u8; 32]) {
        let secret = [n; 32];
        let publique = ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key().to_bytes();
        (secret, publique)
    }

    fn id_de(publique: &[u8; 32]) -> String {
        crate::tunnel::protocole::id_machine(&crate::tunnel::protocole::hexa(publique))
    }

    /// Une machine qui répond « écho » à chaque ligne.
    fn machine_echo(secret: [u8; 32]) -> std::net::SocketAddr {
        let ecoute = TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = ecoute.local_addr().unwrap();
        let config = config_machine(&secret).unwrap();
        std::thread::spawn(move || {
            for prise in ecoute.incoming().flatten() {
                let config = config.clone();
                std::thread::spawn(move || {
                    let Ok(c) = accepter(prise, config) else { return };
                    let mut lecteur = BufReader::new(c.lecteur);
                    let mut ligne = String::new();
                    while lecteur.read_line(&mut ligne).unwrap_or(0) > 0 {
                        let _ = c.canal.ecrire_ligne(&format!("écho {}", ligne.trim_end()));
                        ligne.clear();
                    }
                });
            }
        });
        adresse
    }

    #[test]
    fn la_cle_est_dans_le_certificat() {
        let (secret, publique) = cle_de_test(7);
        let paire = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&PrivatePkcs8KeyDer::from(pkcs8(&secret)), &rcgen::PKCS_ED25519).unwrap();
        let cert = rcgen::CertificateParams::new(vec![NOM.to_string()]).unwrap().self_signed(&paire).unwrap();
        assert_eq!(cle_du_certificat(cert.der()), Some(publique));
        // Deux fois la clé dans le certificat : refusé.
        let mut double = cert.der().to_vec();
        double.extend_from_slice(&DEBUT_CLE);
        double.extend_from_slice(&publique);
        assert_eq!(cle_du_certificat(&double), None);
    }

    #[test]
    fn aller_retour_chiffre() {
        let (secret, publique) = cle_de_test(9);
        let adresse = machine_echo(secret);
        let (c, vue) = connecter(TcpStream::connect(adresse).unwrap(), &id_de(&publique), None).unwrap();
        assert_eq!(vue, publique);
        let mut lecteur = BufReader::new(c.lecteur);
        // Une petite ligne, puis une très grosse (plusieurs enregistrements TLS).
        for texte in ["bonjour".to_string(), "x".repeat(3 * 1024 * 1024)] {
            c.canal.ecrire_ligne(&texte).unwrap();
            let mut ligne = String::new();
            lecteur.read_line(&mut ligne).unwrap();
            assert_eq!(ligne.trim_end(), format!("écho {texte}"));
        }
        // Avec la clé retenue : accepté.
        assert!(connecter(TcpStream::connect(adresse).unwrap(), &id_de(&publique), Some(publique)).is_ok());
    }

    #[test]
    fn mauvaise_machine_refusee() {
        let (secret, _) = cle_de_test(11);
        let (_, autre) = cle_de_test(12);
        let adresse = machine_echo(secret);
        // L'id d'une autre machine.
        assert!(connecter(TcpStream::connect(adresse).unwrap(), &id_de(&autre), None).is_err());
        // Le bon id, mais pas la clé retenue.
        let (_, publique) = cle_de_test(11);
        assert!(connecter(TcpStream::connect(adresse).unwrap(), &id_de(&publique), Some(autre)).is_err());
    }
}
