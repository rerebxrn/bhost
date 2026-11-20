// Le relais de bout en bout, sur la machine : un faux serveur Minecraft, un faux BHOST, un faux joueur.
use bhost_relais::protocole::{self, DeBhost, DuRelais};
use bhost_relais::{Reglages, demarrer};
use ed25519_dalek::{Signer, SigningKey};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const DOMAINE: &str = "bhost.test";

async fn relais() -> (SocketAddr, SocketAddr) {
    let local = "127.0.0.1:0".parse().unwrap();
    let (j, a, _) = demarrer(Reglages { domaine: DOMAINE.into(), joueurs: local, agents: local, registre: None }).await.unwrap();
    (j, a)
}

/// Un faux serveur Minecraft qui renvoie tout ce qu'il reçoit.
async fn echo() -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let adresse = l.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut f, _)) = l.accept().await {
            tokio::spawn(async move {
                let (mut r, mut w) = f.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    adresse
}

fn poignee(hote: &str, suite: i32) -> Vec<u8> {
    let mut c = vec![0x00];
    protocole::ecrire_varint(&mut c, 769);
    protocole::ecrire_varint(&mut c, hote.len() as i32);
    c.extend_from_slice(hote.as_bytes());
    c.extend(25565u16.to_be_bytes());
    protocole::ecrire_varint(&mut c, suite);
    let mut p = Vec::new();
    protocole::ecrire_varint(&mut p, c.len() as i32);
    p.extend(c);
    p
}

/// Se présente au relais avec la clé `graine` ; rend la réponse (OK ou NON) et la connexion.
async fn presenter(agents: SocketAddr, nom: &str, graine: u8, tricher: bool) -> (DuRelais, BufReader<TcpStream>) {
    let cle = SigningKey::from_bytes(&[graine; 32]);
    let mut f = BufReader::new(TcpStream::connect(agents).await.unwrap());
    let bonjour = protocole::ecrire_de_bhost(&DeBhost::Bonjour { nom: nom.into(), cle: protocole::hexa(cle.verifying_key().as_bytes()) });
    f.get_mut().write_all(format!("BHOST 1\n{bonjour}").as_bytes()).await.unwrap();
    let mut ligne = String::new();
    f.read_line(&mut ligne).await.unwrap();
    let defi = match protocole::lire_du_relais(&ligne) {
        Some(DuRelais::Defi { defi }) => defi,
        autre => return (autre.expect("réponse illisible"), f),
    };
    let texte = protocole::a_signer(nom, if tricher { "autre" } else { &defi });
    let signature = protocole::hexa(&cle.sign(texte.as_bytes()).to_bytes());
    f.get_mut().write_all(protocole::ecrire_de_bhost(&DeBhost::Signe { signature }).as_bytes()).await.unwrap();
    ligne.clear();
    f.read_line(&mut ligne).await.unwrap();
    (protocole::lire_du_relais(&ligne).expect("réponse illisible"), f)
}

/// Un faux BHOST en ligne : à chaque JOUEUR, il ouvre DONNEES et branche le joueur sur `serveur`.
async fn bhost(agents: SocketAddr, nom: &str) -> BufReader<TcpStream> {
    let (ok, f) = presenter(agents, nom, 7, false).await;
    assert_eq!(ok, DuRelais::Ok { adresse: format!("{nom}.{DOMAINE}") });
    f
}

fn servir(mut f: BufReader<TcpStream>, agents: SocketAddr, serveur: SocketAddr) {
    tokio::spawn(async move {
        let mut ligne = String::new();
        while f.read_line(&mut ligne).await.unwrap_or(0) > 0 {
            if let Some(DuRelais::Joueur { id, jeton }) = protocole::lire_du_relais(&ligne) {
                tokio::spawn(async move {
                    let mut d = TcpStream::connect(agents).await.unwrap();
                    d.write_all(format!("BHOST 1\n{}", protocole::ecrire_de_bhost(&DeBhost::Donnees { id, jeton })).as_bytes()).await.unwrap();
                    let mut s = TcpStream::connect(serveur).await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut d, &mut s).await;
                });
            }
            ligne.clear();
        }
    });
}

#[tokio::test]
async fn un_joueur_rejoint_le_serveur_par_le_relais() {
    let (joueurs, agents) = relais().await;
    let serveur = echo().await;
    let f = bhost(agents, "survie-k3f9").await;
    servir(f, agents, serveur);

    let mut j = TcpStream::connect(joueurs).await.unwrap();
    let mut envoi = poignee("Survie-K3F9.bhost.test", 2);
    envoi.extend_from_slice(b"connexion de KOTORO_YT");
    j.write_all(&envoi).await.unwrap();
    let mut recu = vec![0u8; envoi.len()];
    timeout(Duration::from_secs(5), j.read_exact(&mut recu)).await.unwrap().unwrap();
    assert_eq!(recu, envoi, "le serveur reçoit tout, poignée de main comprise");
    // Et ça continue dans les deux sens.
    j.write_all(b"encore").await.unwrap();
    let mut recu = [0u8; 6];
    timeout(Duration::from_secs(5), j.read_exact(&mut recu)).await.unwrap().unwrap();
    assert_eq!(&recu, b"encore");
}

#[tokio::test]
async fn serveur_eteint_dans_la_liste_et_a_la_connexion() {
    let (joueurs, _) = relais().await;
    // Liste des serveurs : statut « éteint » puis pong.
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    let mut envoi = poignee("personne.bhost.test", 1);
    envoi.extend([1, 0]);
    let mut ping = vec![9, 0x01];
    ping.extend(1234u64.to_be_bytes());
    envoi.extend(&ping);
    j.write_all(&envoi).await.unwrap();
    let attendu = protocole::statut_absent(769, "Ce serveur est éteint.");
    let mut recu = vec![0u8; attendu.len() + ping.len()];
    timeout(Duration::from_secs(5), j.read_exact(&mut recu)).await.unwrap().unwrap();
    assert_eq!(&recu[..attendu.len()], &attendu[..]);
    assert_eq!(&recu[attendu.len()..], &ping[..]);
    // Connexion : écran « Déconnecté ».
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    j.write_all(&poignee("personne.bhost.test", 2)).await.unwrap();
    let mut recu = Vec::new();
    timeout(Duration::from_secs(5), j.read_to_end(&mut recu)).await.unwrap().unwrap();
    assert_eq!(recu, protocole::refus_connexion("Ce serveur est éteint."));
    // Une adresse qui n'est pas à nous.
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    j.write_all(&poignee("82.64.1.2", 2)).await.unwrap();
    let mut recu = Vec::new();
    timeout(Duration::from_secs(5), j.read_to_end(&mut recu)).await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&recu).contains("Adresse inconnue"));
    // Pas du Minecraft : on coupe sans rien dire.
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    j.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();
    let mut recu = Vec::new();
    timeout(Duration::from_secs(5), j.read_to_end(&mut recu)).await.unwrap().unwrap();
    assert!(recu.is_empty());
}

#[tokio::test]
async fn un_nom_appartient_a_sa_cle() {
    let (_, agents) = relais().await;
    let (ok, _garde) = presenter(agents, "creatif", 1, false).await;
    assert!(matches!(ok, DuRelais::Ok { .. }));
    // Une autre clé : refusé, même déconnecté.
    let (non, _) = presenter(agents, "creatif", 2, false).await;
    assert_eq!(non, DuRelais::Non { raison: "pris".into() });
    // Une signature fausse : refusé.
    let (non, _) = presenter(agents, "autre-nom", 3, true).await;
    assert_eq!(non, DuRelais::Non { raison: "signature".into() });
    // Version inconnue.
    let mut f = BufReader::new(TcpStream::connect(agents).await.unwrap());
    f.get_mut().write_all(b"BHOST 99\n").await.unwrap();
    let mut ligne = String::new();
    f.read_line(&mut ligne).await.unwrap();
    assert_eq!(ligne, "NON version\n");
}

#[tokio::test]
async fn la_nouvelle_connexion_remplace_l_ancienne_et_repond_aux_ping() {
    let (_, agents) = relais().await;
    let mut ancienne = bhost(agents, "survie").await;
    let mut nouvelle = bhost(agents, "survie").await;
    // L'ancienne est coupée.
    let mut ligne = String::new();
    let n = timeout(Duration::from_secs(5), ancienne.read_line(&mut ligne)).await.unwrap().unwrap();
    assert_eq!(n, 0);
    // La nouvelle répond aux PING.
    nouvelle.get_mut().write_all(b"PING\n").await.unwrap();
    timeout(Duration::from_secs(5), nouvelle.read_line(&mut ligne)).await.unwrap().unwrap();
    assert_eq!(ligne, "PONG\n");
    // Une ligne inconnue : coupé.
    nouvelle.get_mut().write_all(b"N'IMPORTE QUOI\n").await.unwrap();
    ligne.clear();
    let n = timeout(Duration::from_secs(5), nouvelle.read_line(&mut ligne)).await.unwrap().unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn un_mauvais_jeton_ne_vole_pas_un_joueur() {
    let (joueurs, agents) = relais().await;
    let mut f = bhost(agents, "survie").await;
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    j.write_all(&poignee("survie.bhost.test", 2)).await.unwrap();
    let mut ligne = String::new();
    f.read_line(&mut ligne).await.unwrap();
    let Some(DuRelais::Joueur { id, .. }) = protocole::lire_du_relais(&ligne) else { panic!("{ligne}") };
    // Quelqu'un d'autre essaie de prendre ce joueur avec un faux jeton.
    let mut voleur = TcpStream::connect(agents).await.unwrap();
    voleur.write_all(format!("BHOST 1\nDONNEES {id} {}\n", "00".repeat(16)).as_bytes()).await.unwrap();
    let mut recu = Vec::new();
    timeout(Duration::from_secs(5), voleur.read_to_end(&mut recu)).await.unwrap().unwrap();
    assert!(recu.is_empty(), "le voleur n'a rien reçu");
}
