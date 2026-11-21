// Le relais BHOST : ce que le relais et BHOST se disent, et ce qu'on lit dans le premier paquet de Minecraft.
// Ce fichier est partagé : le relais (relais/src/main.rs) et BHOST (src-tauri/src/tunnel.rs, par #[path]).
// Il n'utilise que la bibliothèque standard de Rust, et se teste tout seul (cargo test).
//
// Comment ça marche :
//   1. BHOST ouvre une connexion « agent » vers le relais et se présente : son nom (« survie-k3f9 »),
//      sa clé publique, puis il signe un défi. Le premier qui prend un nom le garde (sa clé).
//   2. Un joueur tape « survie-k3f9.bhost.app ». Le DNS (*.bhost.app) mène au relais, port 25565.
//      Minecraft envoie d'abord une « poignée de main » qui contient l'adresse tapée : le relais y lit le nom.
//   3. Le relais dit à l'agent : « JOUEUR 17 <jeton> ». BHOST ouvre une 2ᵉ connexion « DONNEES 17 <jeton> »,
//      se branche sur le serveur Minecraft du PC, et le relais recolle les deux bouts.
//   4. Machines dédiées (BHOST OS) : la machine se présente avec « MACHINE <clé> <code> » et signe un défi.
//      Un PC qui tape ce code envoie « LIER <code> » : le relais répond « LIEE <id> » (l'id de la machine)
//      ou « NON code ». Après 5 codes faux, l'adresse IP attend 15 minutes (« NON bloque »).

/// La version du dialogue entre BHOST et le relais.
pub const VERSION: u32 = 1;

/// Les noms qu'on ne peut pas prendre (le site et les services de bhost.app, et des noms qui trompent).
pub const RESERVES: [&str; 46] = [
    "www", "api", "app", "mail", "smtp", "imap", "pop", "relais", "relay", "status", "statut", "docs", "doc", "blog", "aide", "ns", "ns1",
    "ns2", "ns3", "play", "jouer", "localhost", "ftp", "cdn", "static", "auth", "login", "compte", "account", "secure", "official", "minecraft",
    "mojang", "microsoft", "hypixel", "autodiscover", "autoconfig", "wpad", "mta-sts", "dev", "test", "staging", "beta", "shop", "store",
    "boutique",
];

/// Un nom qui contient l'un de ces mots se ferait passer pour BHOST : refusé.
const MOTS_INTERDITS: [&str; 5] = ["bhost", "admin", "support", "officiel", "moderat"];

/// Un nom de serveur sur le relais : 3 à 32 caractères, lettres minuscules, chiffres et tirets (pas au bord).
pub fn nom_valide(nom: &str) -> bool {
    let n = nom.len();
    (3..=32).contains(&n)
        && nom.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && !nom.starts_with('-')
        && !nom.ends_with('-')
        && !nom.contains("--")
        && !RESERVES.contains(&nom)
        && !MOTS_INTERDITS.iter().any(|m| nom.contains(m))
}

/// « Survie entre amis ! » → « survie-entre-amis » (au plus 20 caractères, « serveur » si rien ne reste).
pub fn slug(texte: &str) -> String {
    let mut s = String::new();
    for c in texte.chars().flat_map(|c| c.to_lowercase()) {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            'ÿ' => 'y',
            'œ' => 'o',
            'æ' => 'a',
            c => c,
        };
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            s.push(c);
        } else if !s.is_empty() && !s.ends_with('-') {
            s.push('-');
        }
        if s.len() >= 20 {
            break;
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.len() < 2 { "serveur".into() } else { s }
}

/// Le nom dans une adresse tapée par le joueur : « Survie-K3F9.bhost.app. » → « survie-k3f9 ».
/// Forge ajoute « \0FML3\0 » après l'adresse : on l'enlève.
pub fn sous_domaine(hote: &str, domaine: &str) -> Option<String> {
    let hote = hote.split('\0').next()?.trim().trim_end_matches('.').to_ascii_lowercase();
    let fin = format!(".{}", domaine.trim_matches('.').to_ascii_lowercase());
    let nom = hote.strip_suffix(&fin)?;
    nom_valide(nom).then(|| nom.to_string())
}

/// Le code d'une machine dédiée : 8 caractères, sans 0, O, 1 ni I (comme src/code.ts).
pub const ALPHABET_CODE: &str = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

pub fn code_valide(code: &str) -> bool {
    code.len() == 8 && code.chars().all(|c| ALPHABET_CODE.contains(c))
}

/// L'id d'une machine : le début de sa clé publique (16 caractères hexadécimaux).
pub fn id_machine(cle: &str) -> String {
    cle.chars().take(16).collect()
}

/// Le texte qu'une machine signe pour prouver que la clé est à elle.
pub fn a_signer_machine(code: &str, defi: &str) -> String {
    format!("bhost-machine-v{VERSION}|{code}|{defi}")
}

/// Le texte que BHOST signe pour prouver que le nom est à lui.
pub fn a_signer(nom: &str, defi: &str) -> String {
    format!("bhost-relais-v{VERSION}|{nom}|{defi}")
}

// ---------------------------------------------------------------------------------------------
// Les nombres de Minecraft (VarInt) et la poignée de main
// ---------------------------------------------------------------------------------------------

/// Lit un VarInt au début de `octets` : (valeur, octets lus). None s'il manque des octets ; Err s'il est trop long.
#[allow(clippy::result_unit_err)]
pub fn lire_varint(octets: &[u8]) -> Result<Option<(i32, usize)>, ()> {
    let mut valeur: u32 = 0;
    for i in 0..5 {
        let Some(&o) = octets.get(i) else { return Ok(None) };
        valeur |= u32::from(o & 0x7f) << (7 * i);
        if o & 0x80 == 0 {
            return Ok(Some((valeur as i32, i + 1)));
        }
    }
    Err(())
}

pub fn ecrire_varint(sortie: &mut Vec<u8>, valeur: i32) {
    let mut v = valeur as u32;
    loop {
        if v & !0x7f == 0 {
            sortie.push(v as u8);
            return;
        }
        sortie.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
}

/// Ce qu'on lit dans le premier paquet d'un joueur.
#[derive(Debug, Clone, PartialEq)]
pub enum Poignee {
    /// Il manque des octets : lire encore.
    Incomplete,
    /// Pas du Minecraft (ou trop gros) : couper.
    Invalide,
    /// Très vieux client (avant 1.7, paquet 0xFE) : pas pris en charge.
    Ancienne,
    Lue {
        /// L'adresse tapée par le joueur.
        hote: String,
        /// La version du jeu du joueur (numéro de protocole).
        protocole: i32,
        /// 1 = il regarde la liste des serveurs ; 2 (ou 3) = il veut entrer.
        suite: i32,
        /// La taille du paquet (pour savoir où commence la suite).
        taille: usize,
    },
}

/// Taille maximale de la poignée de main (adresse de 255 caractères au plus, plus quelques champs).
pub const POIGNEE_MAX: usize = 1100;

pub fn lire_poignee(octets: &[u8]) -> Poignee {
    if octets.first() == Some(&0xFE) {
        return Poignee::Ancienne;
    }
    let (longueur, n) = match lire_varint(octets) {
        Ok(Some(x)) => x,
        Ok(None) => return Poignee::Incomplete,
        Err(()) => return Poignee::Invalide,
    };
    if longueur <= 0 || longueur as usize + n > POIGNEE_MAX {
        return Poignee::Invalide;
    }
    // Le premier paquet est toujours la poignée de main (identifiant 0) : sinon, pas la peine d'attendre la suite.
    if octets.get(n).is_some_and(|&id| id != 0) {
        return Poignee::Invalide;
    }
    let fin = n + longueur as usize;
    if octets.len() < fin {
        return Poignee::Incomplete;
    }
    lire_contenu(&octets[n..fin], fin).unwrap_or(Poignee::Invalide)
}

/// Le contenu de la poignée de main : identifiant 0, protocole, adresse, port, suite.
fn lire_contenu(p: &[u8], taille: usize) -> Option<Poignee> {
    let mut i = 0;
    let mut varint = || -> Option<i32> {
        let (v, k) = lire_varint(&p[i..]).ok()??;
        i += k;
        Some(v)
    };
    if varint()? != 0 {
        return None;
    }
    let protocole = varint()?;
    let taille_hote = usize::try_from(varint()?).ok()?;
    if taille_hote > 1020 || i + taille_hote + 2 > p.len() {
        return None;
    }
    let hote = std::str::from_utf8(&p[i..i + taille_hote]).ok()?.to_string();
    i += taille_hote + 2; // + le port (2 octets)
    let (suite, k) = lire_varint(&p[i..]).ok()??;
    i += k;
    (i == p.len() && (1..=3).contains(&suite)).then_some(Poignee::Lue { hote, protocole, suite, taille })
}

/// Un paquet Minecraft : longueur, identifiant, contenu.
fn paquet(id: i32, contenu: &[u8]) -> Vec<u8> {
    let mut corps = Vec::new();
    ecrire_varint(&mut corps, id);
    corps.extend_from_slice(contenu);
    let mut p = Vec::new();
    ecrire_varint(&mut p, corps.len() as i32);
    p.extend(corps);
    p
}

fn chaine(texte: &str) -> Vec<u8> {
    let mut v = Vec::new();
    ecrire_varint(&mut v, texte.len() as i32);
    v.extend_from_slice(texte.as_bytes());
    v
}

/// Échappe un texte pour le mettre entre guillemets dans du JSON.
pub fn json_texte(texte: &str) -> String {
    let mut s = String::from("\"");
    for c in texte.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            c if (c as u32) < 0x20 => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// La réponse à la liste des serveurs quand le serveur n'est pas là (éteint, ou nom inconnu).
pub fn statut_absent(protocole: i32, message: &str) -> Vec<u8> {
    let json = format!(
        r#"{{"version":{{"name":"BHOST","protocol":{protocole}}},"players":{{"max":0,"online":0}},"description":{{"text":{}}}}}"#,
        json_texte(message)
    );
    paquet(0x00, &chaine(&json))
}

/// La réponse au « ping » de la liste des serveurs (on renvoie les mêmes 8 octets).
pub fn pong(contenu: &[u8; 8]) -> Vec<u8> {
    paquet(0x01, contenu)
}

/// « Déconnecté » pendant la connexion (écran rouge du jeu) : le serveur n'est pas là.
pub fn refus_connexion(message: &str) -> Vec<u8> {
    paquet(0x00, &chaine(&format!(r#"{{"text":{}}}"#, json_texte(message))))
}

/// Lit le paquet « ping » (identifiant 1, 8 octets) envoyé après la demande de statut.
pub fn lire_ping(octets: &[u8]) -> Option<[u8; 8]> {
    let (longueur, n) = lire_varint(octets).ok()??;
    if longueur != 9 || octets.len() < n + 9 || octets[n] != 0x01 {
        return None;
    }
    octets[n + 1..n + 9].try_into().ok()
}

// ---------------------------------------------------------------------------------------------
// Les lignes entre BHOST et le relais (texte, une commande par ligne)
// ---------------------------------------------------------------------------------------------

/// Ce que BHOST envoie au relais (la première ligne de chaque connexion est « BHOST 1 »).
#[derive(Debug, Clone, PartialEq)]
pub enum DeBhost {
    /// Connexion agent : le nom voulu et la clé publique (hexadécimal, 32 octets).
    Bonjour { nom: String, cle: String },
    /// La signature du défi (hexadécimal, 64 octets).
    Signe { signature: String },
    /// Toutes les 20 secondes, pour garder la connexion.
    Ping,
    /// Connexion données : le joueur `id` attendu par le relais.
    Donnees { id: u64, jeton: String },
    /// Une machine dédiée se présente : sa clé publique et son code (puis elle signe un défi).
    Machine { cle: String, code: String },
    /// Un PC veut relier la machine qui a ce code.
    Lier { code: String },
}

/// Ce que le relais répond.
#[derive(Debug, Clone, PartialEq)]
pub enum DuRelais {
    Defi { defi: String },
    /// Le nom est à toi : l'adresse complète à donner aux amis.
    Ok { adresse: String },
    /// Refus : la raison (« pris », « nom », « trop », « version »).
    Non { raison: String },
    Pong,
    /// Un joueur arrive : ouvre une connexion « DONNEES ».
    Joueur { id: u64, jeton: String },
    /// Le code est bon : l'id de la machine.
    Liee { id: String },
}

fn hexa_valide(texte: &str, longueur: usize) -> bool {
    texte.len() == longueur && texte.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn ecrire_de_bhost(m: &DeBhost) -> String {
    match m {
        DeBhost::Bonjour { nom, cle } => format!("BONJOUR {nom} {cle}\n"),
        DeBhost::Signe { signature } => format!("SIGNE {signature}\n"),
        DeBhost::Ping => "PING\n".into(),
        DeBhost::Donnees { id, jeton } => format!("DONNEES {id} {jeton}\n"),
        DeBhost::Machine { cle, code } => format!("MACHINE {cle} {code}\n"),
        DeBhost::Lier { code } => format!("LIER {code}\n"),
    }
}

pub fn lire_de_bhost(ligne: &str) -> Option<DeBhost> {
    let mots: Vec<&str> = ligne.trim_end().split(' ').collect();
    match mots.as_slice() {
        ["BONJOUR", nom, cle] if nom_valide(nom) && hexa_valide(cle, 64) => Some(DeBhost::Bonjour { nom: nom.to_string(), cle: cle.to_ascii_lowercase() }),
        ["SIGNE", s] if hexa_valide(s, 128) => Some(DeBhost::Signe { signature: s.to_ascii_lowercase() }),
        ["PING"] => Some(DeBhost::Ping),
        ["DONNEES", id, jeton] if hexa_valide(jeton, 32) => Some(DeBhost::Donnees { id: id.parse().ok()?, jeton: jeton.to_ascii_lowercase() }),
        ["MACHINE", cle, code] if hexa_valide(cle, 64) && code_valide(code) => Some(DeBhost::Machine { cle: cle.to_ascii_lowercase(), code: code.to_string() }),
        ["LIER", code] if code_valide(code) => Some(DeBhost::Lier { code: code.to_string() }),
        _ => None,
    }
}

pub fn ecrire_du_relais(m: &DuRelais) -> String {
    match m {
        DuRelais::Defi { defi } => format!("DEFI {defi}\n"),
        DuRelais::Ok { adresse } => format!("OK {adresse}\n"),
        DuRelais::Non { raison } => format!("NON {raison}\n"),
        DuRelais::Pong => "PONG\n".into(),
        DuRelais::Joueur { id, jeton } => format!("JOUEUR {id} {jeton}\n"),
        DuRelais::Liee { id } => format!("LIEE {id}\n"),
    }
}

pub fn lire_du_relais(ligne: &str) -> Option<DuRelais> {
    let mots: Vec<&str> = ligne.trim_end().split(' ').collect();
    match mots.as_slice() {
        ["DEFI", d] if hexa_valide(d, 32) => Some(DuRelais::Defi { defi: d.to_ascii_lowercase() }),
        ["OK", a] if a.len() < 100 => Some(DuRelais::Ok { adresse: a.to_string() }),
        ["NON", r] if r.len() < 20 => Some(DuRelais::Non { raison: r.to_string() }),
        ["PONG"] => Some(DuRelais::Pong),
        ["JOUEUR", id, jeton] if hexa_valide(jeton, 32) => Some(DuRelais::Joueur { id: id.parse().ok()?, jeton: jeton.to_ascii_lowercase() }),
        ["LIEE", id] if hexa_valide(id, 16) => Some(DuRelais::Liee { id: id.to_ascii_lowercase() }),
        _ => None,
    }
}

/// Octets → hexadécimal.
pub fn hexa(octets: &[u8]) -> String {
    octets.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hexadécimal → octets.
pub fn depuis_hexa(texte: &str) -> Option<Vec<u8>> {
    if !texte.len().is_multiple_of(2) || !texte.bytes().all(|o| o.is_ascii_hexdigit()) {
        return None;
    }
    (0..texte.len()).step_by(2).map(|i| u8::from_str_radix(texte.get(i..i + 2)?, 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une poignée de main comme celle du jeu.
    fn poignee(hote: &str, protocole: i32, suite: i32) -> Vec<u8> {
        let mut c = Vec::new();
        ecrire_varint(&mut c, protocole);
        c.extend(chaine(hote));
        c.extend(25565u16.to_be_bytes());
        ecrire_varint(&mut c, suite);
        paquet(0x00, &c)
    }

    #[test]
    fn varint_aller_retour() {
        for v in [0, 1, 127, 128, 255, 25565, 2097151, 769, i32::MAX, -1] {
            let mut o = Vec::new();
            ecrire_varint(&mut o, v);
            assert_eq!(lire_varint(&o), Ok(Some((v, o.len()))));
        }
        assert_eq!(lire_varint(&[0x80]), Ok(None));
        assert_eq!(lire_varint(&[0xff, 0xff, 0xff, 0xff, 0xff, 0x01]), Err(()));
    }

    #[test]
    fn lit_la_poignee_de_main() {
        let p = poignee("Survie-K3F9.bhost.app", 769, 2);
        match lire_poignee(&p) {
            Poignee::Lue { hote, protocole, suite, taille } => {
                assert_eq!((hote.as_str(), protocole, suite, taille), ("Survie-K3F9.bhost.app", 769, 2, p.len()));
                assert_eq!(sous_domaine(&hote, "bhost.app").as_deref(), Some("survie-k3f9"));
            }
            autre => panic!("{autre:?}"),
        }
        // Les octets suivants (la suite de la connexion) ne gênent pas.
        let mut avec_suite = p.clone();
        avec_suite.extend([5, 0, 3, 0x41, 0x42, 0x43]);
        assert!(matches!(lire_poignee(&avec_suite), Poignee::Lue { taille, .. } if taille == p.len()));
        // Coupée en route : il faut lire encore.
        assert_eq!(lire_poignee(&p[..p.len() - 1]), Poignee::Incomplete);
        assert_eq!(lire_poignee(&[]), Poignee::Incomplete);
        assert_eq!(lire_poignee(&[0xFE, 0x01]), Poignee::Ancienne);
        assert_eq!(lire_poignee(&[0x02, 0x05, 0x00]), Poignee::Invalide);
        assert_eq!(lire_poignee(&[0xff, 0xff, 0x7f]), Poignee::Invalide);
        assert_eq!(lire_poignee(b"GET / HTTP/1.1\r\n"), Poignee::Invalide);
        assert_eq!(lire_poignee(&poignee("a.bhost.app", 769, 7)), Poignee::Invalide);
    }

    #[test]
    fn lit_le_nom_dans_l_adresse() {
        assert_eq!(sous_domaine("survie-k3f9.bhost.app", "bhost.app").as_deref(), Some("survie-k3f9"));
        assert_eq!(sous_domaine("survie-k3f9.bhost.app.", "bhost.app").as_deref(), Some("survie-k3f9"));
        assert_eq!(sous_domaine("survie-k3f9.bhost.app\0FML3\0", "bhost.app").as_deref(), Some("survie-k3f9"));
        assert_eq!(sous_domaine("bhost.app", "bhost.app"), None);
        assert_eq!(sous_domaine("www.bhost.app", "bhost.app"), None);
        assert_eq!(sous_domaine("a.b.bhost.app", "bhost.app"), None);
        assert_eq!(sous_domaine("survie.evil.com", "bhost.app"), None);
        assert_eq!(sous_domaine("82.64.1.2", "bhost.app"), None);
    }

    #[test]
    fn noms_et_slugs() {
        assert!(nom_valide("survie-k3f9") && nom_valide("abc"));
        assert!(!nom_valide("ab") && !nom_valide("-abc") && !nom_valide("abc-") && !nom_valide("a--b") && !nom_valide("Abc") && !nom_valide("www"));
        assert!(!nom_valide(&"a".repeat(33)));
        // Des noms qui se feraient passer pour BHOST ou un service connu.
        assert!(!nom_valide("bhost-officiel") && !nom_valide("support-x") && !nom_valide("minecraft") && !nom_valide("mon-admin"));
        assert_eq!(depuis_hexa("+f"), None);
        assert_eq!(slug("Survie entre amis !"), "survie-entre-amis");
        assert_eq!(slug("Créatif d'été 2026"), "creatif-d-ete-2026");
        assert_eq!(slug("!!!"), "serveur");
        assert_eq!(slug("Le très très très long nom de serveur"), "le-tres-tres-tres-lo");
    }

    #[test]
    fn paquets_de_reponse() {
        let s = statut_absent(769, "Ce serveur est \"éteint\".");
        let (longueur, n) = lire_varint(&s).unwrap().unwrap();
        assert_eq!(longueur as usize + n, s.len());
        assert_eq!(s[n], 0x00);
        let texte = String::from_utf8_lossy(&s);
        assert!(texte.contains(r#""protocol":769"#) && texte.contains(r#"\"éteint\""#));
        let mut ping = Vec::new();
        ping.extend([9, 0x01]);
        ping.extend(42u64.to_be_bytes());
        let contenu = lire_ping(&ping).unwrap();
        assert_eq!(pong(&contenu), ping);
        assert_eq!(lire_ping(&[9, 0x00, 1, 2, 3, 4, 5, 6, 7, 8]), None);
        assert!(String::from_utf8_lossy(&refus_connexion("Éteint")).contains(r#"{"text":"Éteint"}"#));
    }

    #[test]
    fn lignes_aller_retour() {
        let cle = "ab".repeat(32);
        for m in [
            DeBhost::Bonjour { nom: "survie-k3f9".into(), cle: cle.clone() },
            DeBhost::Signe { signature: "cd".repeat(64) },
            DeBhost::Ping,
            DeBhost::Donnees { id: 17, jeton: "0f".repeat(16) },
            DeBhost::Machine { cle: cle.clone(), code: "K7M2Q9XR".into() },
            DeBhost::Lier { code: "K7M2Q9XR".into() },
        ] {
            assert_eq!(lire_de_bhost(&ecrire_de_bhost(&m)), Some(m));
        }
        for m in [
            DuRelais::Defi { defi: "12".repeat(16) },
            DuRelais::Ok { adresse: "survie-k3f9.bhost.app".into() },
            DuRelais::Non { raison: "pris".into() },
            DuRelais::Pong,
            DuRelais::Joueur { id: 3, jeton: "aa".repeat(16) },
            DuRelais::Liee { id: "ab".repeat(8) },
        ] {
            assert_eq!(lire_du_relais(&ecrire_du_relais(&m)), Some(m));
        }
        assert_eq!(lire_de_bhost("BONJOUR www abc"), None);
        assert_eq!(lire_de_bhost("DONNEES x 00"), None);
        assert_eq!(lire_du_relais("JOUEUR 1 pas-un-jeton"), None);
        assert_eq!(depuis_hexa(&hexa(&[0, 1, 255])), Some(vec![0, 1, 255]));
        assert_eq!(depuis_hexa("zz"), None);
        assert_eq!(a_signer("survie", "ab"), "bhost-relais-v1|survie|ab");
        // Codes de machine : 8 caractères, ni 0, O, 1, I, ni minuscules.
        assert_eq!(lire_de_bhost("LIER K0M2Q9XR"), None);
        assert_eq!(lire_de_bhost("LIER k7m2q9xr"), None);
        assert_eq!(lire_de_bhost("LIER K7M2Q9X"), None);
        assert_eq!(id_machine(&cle), "abababababababab");
    }
}
