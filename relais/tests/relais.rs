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
    let (j, a, _) = demarrer(Reglages::simples(DOMAINE, local, local, None)).await.unwrap();
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
    let attendu = protocole::statut_absent(769, "Serveur éteint ou introuvable.");
    let mut recu = vec![0u8; attendu.len() + ping.len()];
    timeout(Duration::from_secs(5), j.read_exact(&mut recu)).await.unwrap().unwrap();
    assert_eq!(&recu[..attendu.len()], &attendu[..]);
    assert_eq!(&recu[attendu.len()..], &ping[..]);
    // Connexion : écran « Déconnecté ».
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    j.write_all(&poignee("personne.bhost.test", 2)).await.unwrap();
    let mut recu = Vec::new();
    timeout(Duration::from_secs(5), j.read_to_end(&mut recu)).await.unwrap().unwrap();
    assert_eq!(recu, protocole::refus_connexion("Serveur éteint ou introuvable."));
    // Une adresse qui n'est pas à nous.
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    j.write_all(&poignee("82.64.1.2", 2)).await.unwrap();
    let mut recu = Vec::new();
    timeout(Duration::from_secs(5), j.read_to_end(&mut recu)).await.unwrap().unwrap();
    assert!(String::from_utf8_lossy(&recu).contains("introuvable"));
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

/// L'empreinte d'un code, pour les essais (BHOST la calcule avec scrypt : le relais ne voit que 64 caractères hexa).
fn e(code: &str) -> String {
    protocole::hexa(code.as_bytes()).chars().cycle().take(64).collect()
}

/// Une fausse machine dédiée (BHOST OS) : elle se présente avec l'empreinte de son code et reste en ligne.
async fn machine(agents: SocketAddr, code: &str, graine: u8) -> (DuRelais, BufReader<TcpStream>) {
    let code = e(code);
    let code = code.as_str();
    let cle = SigningKey::from_bytes(&[graine; 32]);
    let mut f = BufReader::new(TcpStream::connect(agents).await.unwrap());
    let m = protocole::ecrire_de_bhost(&DeBhost::Machine { cle: protocole::hexa(cle.verifying_key().as_bytes()), empreinte: code.into() });
    f.get_mut().write_all(format!("BHOST 2\n{m}").as_bytes()).await.unwrap();
    let mut ligne = String::new();
    f.read_line(&mut ligne).await.unwrap();
    let Some(DuRelais::Defi { defi }) = protocole::lire_du_relais(&ligne) else { panic!("pas de défi : {ligne}") };
    let signature = protocole::hexa(&cle.sign(protocole::a_signer_machine(code, &defi).as_bytes()).to_bytes());
    f.get_mut().write_all(protocole::ecrire_de_bhost(&DeBhost::Signe { signature }).as_bytes()).await.unwrap();
    ligne.clear();
    f.read_line(&mut ligne).await.unwrap();
    (protocole::lire_du_relais(&ligne).expect("réponse illisible"), f)
}

/// Un PC tape un code : la réponse du relais.
async fn lier(agents: SocketAddr, code: &str) -> DuRelais {
    let mut f = BufReader::new(TcpStream::connect(agents).await.unwrap());
    f.get_mut().write_all(format!("BHOST 2\nLIER {}\n", e(code)).as_bytes()).await.unwrap();
    let mut ligne = String::new();
    timeout(Duration::from_secs(5), f.read_line(&mut ligne)).await.unwrap().unwrap();
    protocole::lire_du_relais(&ligne).expect("réponse illisible")
}

#[tokio::test]
async fn machine_reliee_par_son_code_et_essais_limites() {
    let (_, agents) = relais().await;
    let (ok, _m) = machine(agents, "K7M2Q9XR", 9).await;
    let id = protocole::id_machine(&protocole::hexa(SigningKey::from_bytes(&[9; 32]).verifying_key().as_bytes()));
    assert_eq!(ok, DuRelais::Ok { adresse: id.clone() });
    // Le bon code : l'id de la machine.
    assert_eq!(lier(agents, "K7M2Q9XR").await, DuRelais::Liee { id: id.clone() });
    // Une autre machine ne peut pas prendre le même code.
    assert_eq!(machine(agents, "K7M2Q9XR", 10).await.0, DuRelais::Non { raison: "pris".into() });
    // 5 codes faux, puis même le bon code attend.
    for _ in 0..5 {
        assert_eq!(lier(agents, "ZZZZ2222").await, DuRelais::Non { raison: "code".into() });
    }
    assert_eq!(lier(agents, "K7M2Q9XR").await, DuRelais::Non { raison: "bloque".into() });
}

#[tokio::test]
async fn machine_partie_plus_joignable() {
    let (_, agents) = relais().await;
    let (_, m) = machine(agents, "ABCD2345", 11).await;
    drop(m);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(lier(agents, "ABCD2345").await, DuRelais::Non { raison: "code".into() });
}

#[tokio::test]
async fn un_pc_pilote_la_machine_par_son_code() {
    let (_, agents) = relais().await;
    let (_, mut m) = machine(agents, "PQRS2345", 12).await;
    let id = protocole::id_machine(&protocole::hexa(SigningKey::from_bytes(&[12; 32]).verifying_key().as_bytes()));
    // La machine : à chaque PILOTE, elle ouvre DONNEES et renvoie chaque ligne en majuscules.
    tokio::spawn(async move {
        let mut ligne = String::new();
        while m.read_line(&mut ligne).await.unwrap_or(0) > 0 {
            if let Some(DuRelais::Pilote { id, jeton }) = protocole::lire_du_relais(&ligne) {
                tokio::spawn(async move {
                    let mut d = BufReader::new(TcpStream::connect(agents).await.unwrap());
                    let donnees = protocole::ecrire_de_bhost(&DeBhost::Donnees { id, jeton });
                    d.get_mut().write_all(format!("BHOST 1\n{donnees}").as_bytes()).await.unwrap();
                    let mut l = String::new();
                    while d.read_line(&mut l).await.unwrap_or(0) > 0 {
                        d.get_mut().write_all(l.to_uppercase().as_bytes()).await.unwrap();
                        l.clear();
                    }
                });
            }
            ligne.clear();
        }
    });
    // Le PC : PILOTER, puis il parle à la machine à travers le relais.
    let mut pc = BufReader::new(TcpStream::connect(agents).await.unwrap());
    pc.get_mut().write_all(format!("BHOST 2\nPILOTER {} {id}\n", e("PQRS2345")).as_bytes()).await.unwrap();
    let mut ligne = String::new();
    timeout(Duration::from_secs(5), pc.read_line(&mut ligne)).await.unwrap().unwrap();
    assert_eq!(protocole::lire_du_relais(&ligne), Some(DuRelais::Ok { adresse: id.clone() }));
    pc.get_mut().write_all(b"{\"cmd\":\"lister\"}\n").await.unwrap();
    ligne.clear();
    timeout(Duration::from_secs(5), pc.read_line(&mut ligne)).await.unwrap().unwrap();
    assert_eq!(ligne, "{\"CMD\":\"LISTER\"}\n");
    // Un mauvais code ne pilote rien.
    let mut autre = BufReader::new(TcpStream::connect(agents).await.unwrap());
    autre.get_mut().write_all(format!("BHOST 2\nPILOTER {} {id}\n", e("ZZZZ2345")).as_bytes()).await.unwrap();
    ligne.clear();
    timeout(Duration::from_secs(5), autre.read_line(&mut ligne)).await.unwrap().unwrap();
    assert_eq!(protocole::lire_du_relais(&ligne), Some(DuRelais::Non { raison: "code".into() }));
    // Une machine éteinte (id inconnu du relais) : « absente », sans compter d'essai.
    for _ in 0..6 {
        let mut x = BufReader::new(TcpStream::connect(agents).await.unwrap());
        x.get_mut().write_all(format!("BHOST 2\nPILOTER {} 00000000000000ff\n", e("ZZZZ2345")).as_bytes()).await.unwrap();
        ligne.clear();
        timeout(Duration::from_secs(5), x.read_line(&mut ligne)).await.unwrap().unwrap();
        assert_eq!(protocole::lire_du_relais(&ligne), Some(DuRelais::Non { raison: "absente".into() }));
    }
    assert_eq!(lier(agents, "PQRS2345").await, DuRelais::Liee { id }, "pas bloqué par les essais d'une machine éteinte");
}

/// Une ligne envoyée au relais, sa réponse.
async fn demander(agents: SocketAddr, m: &DeBhost) -> (DuRelais, BufReader<TcpStream>) {
    let mut f = BufReader::new(TcpStream::connect(agents).await.unwrap());
    f.get_mut().write_all(format!("BHOST 1\n{}", protocole::ecrire_de_bhost(m)).as_bytes()).await.unwrap();
    let mut ligne = String::new();
    f.read_line(&mut ligne).await.unwrap();
    (protocole::lire_du_relais(&ligne).expect("réponse illisible"), f)
}

#[tokio::test]
async fn nom_libre_puis_rendu() {
    let (_, agents) = relais().await;
    let cle = SigningKey::from_bytes(&[7; 32]);
    let a = protocole::hexa(cle.verifying_key().as_bytes());
    let b = protocole::hexa(SigningKey::from_bytes(&[8; 32]).verifying_key().as_bytes());
    let libre = |nom: &str, cle: &str| DeBhost::Libre { nom: nom.into(), cle: cle.into() };
    // Libre pour tous, au début.
    assert_eq!(demander(agents, &libre("survie", &b)).await.0, DuRelais::Ok { adresse: format!("survie.{DOMAINE}") });
    // A le prend : il est pris pour B, mais encore « libre » pour A (c'est le sien).
    let (ok, connexion) = presenter(agents, "survie", 7, false).await;
    assert!(matches!(ok, DuRelais::Ok { .. }));
    assert_eq!(demander(agents, &libre("survie", &b)).await.0, DuRelais::Non { raison: "pris".into() });
    assert!(matches!(demander(agents, &libre("survie", &a)).await.0, DuRelais::Ok { .. }));
    // Un nom réservé, ou mal écrit.
    assert_eq!(demander(agents, &libre("panel", &b)).await.0, DuRelais::Non { raison: "nom".into() });
    assert_eq!(demander(agents, &libre("Pas-Bon", &b)).await.0, DuRelais::Non { raison: "nom".into() });
    // Rendre le nom : refusé tant qu'il est en ligne, puis accepté (avec la bonne signature seulement).
    let lacher = |signer: &SigningKey, vrai: bool| {
        let signer = signer.clone();
        let a = a.clone();
        async move {
            let (defi, mut f) = demander(agents, &DeBhost::Lacher { nom: "survie".into(), cle: a }).await;
            let DuRelais::Defi { defi } = defi else { panic!("pas de défi") };
            let texte = protocole::a_signer_lacher("survie", if vrai { &defi } else { "autre" });
            let signature = protocole::hexa(&signer.sign(texte.as_bytes()).to_bytes());
            f.get_mut().write_all(protocole::ecrire_de_bhost(&DeBhost::Signe { signature }).as_bytes()).await.unwrap();
            let mut ligne = String::new();
            f.read_line(&mut ligne).await.unwrap();
            protocole::lire_du_relais(&ligne).unwrap()
        }
    };
    assert_eq!(lacher(&cle, true).await, DuRelais::Non { raison: "refus".into() });
    drop(connexion);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(lacher(&cle, false).await, DuRelais::Non { raison: "refus".into() });
    assert_eq!(lacher(&cle, true).await, DuRelais::Ok { adresse: "survie".into() });
    assert!(matches!(demander(agents, &libre("survie", &b)).await.0, DuRelais::Ok { .. }));
}

#[tokio::test]
async fn ancienne_version_sans_machines() {
    let (_, agents) = relais().await;
    let mut f = BufReader::new(TcpStream::connect(agents).await.unwrap());
    f.get_mut().write_all(format!("BHOST 1\nLIER {}\n", e("K7M2Q9XR")).as_bytes()).await.unwrap();
    let mut ligne = String::new();
    timeout(Duration::from_secs(5), f.read_line(&mut ligne)).await.unwrap().unwrap();
    assert_eq!(protocole::lire_du_relais(&ligne), Some(DuRelais::Non { raison: "version".into() }));
}

/// Un joueur se connecte (poignée de main + un mot) et attend l'écho : vrai s'il est passé.
async fn entrer(joueurs: SocketAddr, hote: &str) -> (bool, TcpStream) {
    let mut j = TcpStream::connect(joueurs).await.unwrap();
    let mut envoi = poignee(hote, 2);
    envoi.extend_from_slice(b"salut");
    j.write_all(&envoi).await.unwrap();
    let mut recu = vec![0u8; envoi.len()];
    let passe = matches!(timeout(Duration::from_secs(5), j.read_exact(&mut recu)).await, Ok(Ok(_))) && recu == envoi;
    (passe, j)
}

#[tokio::test]
async fn relais_plein_les_serveurs_bhost_plus_passent_encore() {
    // 5 places : au-delà de 4 (80 %), seuls les serveurs BHOST+ font entrer leurs joueurs.
    let plus = SigningKey::from_bytes(&[42; 32]);
    let local = "127.0.0.1:0".parse().unwrap();
    let mut reglages = Reglages::simples(DOMAINE, local, local, None);
    reglages.joueurs_max = 5;
    reglages.plus_cle = Some(protocole::hexa(plus.verifying_key().as_bytes()));
    let (joueurs, agents, relais) = demarrer(reglages).await.unwrap();
    let serveur = echo().await;

    // « gratuit » sans jeton ; « abonne » avec un jeton signé par le service BHOST+ ; « faux » avec un jeton d'une autre clé.
    let gratuit = bhost(agents, "gratuit").await;
    servir(gratuit, agents, serveur);
    let (ok, mut abonne) = presenter(agents, "abonne", 8, false).await;
    assert!(matches!(ok, DuRelais::Ok { .. }));
    let cle_abonne = protocole::hexa(SigningKey::from_bytes(&[8; 32]).verifying_key().as_bytes());
    let expire = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
    let signature = protocole::hexa(&plus.sign(protocole::a_signer_priorite(&cle_abonne, expire).as_bytes()).to_bytes());
    abonne.get_mut().write_all(protocole::ecrire_de_bhost(&DeBhost::Priorite { expire, signature }).as_bytes()).await.unwrap();
    servir(abonne, agents, serveur);
    let (ok, mut faux) = presenter(agents, "faux", 9, false).await;
    assert!(matches!(ok, DuRelais::Ok { .. }));
    let cle_faux = protocole::hexa(SigningKey::from_bytes(&[9; 32]).verifying_key().as_bytes());
    let signature = protocole::hexa(&SigningKey::from_bytes(&[1; 32]).sign(protocole::a_signer_priorite(&cle_faux, expire).as_bytes()).to_bytes());
    faux.get_mut().write_all(protocole::ecrire_de_bhost(&DeBhost::Priorite { expire, signature }).as_bytes()).await.unwrap();
    servir(faux, agents, serveur);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut gardes = Vec::new();
    for _ in 0..4 {
        let (passe, j) = entrer(joueurs, "gratuit.bhost.test").await;
        assert!(passe);
        gardes.push(j);
    }
    assert_eq!(relais.relayes(), 4);
    // Plein pour les serveurs gratuits (et pour un jeton mal signé) : écran « Déconnecté ».
    for hote in ["gratuit.bhost.test", "faux.bhost.test"] {
        let mut j = TcpStream::connect(joueurs).await.unwrap();
        j.write_all(&poignee(hote, 2)).await.unwrap();
        let mut recu = Vec::new();
        timeout(Duration::from_secs(5), j.read_to_end(&mut recu)).await.unwrap().unwrap();
        assert_eq!(recu, protocole::refus_connexion("Le relais BHOST est plein. Réessaie dans quelques minutes."));
    }
    // Le serveur BHOST+ passe encore.
    let (passe, j) = entrer(joueurs, "abonne.bhost.test").await;
    assert!(passe, "le serveur BHOST+ garde ses places");
    gardes.push(j);
    // Deux joueurs partent : on repasse sous 80 %, les serveurs gratuits refont entrer.
    gardes.drain(..2);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(relais.relayes(), 3);
    assert!(entrer(joueurs, "gratuit.bhost.test").await.0);
}
