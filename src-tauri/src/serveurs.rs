// La liste des serveurs de BHOST : la reconnaître dans un dossier, la garder sur le disque.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Serveur {
    pub id: String,
    pub nom: String,
    pub dossier: String,
    /// Nom du fichier .jar à lancer, dans le dossier.
    pub jar: String,
    /// Vanilla, Paper, Purpur, Spigot, Fabric, Forge, NeoForge, Youer, Arclight ou Mohist.
    pub logiciel: String,
    /// Version de Minecraft. Vide si on n'a pas pu la lire.
    pub version: String,
    /// Mémoire donnée au serveur, en Mo.
    pub ram_mo: u32,
    /// L'icône du serveur (clic droit › Icône…), une clé de ICONES. Le nom « couleur » vient d'avant les icônes.
    /// Absente : une couleur selon le logiciel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub couleur: Option<String>,
    /// Le Java à utiliser quand la version ne le dit pas (snapshots). Absent : selon la version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java: Option<u32>,
    /// Paramètres › Avancé › Version de Java : absent = automatique ; « 8 », « 17 », « 21 », « 25 » ;
    /// sinon le chemin d'un java.exe choisi par l'utilisateur.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_choix: Option<String>,
    /// Paramètres › Général › Démarrer avec BHOST.
    #[serde(default, skip_serializing_if = "faux")]
    pub demarrer_auto: bool,
    /// Paramètres › Général › Ouvert à tes amis : BHOST ouvre le port sur la box (acces.rs).
    #[serde(default, skip_serializing_if = "faux")]
    pub ouvert_amis: bool,
    /// Paramètres › Avancé › Réglages de démarrage optimisés (options de Java d'Aikar, sans réserver la mémoire d'avance).
    #[serde(default, skip_serializing_if = "faux")]
    pub optimise: bool,
    /// Paramètres › Joueurs Bedrock : le port de Geyser pour ce serveur (19132, ou un suivant s'il est pris).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bedrock_port: Option<u16>,
    /// Paramètres › Avancé › Arguments en plus, donnés à Java avant « -jar ».
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    /// Paramètres › Règles du jeu changées quand le serveur était arrêté : envoyées à son prochain démarrage.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub regles_attente: BTreeMap<String, i64>,
    /// Onglet Sauvegardes : réglages de la sauvegarde automatique.
    #[serde(default)]
    pub sauvegarde: ReglagesSauvegarde,
    /// Onglet Sauvegardes : les archives épinglées (jamais effacées par le nettoyage).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub epinglees: Vec<String>,
    /// Onglet Automatisations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automatisations: Vec<crate::automatisations::Automatisation>,
}

/// La sauvegarde automatique d'un serveur.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReglagesSauvegarde {
    pub auto: bool,
    /// « 1h », « 6h » ou « 24h ».
    pub frequence: String,
    /// Combien de sauvegardes automatiques garder.
    pub garder: u32,
    /// Sauvegarder avant chaque changement de version.
    pub avant_maj: bool,
}

impl Default for ReglagesSauvegarde {
    fn default() -> Self {
        ReglagesSauvegarde { auto: true, frequence: "6h".into(), garder: 10, avant_maj: true }
    }
}

impl ReglagesSauvegarde {
    /// L'intervalle entre deux sauvegardes automatiques, en millisecondes.
    pub fn intervalle_ms(&self) -> u64 {
        match self.frequence.as_str() {
            "1h" => 3_600_000,
            "24h" => 86_400_000,
            _ => 6 * 3_600_000,
        }
    }
}

fn faux(b: &bool) -> bool {
    !*b
}

/// Les icônes qu'on peut choisir pour un serveur (les mêmes que dans src/theme/icones.ts).
/// Les 10 premières sont les anciennes couleurs : les serveurs déjà créés gardent la leur.
pub const ICONES: [&str; 44] = [
    "graphite", "bleu", "indigo", "violet", "rose", "rouge", "orange", "jaune", "vert", "menthe", "serveur-acier", "serveur-dore", "serveur-titane", "serveur-noir", "serveur-orrose", "cadeau-rouge", "cadeau-bleu", "cadeau-vert", "gateau", "gateau-chocolat", "fromage", "pain", "guimauve", "bento", "carton", "de", "camera", "aquarium", "terminal", "damier", "caisse", "herbe", "hiver", "grenouille",
    "numero-0", "numero-1", "numero-2", "numero-3", "numero-4", "numero-5", "numero-6", "numero-7", "numero-8", "numero-9",
];

/// Vérifie une icône avant de l'enregistrer.
pub fn icone_valide(icone: &str) -> bool {
    ICONES.contains(&icone)
}

/// Avant de mettre un dossier à la Corbeille : il faut que ce soit bien un dossier de serveur
/// (jamais un disque entier ni un dossier quelconque).
pub fn dossier_de_serveur(serveur: &Serveur) -> bool {
    let d = Path::new(&serveur.dossier);
    d.is_absolute()
        && d.parent().and_then(|p| p.parent()).is_some()
        && d.is_dir()
        && !dossier_personnel(d)
        // Un vrai serveur a toujours l'un de ces deux fichiers (un simple .jar ne suffit pas).
        && ["server.properties", "eula.txt"].iter().any(|f| d.join(f).is_file())
}

/// Le dossier personnel, un de ses parents, ou un dossier connu (Bureau, Téléchargements, Documents…) :
/// jamais mis à la Corbeille d'un coup, même importé par erreur comme serveur.
fn dossier_personnel(d: &Path) -> bool {
    let Some(maison) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(std::path::PathBuf::from) else { return false };
    const CONNUS: [&str; 9] = ["Desktop", "Bureau", "Downloads", "Téléchargements", "Documents", "Pictures", "Music", "Videos", "OneDrive"];
    let egal = |a: &Path, b: &Path| a.to_string_lossy().trim_end_matches(['/', '\\']).eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['/', '\\']));
    maison.ancestors().any(|a| egal(a, d)) || CONNUS.iter().any(|n| egal(&maison.join(n), d))
}

const RAM_PAR_DEFAUT: u32 = 2048;

/// Lit un fichier texte sans planter si l'encodage n'est pas de l'UTF-8
/// (les anciens journaux de Windows ne le sont pas toujours).
fn lire_texte(fichier: &Path) -> String {
    // Le début seulement (1 Mo) : un journal de plusieurs Go ne doit pas remplir la mémoire.
    crate::garde::lire_debut(fichier, 1024 * 1024).map(|octets| String::from_utf8_lossy(&octets).into_owned()).unwrap_or_default()
}

/// Cherche « -jar quelquechose.jar » et « -Xmx4G » dans une commande de lancement.
fn lire_commande(texte: &str) -> (Option<String>, Option<u32>) {
    let mut jar = None;
    let mut ram = None;
    let mots: Vec<&str> = texte.split_whitespace().collect();
    for (i, mot) in mots.iter().enumerate() {
        if *mot == "-jar" {
            if let Some(suivant) = mots.get(i + 1) {
                jar = Some(suivant.trim_matches('"').to_string());
            }
        }
        // Forge et NeoForge récents : « @libraries/…/win_args.txt » à la place de « -jar ».
        let mot_propre = mot.trim_matches('"');
        let args_du_systeme = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
        if mot_propre.starts_with("@libraries/") && mot_propre.ends_with(args_du_systeme) {
            jar = Some(mot_propre.to_string());
        }
        if let Some(valeur) = mot.strip_prefix("-Xmx") {
            let chiffres: String = valeur.chars().take_while(|c| c.is_ascii_digit()).collect();
            let unite = valeur[chiffres.len()..].to_ascii_uppercase();
            if let Ok(n) = chiffres.parse::<u32>() {
                ram = match unite.as_str() {
                    "G" => Some(n.saturating_mul(1024)),
                    "M" => Some(n),
                    _ => None,
                };
            }
        }
    }
    (jar, ram)
}

/// Les fichiers du dossier (pas les sous-dossiers), avec leur taille.
fn fichiers(dossier: &Path) -> Vec<(String, u64)> {
    let Ok(lecture) = fs::read_dir(dossier) else {
        return Vec::new();
    };
    lecture
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            let taille = e.metadata().map(|m| m.len()).unwrap_or(0);
            (e.file_name().to_string_lossy().into_owned(), taille)
        })
        .collect()
}

fn finit_par(nom: &str, fin: &str) -> bool {
    nom.to_ascii_lowercase().ends_with(fin)
}

/// Trouve le logiciel et la version dans le dernier journal du serveur.
fn lire_journal(texte: &str) -> (Option<&'static str>, Option<String>) {
    let mut logiciel = None;
    let mut version = None;
    for ligne in texte.lines().take(400) {
        if let Some(i) = ligne.find("Starting minecraft server version ") {
            version = Some(ligne[i + "Starting minecraft server version ".len()..].trim().to_string());
        }
        // Fabric : « Loading Minecraft 1.21.4 with Fabric Loader 0.16.9 »
        if let (Some(a), Some(b)) = (ligne.find("Loading Minecraft "), ligne.find(" with Fabric Loader")) {
            if a < b {
                version = Some(ligne[a + "Loading Minecraft ".len()..b].trim().to_string());
                logiciel = Some("Fabric");
            }
        }
        if logiciel.is_none() {
            // L'ordre compte : un serveur Purpur parle aussi de Paper, et Paper de Spigot.
            for (indice, nom) in [
                ("Youer", "Youer"),
                ("Arclight", "Arclight"),
                ("Mohist", "Mohist"),
                ("NeoForge", "NeoForge"),
                ("Purpur", "Purpur"),
                ("Paper", "Paper"),
                ("Spigot", "Spigot"),
                ("CraftBukkit", "Spigot"),
                ("MinecraftForge", "Forge"),
                ("Forge mod loading", "Forge"),
            ] {
                let parle_du_logiciel = ligne.contains("This server is running")
                    || ligne.contains("mod loading")
                    || ligne.contains("MinecraftForge");
                if parle_du_logiciel && ligne.contains(indice) {
                    logiciel = Some(nom);
                    break;
                }
            }
        }
    }
    (logiciel, version)
}

/// « @libraries/net/minecraftforge/forge/1.21.11-61.2.4/win_args.txt » → (Forge, 1.21.11).
/// « @libraries/net/neoforged/neoforge/21.1.267/win_args.txt » → (NeoForge, 1.21.1).
fn selon_arguments(lancement: &str) -> Option<(&'static str, String)> {
    let chemin = lancement.strip_prefix("@libraries/")?;
    if let Some(reste) = chemin.strip_prefix("net/minecraftforge/forge/") {
        return Some(("Forge", reste.split('/').next()?.split('-').next()?.to_string()));
    }
    let reste = chemin.strip_prefix("net/neoforged/neoforge/")?;
    let (mc, _) = crate::installeur::minecraft_de_neoforge(reste.split('/').next()?)?;
    Some(("NeoForge", mc))
}

/// Devine le logiciel d'après les fichiers de réglages présents.
fn logiciel_selon_fichiers(dossier: &Path) -> &'static str {
    let a = |chemin: &str| dossier.join(chemin).exists();
    if a("purpur.yml") {
        "Purpur"
    } else if a("paper.yml") || a("config/paper-global.yml") {
        "Paper"
    } else if a("spigot.yml") || a("bukkit.yml") {
        "Spigot"
    } else if a("fabric-server-launcher.properties") || a(".fabric") {
        "Fabric"
    } else {
        "Vanilla"
    }
}

/// Le port du serveur, lu dans server.properties (25565 si rien n'est écrit).
pub fn port(dossier: &Path) -> u16 {
    lire_texte(&dossier.join("server.properties"))
        .lines()
        .find_map(|ligne| ligne.trim().strip_prefix("server-port="))
        .and_then(|valeur| valeur.trim().parse().ok())
        .unwrap_or(25565)
}

/// Le nombre de joueurs maximum, lu dans server.properties (20 si rien n'est écrit).
pub fn joueurs_max(dossier: &Path) -> u32 {
    lire_texte(&dossier.join("server.properties"))
        .lines()
        .find_map(|ligne| ligne.trim().strip_prefix("max-players="))
        .and_then(|valeur| valeur.trim().parse().ok())
        .unwrap_or(20)
}

/// Toutes les lignes « clé=valeur » de server.properties (sans les commentaires).
pub fn lire_proprietes(dossier: &Path) -> Vec<(String, String)> {
    lire_texte(&dossier.join("server.properties"))
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(c, v)| (c.trim().to_string(), v.trim().to_string())))
        .collect()
}

/// Change quelques valeurs de server.properties sans toucher au reste (ordre, commentaires, autres clés).
/// Une clé absente est ajoutée à la fin.
pub fn modifier_proprietes(dossier: &Path, valeurs: &[(&str, String)]) -> Result<(), String> {
    let fichier = dossier.join("server.properties");
    // Java écrit ce fichier en Latin-1 : on le relit et le réécrit pareil, pour ne pas abîmer les accents.
    let (texte, latin1) = match String::from_utf8(fs::read(&fichier).unwrap_or_default()) {
        Ok(texte) => (texte, false),
        Err(e) => (e.into_bytes().iter().map(|&o| o as char).collect::<String>(), true),
    };
    let fin = if texte.contains("\r\n") { "\r\n" } else { "\n" };
    let mut restantes: Vec<&(&str, String)> = valeurs.iter().collect();
    let mut lignes: Vec<String> = texte
        .lines()
        .map(|ligne| {
            let cle = ligne.split_once('=').map(|(c, _)| c.trim()).filter(|_| !ligne.trim_start().starts_with('#'));
            match cle.and_then(|c| restantes.iter().position(|(k, _)| *k == c)) {
                Some(i) => {
                    let (k, v) = restantes.remove(i);
                    format!("{k}={v}")
                }
                None => ligne.to_string(),
            }
        })
        .collect();
    lignes.extend(restantes.into_iter().map(|(k, v)| format!("{k}={v}")));
    let texte = lignes.join(fin) + fin;
    let octets = if latin1 { texte.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect() } else { texte.into_bytes() };
    crate::garde::ecrire_atomique(&fichier, &octets).map_err(|e| format!("Impossible d'écrire server.properties. {e}"))
}

/// Donne un autre port au serveur (copie d'un serveur).
pub fn changer_port(dossier: &Path, port: u16) -> Result<(), String> {
    modifier_proprietes(dossier, &[("server-port", port.to_string())])
}

fn majuscule(nom: &str) -> String {
    let mut lettres = nom.chars();
    match lettres.next() {
        Some(premiere) => premiere.to_uppercase().collect::<String>() + lettres.as_str(),
        None => String::new(),
    }
}

/// Regarde un dossier et dit quel serveur il contient.
pub fn detecter(dossier: &Path) -> Result<Serveur, String> {
    if !dossier.is_dir() {
        return Err("Ce dossier n'existe pas.".into());
    }
    let liste = fichiers(dossier);

    // 1. La commande de lancement, si le dossier a un .bat ou un .cmd.
    let mut jar = None;
    let mut ram = None;
    for (nom, _) in &liste {
        if finit_par(nom, ".bat") || finit_par(nom, ".cmd") || finit_par(nom, ".sh") {
            let (j, r) = lire_commande(&lire_texte(&dossier.join(nom)));
            if let Some(j) = j {
                if dossier.join(j.trim_start_matches('@')).is_file() {
                    jar = Some(j);
                }
            }
            ram = ram.or(r);
        }
    }

    // 2. Sinon : server.jar, ou à défaut le plus gros .jar du dossier.
    if jar.is_none() {
        let mut jars: Vec<&(String, u64)> = liste.iter().filter(|(nom, _)| finit_par(nom, ".jar")).collect();
        jars.sort_by(|a, b| b.1.cmp(&a.1));
        jar = jars
            .iter()
            .find(|(nom, _)| nom.eq_ignore_ascii_case("server.jar"))
            .or(jars.first())
            .map(|(nom, _)| nom.clone());
    }
    let Some(jar) = jar else {
        return Err("Pas de serveur ici. Choisis le dossier qui contient le fichier .jar.".into());
    };

    let (logiciel, version) = lire_journal(&lire_texte(&dossier.join("logs").join("latest.log")));
    // Forge et NeoForge récents : la version est dans le chemin des arguments.
    let (logiciel, version) = match (logiciel, version, selon_arguments(&jar)) {
        (l, v, Some((l2, v2))) => (l.or(Some(l2)), v.or(Some(v2))),
        (l, v, None) => (l, v),
    };
    let nom = dossier
        .file_name()
        .map(|n| majuscule(&n.to_string_lossy()))
        .unwrap_or_else(|| "Serveur".into());

    Ok(Serveur {
        id: String::new(), // donné au moment de l'ajout à la liste
        nom,
        dossier: dossier.to_string_lossy().into_owned(),
        jar,
        logiciel: logiciel.unwrap_or_else(|| logiciel_selon_fichiers(dossier)).to_string(),
        version: version.unwrap_or_default(),
        ram_mo: ram.unwrap_or(RAM_PAR_DEFAUT),
        couleur: None,
        java: None,
        ..Default::default()
    })
}

/// Ajoute un serveur à la liste. Refuse un dossier déjà présent.
pub fn ajouter(liste: &mut Vec<Serveur>, mut serveur: Serveur) -> Result<Serveur, String> {
    let meme_dossier = |a: &str, b: &str| {
        a.trim_end_matches(['\\', '/']).eq_ignore_ascii_case(b.trim_end_matches(['\\', '/']))
    };
    if liste.iter().any(|s| meme_dossier(&s.dossier, &serveur.dossier)) {
        return Err("Ce serveur est déjà dans BHOST.".into());
    }
    // Un identifiant court et unique : s1, s2, s3…
    let mut n = liste.len() + 1;
    while liste.iter().any(|s| s.id == format!("s{n}")) {
        n += 1;
    }
    serveur.id = format!("s{n}");
    liste.push(serveur.clone());
    Ok(serveur)
}

/// Relit la liste gardée sur le disque. Liste vide si le fichier manque ou est abîmé.
pub fn charger(fichier: &Path) -> Vec<Serveur> {
    fs::read_to_string(fichier)
        .ok()
        .and_then(|texte| serde_json::from_str(&texte).ok())
        .unwrap_or_default()
}

/// Écrit la liste sur le disque.
pub fn enregistrer(fichier: &Path, liste: &[Serveur]) -> Result<(), String> {
    let texte = serde_json::to_string_pretty(liste).map_err(|e| e.to_string())?;
    crate::garde::ecrire_atomique(fichier, texte.as_bytes()).map_err(|_| "Impossible d'enregistrer la liste des serveurs.".to_string())
}

/// Le logiciel s'appelait Cubo : ses données étaient dans `%APPDATA%\\Cubo`.
/// Déplace ce dossier vers le nouveau (`%APPDATA%\\BHOST`) et corrige les chemins gardés dedans
/// (serveurs.json, reglages.json). Renvoie le dossier à utiliser : l'ancien si le déplacement échoue.
pub fn demenager(ancien: &Path, nouveau: &Path) -> PathBuf {
    if nouveau.exists() || !ancien.is_dir() {
        return nouveau.to_path_buf();
    }
    if fs::rename(ancien, nouveau).is_err() {
        return ancien.to_path_buf();
    }
    let corriger = |chemin: &str| -> Option<String> {
        let reste = Path::new(chemin).strip_prefix(ancien).ok()?;
        Some(nouveau.join(reste).to_string_lossy().into_owned())
    };
    let fichier = nouveau.join("serveurs.json");
    let mut liste = charger(&fichier);
    let mut change = false;
    for s in &mut liste {
        if let Some(c) = corriger(&s.dossier) {
            s.dossier = c;
            change = true;
        }
    }
    if change {
        let _ = enregistrer(&fichier, &liste);
    }
    let reglages = nouveau.join("reglages.json");
    if let Some(mut json) = fs::read(&reglages).ok().and_then(|o| serde_json::from_slice::<serde_json::Value>(&o).ok()) {
        if let Some(c) = json.get("dossierServeurs").and_then(|v| v.as_str()).and_then(corriger) {
            json["dossierServeurs"] = serde_json::Value::String(c);
            let _ = fs::write(&reglages, serde_json::to_string_pretty(&json).unwrap_or_default());
        }
    }
    nouveau.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Un dossier de test tout neuf, effacé à la fin du test.
    struct Dossier(PathBuf);
    impl Dossier {
        fn nouveau(nom: &str) -> Self {
            let chemin = std::env::temp_dir().join(format!("bhost-test-{nom}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&chemin);
            fs::create_dir_all(&chemin).unwrap();
            Dossier(chemin)
        }
        fn ecrire(&self, nom: &str, contenu: &[u8]) {
            let fichier = self.0.join(nom);
            fs::create_dir_all(fichier.parent().unwrap()).unwrap();
            fs::write(fichier, contenu).unwrap();
        }
    }
    impl Drop for Dossier {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn demenage_les_donnees_de_cubo() {
        let racine = Dossier::nouveau("demenage");
        let (ancien, nouveau) = (racine.0.join("Cubo"), racine.0.join("BHOST"));
        let a = |s: &str| ancien.join(s).to_string_lossy().into_owned();
        let liste = vec![
            Serveur { dossier: a("serveurs/Survie"), ..detecter_faux("s1") },
            Serveur { dossier: "D:\\Mes serveurs\\Spigot".into(), ..detecter_faux("s2") },
        ];
        fs::create_dir_all(ancien.join("serveurs/Survie")).unwrap();
        enregistrer(&ancien.join("serveurs.json"), &liste).unwrap();
        fs::write(ancien.join("reglages.json"), serde_json::json!({ "dossierServeurs": a("serveurs") }).to_string()).unwrap();

        assert_eq!(demenager(&ancien, &nouveau), nouveau);
        assert!(!ancien.exists() && nouveau.join("serveurs/Survie").is_dir());
        let liste = charger(&nouveau.join("serveurs.json"));
        assert_eq!(Path::new(&liste[0].dossier), nouveau.join("serveurs/Survie"));
        assert_eq!(liste[1].dossier, "D:\\Mes serveurs\\Spigot");
        let reglages: serde_json::Value = serde_json::from_slice(&fs::read(nouveau.join("reglages.json")).unwrap()).unwrap();
        assert_eq!(Path::new(reglages["dossierServeurs"].as_str().unwrap()), nouveau.join("serveurs"));
        // Une deuxième fois : rien à faire.
        assert_eq!(demenager(&ancien, &nouveau), nouveau);
    }

    #[test]
    fn modifie_server_properties_sans_rien_casser() {
        let d = Dossier::nouveau("proprietes");
        d.ecrire("server.properties", b"#Minecraft server properties\r\nmotd=Salut\r\ndifficulty=easy\r\nserver-port=25565\r\n");
        modifier_proprietes(&d.0, &[("difficulty", "hard".into()), ("max-players", "8".into())]).unwrap();
        let texte = fs::read_to_string(d.0.join("server.properties")).unwrap();
        assert_eq!(texte, "#Minecraft server properties\r\nmotd=Salut\r\ndifficulty=hard\r\nserver-port=25565\r\nmax-players=8\r\n");
        changer_port(&d.0, 25570).unwrap();
        assert_eq!(port(&d.0), 25570);
        assert!(lire_proprietes(&d.0).contains(&("motd".to_string(), "Salut".to_string())));
        // Un fichier en Latin-1 (« é » = 0xE9) garde son octet.
        d.ecrire("server.properties", b"motd=Caf\xe9\ndifficulty=1\n");
        modifier_proprietes(&d.0, &[("difficulty", "3".into())]).unwrap();
        assert_eq!(fs::read(d.0.join("server.properties")).unwrap(), b"motd=Caf\xe9\ndifficulty=3\n");
    }

    /// Un serveur minimal pour les tests.
    fn detecter_faux(id: &str) -> Serveur {
        serde_json::from_value(serde_json::json!({
            "id": id, "nom": id, "dossier": "", "logiciel": "Spigot", "version": "1.8.8", "jar": "server.jar", "ramMo": 1024
        }))
        .unwrap()
    }

    #[test]
    fn reconnait_un_spigot_lance_par_un_bat() {
        // Comme le serveur de test de Thomas : Spigot 1.8.8, start.bat, journal en Latin-1.
        let d = Dossier::nouveau("spigot");
        d.ecrire("server.jar", &[0; 64]);
        d.ecrire("worldedit.jar", &[0; 999]);
        d.ecrire("spigot.yml", b"");
        d.ecrire("start.bat", b"java -Xmx8096M -Xms8096M -jar server.jar nogui\r\n");
        d.ecrire(
            "logs/latest.log",
            b"[10:00:06] [Server thread/INFO]: Starting minecraft server version 1.8.8\r\n\
              [10:00:06] [Server thread/INFO]: This server is running CraftBukkit version git-Spigot-21fe707 (MC: 1.8.8)\r\n\
              [10:00:07] [Server thread/INFO]: caract\xe8re pas UTF-8\r\n",
        );
        let s = detecter(&d.0).unwrap();
        assert_eq!(s.jar, "server.jar");
        assert_eq!(s.logiciel, "Spigot");
        assert_eq!(s.version, "1.8.8");
        assert_eq!(s.ram_mo, 8096);
        assert!(s.nom.starts_with("Bhost-test-spigot"));
    }

    #[test]
    fn reconnait_paper_et_fabric_dans_le_journal() {
        let (l, v) = lire_journal(
            "[12:00:00 INFO]: Starting minecraft server version 1.21.4\n\
             [12:00:00 INFO]: This server is running Paper version 1.21.4-150 (MC: 1.21.4)\n",
        );
        assert_eq!((l, v.as_deref()), (Some("Paper"), Some("1.21.4")));
        let (l, v) = lire_journal("[main/INFO]: Loading Minecraft 1.21.4 with Fabric Loader 0.16.9\n");
        assert_eq!((l, v.as_deref()), (Some("Fabric"), Some("1.21.4")));
        let (l, _) = lire_journal("[INFO]: This server is running Purpur version 1.21-2300 (Implementing Paper API)\n");
        assert_eq!(l, Some("Purpur"));
    }

    #[test]
    fn sans_journal_prend_le_plus_gros_jar_et_devine_le_logiciel() {
        let d = Dossier::nouveau("sansjournal");
        d.ecrire("petit.jar", &[0; 10]);
        d.ecrire("paper-1.21.jar", &[0; 500]);
        d.ecrire("config/paper-global.yml", b"");
        let s = detecter(&d.0).unwrap();
        assert_eq!(s.jar, "paper-1.21.jar");
        assert_eq!(s.logiciel, "Paper");
        assert_eq!(s.version, "");
        assert_eq!(s.ram_mo, RAM_PAR_DEFAUT);
    }

    #[test]
    fn lit_le_port_dans_server_properties() {
        let d = Dossier::nouveau("port");
        assert_eq!(port(&d.0), 25565);
        d.ecrire("server.properties", b"#Minecraft server properties\r\nmotd=A Minecraft Server\r\nserver-port=25570\r\n");
        assert_eq!(port(&d.0), 25570);
    }

    #[test]
    fn refuse_un_dossier_sans_jar() {
        let d = Dossier::nouveau("vide");
        d.ecrire("notes.txt", b"rien");
        assert!(detecter(&d.0).is_err());
    }

    #[test]
    fn lit_la_ram_en_giga_et_le_jar_entre_guillemets() {
        assert_eq!(
            lire_commande("java -Xms1G -Xmx4G -jar \"paper.jar\" --nogui"),
            (Some("paper.jar".to_string()), Some(4096))
        );
    }

    #[test]
    fn ajoute_garde_et_relit_la_liste() {
        let d = Dossier::nouveau("liste");
        d.ecrire("server.jar", &[0; 8]);
        let fichier = d.0.join("serveurs.json");
        let mut liste = charger(&fichier);
        assert!(liste.is_empty());
        let s = ajouter(&mut liste, detecter(&d.0).unwrap()).unwrap();
        assert_eq!(s.id, "s1");
        // Le même dossier une deuxième fois : refusé, même avec une autre casse.
        let mut double = detecter(&d.0).unwrap();
        double.dossier = double.dossier.to_uppercase() + "/";
        assert!(ajouter(&mut liste, double).is_err());
        enregistrer(&fichier, &liste).unwrap();
        assert_eq!(charger(&fichier), liste);
        assert!(fs::read_to_string(&fichier).unwrap().contains("\"ramMo\""));
        // Pas d'apparence choisie : rien d'écrit (les anciens fichiers restent lisibles).
        assert!(!fs::read_to_string(&fichier).unwrap().contains("couleur"));
        liste[0].couleur = Some("vert".into());
        enregistrer(&fichier, &liste).unwrap();
        assert_eq!(charger(&fichier), liste);
        // Les anciennes apparences (icône + couleur en hexadécimal) se lisent encore.
        let ancien = fs::read_to_string(&fichier).unwrap().replace("\"vert\"", "\"#5E7A45\", \"icone\": \"arbre\"");
        fs::write(&fichier, ancien).unwrap();
        assert_eq!(charger(&fichier)[0].couleur.as_deref(), Some("#5E7A45"));
    }

    #[test]
    fn verifie_la_couleur() {
        assert!(icone_valide("bleu"));
        assert!(icone_valide("menthe"));
        assert!(icone_valide("guimauve"));
        assert!(icone_valide("serveur-orrose"));
        assert!(!icone_valide("glacon"));
        assert!(!icone_valide("#4F6C8C"));
        assert!(!icone_valide("red\"><script>"));
    }

    #[test]
    fn memes_icones_que_l_interface() {
        // src/theme/icones.ts et ICONES doivent avoir exactement les mêmes clés.
        let ts = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/theme/icones.ts")).unwrap();
        let mut cles: Vec<&str> = ts.split("cle: \"").skip(1).map(|r| r.split('"').next().unwrap()).collect();
        cles.sort();
        let mut rust = ICONES.to_vec();
        rust.sort();
        assert_eq!(cles, rust);
    }

    #[test]
    fn ne_jette_que_des_dossiers_de_serveur() {
        let d = Dossier::nouveau("jeter");
        d.ecrire("server.jar", &[0; 8]);
        let mut s = detecter(&d.0).unwrap();
        // Un .jar seul ne suffit pas : il faut server.properties ou eula.txt.
        assert!(!dossier_de_serveur(&s));
        d.ecrire("eula.txt", b"eula=true\n");
        assert!(dossier_de_serveur(&s));
        s.dossier = d.0.parent().unwrap().to_string_lossy().into_owned(); // le dossier au-dessus : pas un serveur
        assert!(!dossier_de_serveur(&s));
        s.dossier = "/".into();
        assert!(!dossier_de_serveur(&s));
        s.dossier = "relatif".into();
        assert!(!dossier_de_serveur(&s));
    }

    #[test]
    fn reconnait_forge_et_neoforge_recents() {
        let fin = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
        assert_eq!(selon_arguments(&format!("@libraries/net/minecraftforge/forge/1.21.11-61.2.4/{fin}")), Some(("Forge", "1.21.11".to_string())));
        assert_eq!(selon_arguments(&format!("@libraries/net/neoforged/neoforge/21.1.267/{fin}")), Some(("NeoForge", "1.21.1".to_string())));
        assert_eq!(selon_arguments("server.jar"), None);
        let (jar, ram) = lire_commande(&format!("java -Xmx6G @user_jvm_args.txt @libraries/net/neoforged/neoforge/21.1.267/{fin} %*"));
        assert_eq!(jar.as_deref(), Some(format!("@libraries/net/neoforged/neoforge/21.1.267/{fin}").as_str()));
        assert_eq!(ram, Some(6144));
    }
}