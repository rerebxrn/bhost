// Onglet Plugins / Mods / Datapacks : ce qui est installé sur un serveur, et le catalogue en ligne.
// Le catalogue est celui de Modrinth (API publique, https://api.modrinth.com/v2, sans compte).
// Ici : les fichiers, ce qu'on lit dedans, et la lecture des réponses du catalogue.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const API: &str = "https://api.modrinth.com/v2";
/// Un mod ou un plugin fait rarement plus de quelques dizaines de Mo.
pub const TAILLE_MAX: u64 = 200 * 1024 * 1024;
/// Le catalogue est redemandé au plus tous les quarts d'heure pour un même fichier.
const FRAICHEUR: Duration = Duration::from_secs(15 * 60);

/// Ce que le serveur sait charger.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Genre {
    Plugins,
    Mods,
    Datapacks,
}

impl Genre {
    pub fn de(logiciel: &str) -> Option<Genre> {
        match logiciel {
            "Paper" | "Purpur" | "Spigot" => Some(Genre::Plugins),
            // Les hybrides (Mohist, Youer, Arclight) chargent des mods et des plugins : les mods d'abord.
            "Fabric" | "Forge" | "NeoForge" | "Mohist" | "Youer" | "Arclight" => Some(Genre::Mods),
            "Vanilla" => Some(Genre::Datapacks),
            _ => None,
        }
    }

    pub fn depuis_id(id: &str) -> Option<Genre> {
        match id {
            "plugins" => Some(Genre::Plugins),
            "mods" => Some(Genre::Mods),
            "datapacks" => Some(Genre::Datapacks),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Genre::Plugins => "plugins",
            Genre::Mods => "mods",
            Genre::Datapacks => "datapacks",
        }
    }

    /// L'extension des fichiers : .jar pour les plugins et les mods, .zip pour les datapacks.
    fn extension(self) -> &'static str {
        if self == Genre::Datapacks { ".zip" } else { ".jar" }
    }
}

/// Les datapacks existent depuis Minecraft 1.13 (et dans tous les snapshots récents, « 24w14a »).
pub fn datapacks_possibles(version: &str) -> bool {
    let v = version.trim();
    if v.len() >= 3 && v.as_bytes()[2] == b'w' && v[..2].chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let n: Vec<u32> = v.split(['.', '-']).map_while(|x| x.parse().ok()).collect();
    match n.as_slice() {
        [1, mineure, ..] => *mineure >= 13,
        [annee, ..] => *annee >= 26,
        [] => false,
    }
}

/// Les logiciels « hybrides » : des mods (Forge, NeoForge ou Fabric) et des plugins (Bukkit, Spigot, Paper).
pub fn hybride(logiciel: &str) -> bool {
    matches!(logiciel, "Mohist" | "Youer" | "Arclight")
}

/// Ce qu'un serveur sait charger : son genre principal (et les plugins pour un hybride),
/// plus les datapacks (dans le monde) depuis la 1.13.
pub fn genres(logiciel: &str, version: &str) -> Vec<Genre> {
    let Some(principal) = Genre::de(logiciel) else { return Vec::new() };
    let mut l = vec![principal];
    if hybride(logiciel) {
        l.push(Genre::Plugins);
    }
    if principal != Genre::Datapacks && datapacks_possibles(version) {
        l.push(Genre::Datapacks);
    }
    l
}

/// Les « loaders » du catalogue pour ce genre : « datapack » pour les datapacks, sinon ceux du logiciel.
/// `jar` : le fichier lancé (Arclight existe pour Forge, NeoForge et Fabric : son nom le dit).
pub fn loaders_genre(genre: Genre, logiciel: &str, jar: &str) -> Vec<&'static str> {
    match (genre, logiciel) {
        (Genre::Datapacks, _) => vec!["datapack"],
        (Genre::Plugins, "Mohist" | "Youer") => vec!["paper", "spigot", "bukkit"],
        (Genre::Plugins, "Arclight") => vec!["spigot", "bukkit"],
        (Genre::Mods, "Mohist") => vec!["forge"],
        (Genre::Mods, "Youer") => vec!["neoforge"],
        (Genre::Mods, "Arclight") => vec![moteur_arclight(jar)],
        _ => loaders(logiciel),
    }
}

/// « arclight-fabric-1.21.1-1.0.1.jar » → « fabric ». Sans indice : « forge ».
pub fn moteur_arclight(jar: &str) -> &'static str {
    let j = jar.to_ascii_lowercase();
    if j.starts_with("arclight-neoforge-") {
        "neoforge"
    } else if j.starts_with("arclight-fabric-") {
        "fabric"
    } else {
        "forge"
    }
}

/// Les « loaders » du catalogue qui marchent sur ce logiciel (Paper charge aussi les plugins Spigot et Bukkit…).
pub fn loaders(logiciel: &str) -> Vec<&'static str> {
    match logiciel {
        "Paper" => vec!["paper", "spigot", "bukkit"],
        "Purpur" => vec!["purpur", "paper", "spigot", "bukkit"],
        "Spigot" => vec!["spigot", "bukkit"],
        "Fabric" => vec!["fabric"],
        "Forge" => vec!["forge"],
        "NeoForge" => vec!["neoforge"],
        "Vanilla" => vec!["datapack"],
        _ => vec![],
    }
}

/// Le dossier où vont les fichiers : plugins/, mods/, ou <monde>/datapacks/.
pub fn dossier(serveur: &Path, genre: Genre) -> PathBuf {
    match genre {
        Genre::Plugins => serveur.join("plugins"),
        Genre::Mods => serveur.join("mods"),
        Genre::Datapacks => serveur.join(crate::mondes::niveau(serveur)).join("datapacks"),
    }
}

const COUPE: &str = ".disabled";

/// Un fichier installé (« WorldEdit.jar », ou « WorldEdit.jar.disabled » s'il est coupé).
#[derive(Debug, Clone, PartialEq)]
pub struct Fichier {
    pub nom: String,
    pub actif: bool,
    /// Un datapack en dossier (pas en .zip) : on ne peut que le supprimer.
    pub dossier: bool,
}

/// Les fichiers installés, par ordre alphabétique.
pub fn lister(dossier: &Path, genre: Genre) -> Vec<Fichier> {
    let Ok(entrees) = fs::read_dir(dossier) else { return vec![] };
    let ext = genre.extension();
    let mut liste: Vec<Fichier> = entrees
        .flatten()
        .filter_map(|e| {
            let nom = e.file_name().to_string_lossy().into_owned();
            let type_ = e.file_type().ok()?;
            // « bukkit » : le datapack que Spigot et Paper créent eux-mêmes dans chaque monde. On ne le montre pas.
            if type_.is_dir() && genre == Genre::Datapacks && nom.eq_ignore_ascii_case("bukkit") {
                return None;
            }
            if type_.is_dir() {
                return (genre == Genre::Datapacks && e.path().join("pack.mcmeta").is_file()).then_some(Fichier { nom, actif: true, dossier: true });
            }
            let bas = nom.to_ascii_lowercase();
            if bas.ends_with(ext) {
                Some(Fichier { nom, actif: true, dossier: false })
            } else if bas.ends_with(&format!("{ext}{COUPE}")) {
                Some(Fichier { nom, actif: false, dossier: false })
            } else {
                None
            }
        })
        .collect();
    liste.sort_by_key(|f| f.nom.to_lowercase());
    liste
}

/// « WorldEdit.jar.disabled » → « WorldEdit.jar ».
pub fn nom_actif(nom: &str) -> &str {
    nom.strip_suffix(COUPE).unwrap_or(nom)
}

/// Active ou coupe un fichier (en ajoutant ou retirant « .disabled »). Renvoie son nouveau nom.
pub fn basculer(dossier: &Path, nom: &str, actif: bool) -> Result<String, String> {
    let nouveau = if actif { nom_actif(nom).to_string() } else { format!("{}{COUPE}", nom_actif(nom)) };
    if nouveau != nom {
        fs::rename(dossier.join(nom), dossier.join(&nouveau)).map_err(|_| "Windows refuse : arrête le serveur d’abord.".to_string())?;
    }
    Ok(nouveau)
}

// ---------------------------------------------------------------------------------------------
// Ce qu'on lit dans le fichier lui-même (sans Internet)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InfosLocales {
    /// L'identifiant du plugin ou du mod (name de plugin.yml, id de fabric.mod.json, modId de mods.toml) :
    /// deux fichiers avec le même identifiant sont le même plugin en double.
    pub id: Option<String>,
    pub nom: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    /// Les logiciels que le fichier dit savoir faire tourner (« paper », « fabric »…), d'après ce qu'il contient.
    /// Un même fichier peut en avoir plusieurs (ViaVersion : plugin.yml et fabric.mod.json).
    pub charge: Vec<String>,
}

/// La valeur d'une clé de premier niveau dans un YAML simple (plugin.yml) : « name: WorldEdit ».
fn yaml(texte: &str, cle: &str) -> Option<String> {
    texte.lines().find_map(|l| {
        let reste = l.strip_prefix(cle)?.strip_prefix(':')?;
        propre(reste.trim().trim_matches(|c| c == '"' || c == '\''))
    })
}

/// La valeur d'une clé dans un TOML simple (mods.toml) : « displayName="JEI" ».
fn toml(texte: &str, cle: &str) -> Option<String> {
    texte.lines().find_map(|l| {
        let l = l.trim();
        let reste = l.strip_prefix(cle)?.trim_start().strip_prefix('=')?.trim();
        let v = reste.trim_start_matches("'''").trim_matches(|c| c == '"' || c == '\'');
        propre(v)
    })
}

/// Une valeur utilisable : pas vide, pas une variable non remplacée (« ${version} »), une ligne, 200 lettres au plus.
fn propre(v: &str) -> Option<String> {
    let v = v.lines().next().unwrap_or("").trim();
    (!v.is_empty() && !v.contains("${")).then(|| v.chars().take(200).collect())
}

/// Le texte d'une description Minecraft : une chaîne, ou un texte en JSON ({"text": …}, ou une liste).
fn texte_minecraft(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(texte_minecraft).collect(),
        Value::Object(o) => {
            let mut t = o.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
            if let Some(extra) = o.get("extra") {
                t.push_str(&texte_minecraft(extra));
            }
            t
        }
        _ => String::new(),
    }
}

/// Retire les codes de couleur de Minecraft (« §a »).
fn sans_couleurs(t: &str) -> String {
    let mut sortie = String::new();
    let mut passer = false;
    for c in t.chars() {
        if passer {
            passer = false;
        } else if c == '§' {
            passer = true;
        } else {
            sortie.push(c);
        }
    }
    sortie
}

fn infos_mcmeta(texte: &str) -> InfosLocales {
    let v: Value = serde_json::from_str(texte).unwrap_or(Value::Null);
    let description = v.pointer("/pack/description").map(texte_minecraft).map(|t| sans_couleurs(&t)).and_then(|t| propre(&t));
    InfosLocales { description, ..Default::default() }
}

/// Lit le nom, la version et la description écrits dans un .jar ou un .zip (ou un datapack en dossier).
pub fn infos_locales(chemin: &Path) -> InfosLocales {
    if chemin.is_dir() {
        return fs::read_to_string(chemin.join("pack.mcmeta")).map(|t| infos_mcmeta(&t)).unwrap_or_default();
    }
    let Ok(f) = fs::File::open(chemin) else { return InfosLocales::default() };
    infos_archive(f)
}

/// Lit le nom, la version et la description écrits dans une archive (.jar ou .zip).
fn infos_archive(f: impl Read + std::io::Seek) -> InfosLocales {
    let Ok(mut archive) = zip::ZipArchive::new(f) else { return InfosLocales::default() };
    let mut charge = Vec::new();
    for (fichier, logiciels) in [
        ("plugin.yml", &["bukkit", "spigot", "paper", "purpur", "folia"][..]),
        ("paper-plugin.yml", &["paper", "purpur", "folia"][..]),
        ("fabric.mod.json", &["fabric", "quilt"][..]),
        ("quilt.mod.json", &["quilt"][..]),
        ("META-INF/mods.toml", &["forge"][..]),
        ("META-INF/neoforge.mods.toml", &["neoforge"][..]),
    ] {
        if archive.index_for_name(fichier).is_some() {
            charge.extend(logiciels.iter().map(|l| l.to_string()));
        }
    }
    let infos = infos_archive_texte(&mut archive);
    InfosLocales { charge, ..infos }
}

fn infos_archive_texte<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> InfosLocales {
    let mut lire = |nom: &str| -> Option<String> {
        let mut entree = archive.by_name(nom).ok()?;
        let mut texte = String::new();
        entree.by_ref().take(256 * 1024).read_to_string(&mut texte).ok()?;
        Some(texte)
    };
    if let Some(t) = lire("plugin.yml").or_else(|| lire("paper-plugin.yml")) {
        return InfosLocales { id: yaml(&t, "name"), nom: yaml(&t, "name"), version: yaml(&t, "version"), description: yaml(&t, "description"), ..Default::default() };
    }
    if let Some(t) = lire("fabric.mod.json") {
        let v: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
        let champ = |c: &str| v.get(c).and_then(|x| x.as_str()).and_then(propre);
        return InfosLocales { id: champ("id"), nom: champ("name"), version: champ("version"), description: champ("description"), ..Default::default() };
    }
    if let Some(t) = lire("META-INF/neoforge.mods.toml").or_else(|| lire("META-INF/mods.toml")) {
        return InfosLocales { id: toml(&t, "modId"), nom: toml(&t, "displayName"), version: toml(&t, "version"), description: toml(&t, "description"), ..Default::default() };
    }
    if let Some(t) = lire("pack.mcmeta") {
        return infos_mcmeta(&t);
    }
    InfosLocales::default()
}

/// L'empreinte SHA-1 d'un fichier, en hexadécimal (ce que le catalogue utilise pour le reconnaître).
pub fn sha1_fichier(chemin: &Path) -> Option<String> {
    let mut f = fs::File::open(chemin).ok()?;
    let mut h = Sha1::new();
    let mut tampon = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut tampon).ok()?;
        if n == 0 {
            break;
        }
        h.update(&tampon[..n]);
    }
    Some(hex(&h.finalize()))
}

fn hex(octets: &[u8]) -> String {
    octets.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// Le catalogue en ligne
// ---------------------------------------------------------------------------------------------

/// Un projet du catalogue (un mod, un plugin, un datapack).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Projet {
    pub id: String,
    pub nom: String,
    pub description: String,
    /// La couleur principale de son icône (0xRRGGBB), pour la tuile quand l'image manque.
    pub couleur: Option<u32>,
    /// L'adresse de son icône (seulement sur cdn.modrinth.com).
    pub icone: Option<String>,
    /// Son auteur (seulement dans une recherche).
    pub auteur: Option<String>,
    pub telechargements: u64,
}

/// Un fichier à télécharger.
#[derive(Debug, Clone, PartialEq)]
pub struct FichierMr {
    pub url: String,
    pub nom: String,
    pub sha1: String,
    pub sha512: String,
}

/// Une version d'un projet.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionMr {
    pub id: String,
    pub projet: String,
    pub numero: String,
    /// Une version « release » (pas une bêta ni une alpha).
    pub stable: bool,
    pub jeux: Vec<String>,
    pub loaders: Vec<String>,
    pub fichier: FichierMr,
    /// Les projets obligatoires pour celui-ci.
    pub dependances: Vec<String>,
    /// Les projets avec lesquels il ne marche pas (déclarés par son auteur).
    pub incompatibles: Vec<String>,
}

fn texte(v: &Value, cle: &str) -> String {
    v.get(cle).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

fn liste(v: &Value, cle: &str) -> Vec<String> {
    v.get(cle).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Un identifiant du catalogue : lettres et chiffres seulement.
pub fn id_valide(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Un nom de fichier sans chemin (« WorldEdit-7.3.jar »).
pub fn nom_fichier_valide(nom: &str) -> bool {
    // Les noms que Windows réserve (« con », « nul.jar »…), même avec une extension.
    const RESERVES: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6",
        "lpt7", "lpt8", "lpt9",
    ];
    let base = nom.split('.').next().unwrap_or("").trim_end().to_ascii_lowercase();
    !nom.is_empty()
        && nom.len() <= 200
        && !nom.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
        && !nom.starts_with('.')
        && !nom.ends_with(['.', ' '])
        && !RESERVES.contains(&base.as_str())
}

fn lire_projet(v: &Value, cle_id: &str, cle_nom: &str) -> Option<Projet> {
    let id = texte(v, cle_id);
    id_valide(&id).then(|| Projet {
        id,
        nom: texte(v, cle_nom),
        description: texte(v, "description"),
        couleur: v.get("color").and_then(|c| c.as_u64()).map(|c| (c & 0xFF_FF_FF) as u32),
        icone: Some(texte(v, "icon_url")).filter(|u| icone_valide(u)),
        auteur: Some(texte(v, "author")).filter(|a| !a.is_empty()),
        telechargements: v.get("downloads").and_then(|d| d.as_u64()).unwrap_or(0),
    })
}

/// Une icône qu'on peut afficher : une image du catalogue, rien d'autre.
pub fn icone_valide(u: &str) -> bool {
    u.len() <= 300 && u.starts_with("https://cdn.modrinth.com/") && !u.contains(['"', '\'', '<', '>', ' ', '\\'])
}

/// Une page de résultats, et combien il y en a en tout.
#[derive(Debug, Default, PartialEq)]
pub struct Recherche {
    pub projets: Vec<Projet>,
    pub total: u64,
    /// Où la page commence vraiment (le catalogue peut refuser d'aller trop loin).
    pub decalage: u64,
}

/// La réponse d'une recherche (/search).
pub fn lire_recherche(octets: &[u8]) -> Result<Recherche, String> {
    let v: Value = serde_json::from_slice(octets).map_err(|_| "Réponse illisible.".to_string())?;
    let projets = v.get("hits").and_then(|h| h.as_array()).map(|a| a.iter().filter_map(|x| lire_projet(x, "project_id", "title")).collect()).unwrap_or_default();
    let nombre = |c: &str| v.get(c).and_then(|x| x.as_u64()).unwrap_or(0);
    Ok(Recherche { projets, total: nombre("total_hits"), decalage: nombre("offset") })
}

/// La réponse de /projects?ids=[…].
pub fn lire_projets(octets: &[u8]) -> Vec<Projet> {
    let v: Value = serde_json::from_slice(octets).unwrap_or(Value::Null);
    v.as_array().map(|a| a.iter().filter_map(|x| lire_projet(x, "id", "title")).collect()).unwrap_or_default()
}

fn lire_version(v: &Value) -> Option<VersionMr> {
    let fichiers = v.get("files")?.as_array()?;
    let f = fichiers.iter().find(|f| f.get("primary").and_then(|p| p.as_bool()) == Some(true)).or_else(|| fichiers.first())?;
    let hashes = f.get("hashes")?;
    let fichier = FichierMr {
        url: texte(f, "url"),
        nom: texte(f, "filename"),
        sha1: texte(hashes, "sha1").to_ascii_lowercase(),
        sha512: texte(hashes, "sha512").to_ascii_lowercase(),
    };
    let extension_ok = [".jar", ".zip"].iter().any(|e| fichier.nom.to_ascii_lowercase().ends_with(e));
    if !fichier.url.starts_with("https://") || !nom_fichier_valide(&fichier.nom) || !extension_ok {
        return None;
    }
    let deps = |genre: &str| -> Vec<String> {
        v.get("dependencies")
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter(|d| d.get("dependency_type").and_then(|t| t.as_str()) == Some(genre))
                    .filter_map(|d| d.get("project_id").and_then(|p| p.as_str()).filter(|p| id_valide(p)).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let dependances = deps("required");
    let incompatibles = deps("incompatible");
    // L'identifiant du projet sert ensuite dans des adresses : seulement des lettres et des chiffres.
    if !id_valide(&texte(v, "project_id")) {
        return None;
    }
    Some(VersionMr {
        id: texte(v, "id"),
        projet: texte(v, "project_id"),
        numero: texte(v, "version_number"),
        // Absent dans les vieilles réponses : on la croit stable.
        stable: v.get("version_type").and_then(|t| t.as_str()).is_none_or(|t| t == "release"),
        jeux: liste(v, "game_versions"),
        loaders: liste(v, "loaders"),
        fichier,
        dependances,
        incompatibles,
    })
}

/// La réponse de /project/<id>/version : les versions, la plus récente en premier.
pub fn lire_versions(octets: &[u8]) -> Vec<VersionMr> {
    let v: Value = serde_json::from_slice(octets).unwrap_or(Value::Null);
    v.as_array().map(|a| a.iter().filter_map(lire_version).collect()).unwrap_or_default()
}

/// La réponse de /version_files et /version_files/update : empreinte → version.
pub fn lire_par_empreinte(octets: &[u8]) -> HashMap<String, VersionMr> {
    let v: Value = serde_json::from_slice(octets).unwrap_or(Value::Null);
    v.as_object()
        .map(|o| o.iter().filter_map(|(k, x)| Some((k.to_ascii_lowercase(), lire_version(x)?))).collect())
        .unwrap_or_default()
}

/// Encode un texte pour une adresse web.
pub fn encoder(t: &str) -> String {
    t.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Une catégorie du catalogue : lettres minuscules et tirets.
pub fn categorie_valide(c: &str) -> bool {
    (1..=32).contains(&c.len()) && c.chars().all(|x| x.is_ascii_lowercase() || x == '-')
}

/// L'ordre des résultats.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tri {
    /// Le plus proche du texte cherché (sans texte : les plus téléchargés).
    Pertinence,
    Telecharges,
    /// Les moins téléchargés : la fin de la liste « téléchargés », lue à l'envers.
    MoinsTelecharges,
    Suivis,
    Recents,
    MisAJour,
}

impl Tri {
    pub fn de(t: &str) -> Tri {
        match t {
            "telecharges" => Tri::Telecharges,
            "moins" => Tri::MoinsTelecharges,
            "suivis" => Tri::Suivis,
            "recents" => Tri::Recents,
            "maj" => Tri::MisAJour,
            _ => Tri::Pertinence,
        }
    }

    fn index(self, texte_vide: bool) -> &'static str {
        match self {
            Tri::Pertinence if !texte_vide => "relevance",
            Tri::Pertinence | Tri::Telecharges | Tri::MoinsTelecharges => "downloads",
            Tri::Suivis => "follows",
            Tri::Recents => "newest",
            Tri::MisAJour => "updated",
        }
    }
}

/// Combien de résultats par page.
pub const PAGE: u64 = 20;

/// L'adresse d'une recherche : seulement ce qui marche sur ce logiciel et cette version de Minecraft.
/// Pour les mods : seulement ceux qui tournent sur un serveur.
#[allow(clippy::too_many_arguments)]
pub fn adresse_recherche(genre: Genre, logiciel: &str, version: &str, texte: &str, categorie: Option<&str>, tri: Tri, decalage: u64, nombre: u64) -> String {
    let mut facettes: Vec<Vec<String>> = vec![vec![format!(
        "project_type:{}",
        match genre {
            Genre::Plugins => "plugin",
            Genre::Mods => "mod",
            Genre::Datapacks => "datapack",
        }
    )]];
    if genre != Genre::Datapacks {
        facettes.push(loaders(logiciel).iter().map(|l| format!("categories:{l}")).collect());
    }
    if !version.is_empty() {
        facettes.push(vec![format!("versions:{version}")]);
    }
    if genre == Genre::Mods {
        facettes.push(vec!["server_side:required".into(), "server_side:optional".into()]);
    }
    if let Some(c) = categorie.filter(|c| categorie_valide(c)) {
        facettes.push(vec![format!("categories:{c}")]);
    }
    let texte = texte.trim();
    let index = tri.index(texte.is_empty());
    let facettes = serde_json::to_string(&facettes).unwrap_or_default();
    let nombre = nombre.clamp(1, 100);
    format!("{API}/search?query={}&facets={}&index={index}&offset={decalage}&limit={nombre}", encoder(texte), encoder(&facettes))
}

/// « Moins téléchargés » : le catalogue ne trie que du plus au moins téléchargé, et son total n'est qu'une estimation.
/// On lit donc la fin de la liste (la page `page` en partant de la fin), puis on la retourne.
/// Si la page tombe à côté, on cherche d'abord où la liste s'arrête vraiment (par dichotomie).
/// `chercher(décalage, nombre)` fait la demande. Renvoie les projets (le moins téléchargé d'abord),
/// le total retenu, et s'il en reste d'autres.
pub fn fin_de_liste(page: u64, mut chercher: impl FnMut(u64, u64) -> Result<Recherche, String>) -> Result<(Vec<Projet>, u64, bool), String> {
    let deja = page * PAGE;
    // Le catalogue arrondit le décalage à un multiple du nombre demandé (12 806 + 20 → 12 800) :
    // on lit donc des blocs de 20 bien alignés (deux au plus) et on garde la tranche voulue.
    // `sur` : le total vient d'être mesuré, on ne le compare plus à celui annoncé.
    let lire = |chercher: &mut dyn FnMut(u64, u64) -> Result<Recherche, String>, total: u64, sur: bool| -> Result<Option<(Vec<Projet>, bool)>, String> {
        if deja >= total {
            return Ok(Some((Vec::new(), false)));
        }
        let fin = total - deja;
        let debut = fin.saturating_sub(PAGE);
        let mut bloc = debut / PAGE * PAGE;
        let mut tranche = Vec::new();
        while bloc < fin {
            let r = chercher(bloc, PAGE)?;
            if r.decalage != bloc || (!sur && r.total > total) {
                return Ok(None);
            }
            tranche.extend(r.projets.into_iter().enumerate().map(|(i, p)| (bloc + i as u64, p)));
            bloc += PAGE;
        }
        let mut projets: Vec<Projet> = tranche.into_iter().filter(|(i, _)| (debut..fin).contains(i)).map(|(_, p)| p).collect();
        if projets.len() as u64 != fin - debut {
            return Ok(None);
        }
        projets.reverse();
        Ok(Some((projets, debut > 0)))
    };
    let annonce = chercher(0, 1)?.total;
    if let Some((p, suite)) = lire(&mut chercher, annonce, false)? {
        return Ok((p, annonce, suite));
    }
    // Où la liste s'arrête-t-elle ? Le premier décalage sans résultat.
    let mut existe = |o: u64| -> Result<bool, String> {
        let r = chercher(o, 1)?;
        Ok(r.decalage == o && !r.projets.is_empty())
    };
    let (mut bas, mut haut) = (0u64, annonce.max(1));
    let mut tours = 0;
    while existe(haut)? {
        bas = haut + 1;
        haut = haut.saturating_mul(2);
        tours += 1;
        if tours > 6 {
            return Err("Le catalogue ne répond pas bien. Réessaie.".into());
        }
    }
    while bas < haut {
        let milieu = bas + (haut - bas) / 2;
        if existe(milieu)? {
            bas = milieu + 1;
        } else {
            haut = milieu;
        }
    }
    let total = bas;
    match lire(&mut chercher, total, true)? {
        Some((p, suite)) => Ok((p, total, suite)),
        None => Err("Le catalogue ne répond pas bien. Réessaie.".into()),
    }
}

/// Ce que `choisir_version` a trouvé.
#[allow(clippy::large_enum_variant)]
pub enum Essai<T> {
    Version(VersionMr, T),
    DejaLa,
    Aucune,
    /// Toutes les versions essayées demandent ce Java.
    TropRecente(u32),
}

/// Choisit la plus récente version (dans l'ordre donné) dont le Java tourne sur ce serveur.
/// `refusees` : « version|java du serveur » → Java demandé (gardé d'un appel à l'autre).
/// `telecharger` renvoie le fichier et le Java qu'il demande. Au plus `max` téléchargements :
/// les versions déjà refusées ne comptent pas (et ne sont pas retéléchargées).
pub fn choisir_version<T>(
    versions: Vec<VersionMr>,
    actuelle: Option<&str>,
    java: Option<u32>,
    refusees: &mut HashMap<String, u32>,
    max: usize,
    mut telecharger: impl FnMut(&VersionMr) -> Result<(T, Option<u32>), String>,
) -> Result<Essai<T>, String> {
    let mut demande: Option<u32> = None;
    let mut essais = 0;
    for v in versions {
        if actuelle == Some(v.id.as_str()) {
            return Ok(Essai::DejaLa);
        }
        if let Some(il_faut) = java.and_then(|j| refusees.get(&format!("{}|{j}", v.id)).copied()) {
            demande.get_or_insert(il_faut);
            continue;
        }
        if essais == max {
            break;
        }
        essais += 1;
        let (octets, il_faut) = telecharger(&v)?;
        if let (Some(il_faut), Some(serveur)) = (il_faut, java)
            && il_faut > serveur
        {
            refusees.insert(format!("{}|{serveur}", v.id), il_faut);
            demande.get_or_insert(il_faut);
            continue;
        }
        return Ok(Essai::Version(v, octets));
    }
    Ok(demande.map_or(Essai::Aucune, Essai::TropRecente))
}

/// Pour une mise à jour : seulement les versions publiées après l'actuelle (l'API les donne de la plus récente
/// à la plus ancienne). Si l'actuelle n'est pas dans la liste, toutes.
pub fn plus_recentes(versions: Vec<VersionMr>, actuelle: &str) -> Vec<VersionMr> {
    match versions.iter().position(|v| v.id == actuelle) {
        Some(i) => versions.into_iter().take(i).collect(),
        None => versions,
    }
}

/// L'identifiant d'un plugin ou d'un mod (en minuscules) dans un fichier téléchargé (plugin.yml, fabric.mod.json…).
pub fn id_dans(octets: &[u8]) -> Option<String> {
    infos_archive(std::io::Cursor::new(octets)).cle()
}

impl InfosLocales {
    /// Ce qui désigne ce plugin ou ce mod, en minuscules (identifiant, sinon nom).
    pub fn cle(&self) -> Option<String> {
        self.id.as_ref().or(self.nom.as_ref()).map(|x| x.trim().to_lowercase()).filter(|x| !x.is_empty())
    }
}

/// Les fichiers en double : deux fichiers actifs qui sont le même plugin (même clé).
/// `fichiers` : (nom du fichier, clé, actif). Renvoie, pour chaque fichier en double, le nom d'un autre.
pub fn doublons(fichiers: &[(String, Option<String>, bool)]) -> HashMap<String, String> {
    let mut par_cle: HashMap<&str, Vec<&str>> = HashMap::new();
    for (nom, cle, actif) in fichiers {
        if let (Some(c), true) = (cle, actif) {
            par_cle.entry(c.as_str()).or_default().push(nom.as_str());
        }
    }
    let mut sortie = HashMap::new();
    for noms in par_cle.values().filter(|n| n.len() > 1) {
        for (i, n) in noms.iter().enumerate() {
            sortie.insert(n.to_string(), noms[(i + 1) % noms.len()].to_string());
        }
    }
    sortie
}

/// Les versions à essayer, dans l'ordre : les stables d'abord (de la plus récente à la plus ancienne), puis les bêtas.
pub fn ordre_essai(mut versions: Vec<VersionMr>) -> Vec<VersionMr> {
    versions.sort_by_key(|v| !v.stable);
    versions
}

/// La version de Java qu'il faut pour un .jar : celle de sa classe principale
/// (ou, sans classe principale connue, la plus haute de ses classes). `None` si on ne trouve rien.
pub fn java_du_jar(octets: &[u8]) -> Option<u32> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(octets)).ok()?;
    // Les 300 premières classes (hors META-INF, où rangent les .jar « multi-versions »).
    let noms: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index_raw(i).ok().and_then(|f| f.name().ok().map(|n| n.to_string())))
        .filter(|n| n.ends_with(".class") && !n.starts_with("META-INF/"))
        .take(300)
        .collect();
    let mut lire = |nom: &str, max: u64| -> Option<Vec<u8>> {
        let mut entree = archive.by_name(nom).ok()?;
        let mut o = Vec::new();
        entree.by_ref().take(max).read_to_end(&mut o).ok()?;
        Some(o)
    };
    let classe = |o: &[u8]| -> Option<u32> {
        (o.len() >= 8 && o[..4] == [0xCA, 0xFE, 0xBA, 0xBE]).then(|| u32::from(u16::from_be_bytes([o[6], o[7]]))).filter(|m| *m >= 45).map(|m| m - 44)
    };
    // Plugins : « main: com.sk89q.worldedit.bukkit.WorldEditPlugin ».
    let principale = lire("plugin.yml", 256 * 1024)
        .or_else(|| lire("paper-plugin.yml", 256 * 1024))
        .and_then(|o| yaml(&String::from_utf8_lossy(&o), "main"))
        .or_else(|| {
            // Mods Fabric : le premier point d'entrée.
            let o = lire("fabric.mod.json", 256 * 1024)?;
            let v: Value = serde_json::from_slice(&o).ok()?;
            let e = v.pointer("/entrypoints/main/0").or_else(|| v.pointer("/entrypoints/server/0"))?;
            e.as_str().or_else(|| e.get("value").and_then(|x| x.as_str())).map(|x| x.split("::").next().unwrap_or(x).to_string())
        });
    if let Some(p) = principale
        && let Some(j) = lire(&format!("{}.class", p.replace('.', "/")), 8).as_deref().and_then(classe)
    {
        return Some(j);
    }
    // Sinon : la plus haute de ces classes.
    noms.iter().filter_map(|n| lire(n, 8).as_deref().and_then(classe)).max()
}

/// Un plugin qui n'a pas démarré, d'après une ligne de la console.
#[derive(Debug, PartialEq)]
pub enum Panne {
    /// « Could not load 'plugins/X.jar' » : le fichier (en minuscules).
    Fichier(String),
    /// « Error occurred while enabling WorldEdit v7.4 » : le nom du plugin (en minuscules).
    Nom(String),
    /// Le plugin a démarré mais plante en jeu (« Plugin X v1 generated an exception… ») : son nom (en minuscules).
    Plante(String),
    /// Une ligne qui suit et qui explique pourquoi.
    Raison(String),
    /// « Missing data pack file/Terralith.zip » : un datapack retiré alors que le monde l'utilisait.
    PackManquant(String),
    /// « Failed to load datapacks, can't proceed with server load » : le monde ne se charge plus.
    MondeBloque,
}

/// Lit une ligne de console : un plugin en panne, ou la raison de la panne d'avant.
/// Les messages du chat passent par INFO : on les ignore (un joueur ne peut pas inventer une panne).
pub fn lire_panne(ligne: &str) -> Option<Panne> {
    let tete: String = ligne.chars().take(48).collect();
    // « [10:42:01 INFO]: … » (Paper) ou « 10:42:01 INFO … » (vieux Spigot).
    if tete.contains("INFO]") || tete.split_whitespace().nth(1) == Some("INFO") {
        return None;
    }
    if let Some(i) = ligne.find("Missing data pack file/") {
        let nom = ligne[i + "Missing data pack file/".len()..].trim();
        return (!nom.is_empty() && nom.len() <= 200).then(|| Panne::PackManquant(nom.to_string()));
    }
    if ligne.contains("Failed to load datapacks, can't proceed with server load") {
        return Some(Panne::MondeBloque);
    }
    let erreur = tete.contains("ERROR") || tete.contains("SEVERE");
    let alerte = erreur || tete.contains("WARN");
    let entre = |t: &str, debut: &str| -> Option<String> {
        let reste = &t[t.find(debut)? + debut.len()..];
        Some(reste[..reste.find('\'')?].to_string())
    };
    if let Some(chemin) = entre(ligne, "Could not load plugin '").or_else(|| entre(ligne, "Could not load '")).filter(|_| erreur) {
        let nom = chemin.rsplit(['/', '\\']).next().unwrap_or(&chemin).trim().to_lowercase();
        return (!nom.is_empty()).then_some(Panne::Fichier(nom));
    }
    if let Some(i) = ligne.find("while enabling ").filter(|_| erreur) {
        let reste = &ligne[i + "while enabling ".len()..];
        let reste = reste.split(" (Is it up to date").next().unwrap_or(reste);
        let nom = reste.rsplit_once(" v").map(|(n, _)| n).unwrap_or(reste).trim().to_lowercase();
        return (!nom.is_empty()).then_some(Panne::Nom(nom));
    }
    if let Some(nom) = plante_en_jeu(ligne).filter(|_| alerte) {
        return Some(Panne::Plante(nom));
    }
    // Le plugin appelle une fonction que ce Minecraft n'a pas : il est fait pour une autre version.
    if ["NoSuchMethodError: ", "NoSuchFieldError: ", "NoClassDefFoundError: ", "ClassNotFoundException: "].iter().any(|e| {
        ligne.find(e).is_some_and(|i| {
            let quoi = ligne[i + e.len()..].trim_start().replace('/', ".");
            ["org.bukkit.", "net.minecraft.", "com.destroystokyo.paper.", "io.papermc.", "net.md_5.bungee.", "org.spigotmc."].iter().any(|d| quoi.starts_with(d))
        })
    }) {
        return Some(Panne::Raison("Pas fait pour cette version de Minecraft.".into()));
    }
    if let Some(i) = ligne.find("class file version ") {
        let n: String = ligne[i + "class file version ".len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(m) = n.parse::<u32>()
            && m >= 45
        {
            return Some(Panne::Raison(format!("Il lui faut Java {}.", m - 44)));
        }
    }
    let bas = ligne.to_lowercase();
    if bas.contains("unknown dependency") || bas.contains("missing dependency") || bas.contains("unknown/missing dependency") {
        let liste = ligne.rfind('[').and_then(|d| Some(ligne[d + 1..d + 1 + ligne[d + 1..].find(']')?].trim().to_string())).filter(|l| !l.is_empty() && l.len() <= 80);
        return Some(Panne::Raison(match liste {
            Some(l) => format!("Il lui faut {l}."),
            None => "Il lui manque un autre plugin.".into(),
        }));
    }
    if bas.contains("unsupported api version") {
        return Some(Panne::Raison("Prévu pour un Minecraft plus récent.".into()));
    }
    None
}

/// Le nom du plugin qui plante en jeu, d'après ce qu'écrivent Bukkit, Spigot et Paper.
fn plante_en_jeu(ligne: &str) -> Option<String> {
    let sans_version = |t: &str| t.rsplit_once(" v").map(|(n, _)| n).unwrap_or(t).trim().to_lowercase();
    let nom = if let Some(fin) = ligne.find(" generated an exception") {
        // « Plugin SkinsRestorer v15.12.6 generated an exception while executing task 9 »
        let debut = ligne[..fin].rfind("Plugin ")? + "Plugin ".len();
        sans_version(&ligne[debut..fin])
    } else if let Some(i) = ligne.find("Could not pass event ") {
        // « Could not pass event PlayerJoinEvent to Essentials v2.21.0 »
        let reste = &ligne[i..];
        sans_version(&reste[reste.find(" to ")? + 4..])
    } else if let Some(i) = ligne.find("Unhandled exception executing command '") {
        // « Unhandled exception executing command 'skin' in plugin SkinsRestorer v15.12.6 »
        let reste = &ligne[i..];
        sans_version(&reste[reste.find(" in plugin ")? + " in plugin ".len()..])
    } else {
        // « [SkinsRestorer] An unhandled exception was thrown during command execution »
        let i = ligne.find("ERROR")?;
        let reste = ligne[i + "ERROR".len()..].trim_start_matches([']', ':', ' ']);
        let nom = reste.strip_prefix('[')?;
        let (nom, apres) = nom.split_once(']')?;
        if nom.contains(' ') || ["stderr", "stdout", "server", "minecraft"].contains(&nom.to_lowercase().as_str()) || !apres.to_lowercase().contains("exception") {
            return None;
        }
        nom.to_lowercase()
    };
    (!nom.is_empty() && nom.len() <= 60).then_some(nom)
}

/// Un chemin d'archive de datapack qui touche à la génération du monde (terrain, biomes, dimensions).
fn chemin_du_monde(nom: &str) -> bool {
    let p: Vec<&str> = nom.trim_start_matches("./").split(['/', '\\']).filter(|x| !x.is_empty()).collect();
    p.len() >= 4 && p[0] == "data" && matches!(p[2], "worldgen" | "dimension" | "dimension_type")
}

/// Ce datapack change la génération du monde : une fois le monde lancé avec lui, le retirer peut le casser.
pub fn change_le_monde(chemin: &Path) -> bool {
    if chemin.is_dir() {
        let Ok(espaces) = fs::read_dir(chemin.join("data")) else { return false };
        return espaces.flatten().any(|e| ["worldgen", "dimension", "dimension_type"].iter().any(|d| e.path().join(d).is_dir()));
    }
    let Ok(f) = fs::File::open(chemin) else { return false };
    let Ok(mut archive) = zip::ZipArchive::new(f) else { return false };
    (0..archive.len().min(20_000)).any(|i| archive.by_index_raw(i).ok().is_some_and(|e| e.name().is_ok_and(|n| chemin_du_monde(&n))))
}

/// Les datapacks avec lesquels le monde a déjà été chargé (« file/Terralith.zip », dans level.dat).
pub fn packs_du_monde(monde: &Path) -> Vec<String> {
    let Some(niveau) = crate::nbt::lire_fichier(&monde.join("level.dat")) else { return vec![] };
    match niveau.chemin(&["Data", "DataPacks", "Enabled"]) {
        Some(crate::nbt::Nbt::Liste(l)) => l.iter().filter_map(|n| n.texte().map(String::from)).collect(),
        _ => vec![],
    }
}

/// « Terralith_1.21.11_v2.6.0.zip » → « Terralith » (pour le chercher dans le catalogue).
pub fn nom_probable(fichier: &str) -> String {
    let sans = nom_actif(fichier);
    let sans = sans.strip_suffix(".zip").unwrap_or(sans);
    let mut mots: Vec<&str> = vec![];
    for m in sans.split(['_', '-', ' ', '+']) {
        let version = m.chars().next().is_some_and(|c| c.is_ascii_digit()) || (m.len() > 1 && m.starts_with(['v', 'V']) && m[1..].starts_with(|c: char| c.is_ascii_digit()));
        if version || m.eq_ignore_ascii_case("mc") {
            break;
        }
        if !m.is_empty() {
            mots.push(m);
        }
    }
    if mots.is_empty() { sans.to_string() } else { mots.join(" ") }
}

/// L'adresse des versions d'un projet qui marchent sur ce logiciel et cette version de Minecraft.
pub fn adresse_versions(projet: &str, loaders: &[&str], version: &str) -> String {
    let l = serde_json::to_string(loaders).unwrap_or_default();
    let mut a = format!("{API}/project/{projet}/version?loaders={}", encoder(&l));
    if !version.is_empty() {
        a.push_str(&format!("&game_versions={}", encoder(&serde_json::to_string(&[version]).unwrap_or_default())));
    }
    a
}

/// Vérifie qu'un fichier téléchargé est bien celui annoncé par le catalogue.
pub fn verifier(octets: &[u8], f: &FichierMr) -> bool {
    if f.sha512.len() == 128 {
        return hex(&Sha512::digest(octets)) == f.sha512;
    }
    f.sha1.len() == 40 && hex(&Sha1::digest(octets)) == f.sha1
}

// ---------------------------------------------------------------------------------------------
// Mémoires
// ---------------------------------------------------------------------------------------------

/// Ce que le catalogue a dit d'un fichier (reconnu par son empreinte), gardé un quart d'heure.
#[derive(Clone)]
pub struct Connu {
    pub version: Option<VersionMr>,
    pub projet: Option<Projet>,
    /// La version la plus récente pour ce logiciel et cette version de Minecraft.
    pub derniere: Option<VersionMr>,
    quand: Instant,
}

#[derive(Default)]
pub struct Cache {
    /// Clé : « empreinte|logiciel|version ».
    connus: HashMap<String, Connu>,
}

impl Cache {
    fn cle(sha1: &str, logiciel: &str, version: &str) -> String {
        format!("{sha1}|{logiciel}|{version}")
    }

    /// Ce qu'on sait d'un fichier, si c'est encore frais.
    pub fn lire(&self, sha1: &str, logiciel: &str, version: &str) -> Option<&Connu> {
        self.connus.get(&Self::cle(sha1, logiciel, version)).filter(|c| c.quand.elapsed() < FRAICHEUR)
    }

    pub fn noter(&mut self, sha1: &str, logiciel: &str, version: &str, v: Option<VersionMr>, projet: Option<Projet>, derniere: Option<VersionMr>) {
        if self.connus.len() > 2000 {
            self.connus.retain(|_, c| c.quand.elapsed() < FRAICHEUR);
        }
        self.connus.insert(Self::cle(sha1, logiciel, version), Connu { version: v, projet, derniere, quand: Instant::now() });
    }
}

/// « Installé pour WorldEdit » : les fichiers ajoutés comme dépendance d'un autre (gardés dans extensions.json).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Liens {
    #[serde(default)]
    serveurs: BTreeMap<String, BTreeMap<String, String>>,
}

impl Liens {
    pub fn charger(fichier: &Path) -> Liens {
        fs::read_to_string(fichier).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn enregistrer(&self, fichier: &Path) {
        if let Ok(t) = serde_json::to_string(self) {
            let _ = fs::write(fichier, t);
        }
    }

    /// Pour quel projet ce fichier a été installé (nom actif, sans « .disabled »).
    pub fn pour(&self, id: &str, fichier: &str) -> Option<String> {
        self.serveurs.get(id)?.get(nom_actif(fichier)).cloned()
    }

    pub fn noter(&mut self, id: &str, fichier: &str, pour: &str) {
        self.serveurs.entry(id.to_string()).or_default().insert(nom_actif(fichier).to_string(), pour.to_string());
    }

    pub fn oublier_fichier(&mut self, id: &str, fichier: &str) {
        if let Some(s) = self.serveurs.get_mut(id) {
            s.remove(nom_actif(fichier));
        }
    }

    /// Une mise à jour change le nom du fichier : le lien suit.
    pub fn renommer(&mut self, id: &str, ancien: &str, nouveau: &str) {
        if let Some(pour) = self.pour(id, ancien) {
            self.oublier_fichier(id, ancien);
            self.noter(id, nouveau, &pour);
        }
    }

    pub fn oublier(&mut self, id: &str) {
        self.serveurs.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-ext-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn jar(chemin: &Path, fichiers: &[(&str, &str)]) {
        let mut z = zip::ZipWriter::new(fs::File::create(chemin).unwrap());
        for (nom, texte) in fichiers {
            z.start_file(*nom, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(texte.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn sait_ce_que_charge_chaque_logiciel() {
        assert_eq!(Genre::de("Paper"), Some(Genre::Plugins));
        assert_eq!(Genre::de("NeoForge"), Some(Genre::Mods));
        assert_eq!(Genre::de("Vanilla"), Some(Genre::Datapacks));
        assert_eq!(Genre::de("Mohist"), Some(Genre::Mods));
        assert_eq!(Genre::de("Inconnu"), None);
        assert_eq!(loaders("Purpur"), ["purpur", "paper", "spigot", "bukkit"]);
        let d = Path::new("C:/s");
        assert_eq!(super::dossier(d, Genre::Plugins), d.join("plugins"));
    }

    #[test]
    fn liste_coupe_et_rallume() {
        let d = dossier("liste");
        jar(&d.join("WorldEdit.jar"), &[("plugin.yml", "name: WorldEdit\nversion: \"7.3.9\"\ndescription: Construire vite\n")]);
        fs::write(d.join("Vieux.jar.disabled"), "x").unwrap();
        fs::write(d.join("notes.txt"), "x").unwrap();
        fs::create_dir_all(d.join("WorldEdit")).unwrap();
        let l = lister(&d, Genre::Plugins);
        assert_eq!(
            l,
            [Fichier { nom: "Vieux.jar.disabled".into(), actif: false, dossier: false }, Fichier { nom: "WorldEdit.jar".into(), actif: true, dossier: false }]
        );
        assert_eq!(basculer(&d, "WorldEdit.jar", false).unwrap(), "WorldEdit.jar.disabled");
        assert!(d.join("WorldEdit.jar.disabled").is_file());
        assert_eq!(basculer(&d, "WorldEdit.jar.disabled", true).unwrap(), "WorldEdit.jar");
        assert_eq!(basculer(&d, "WorldEdit.jar", true).unwrap(), "WorldEdit.jar");
        let i = infos_locales(&d.join("WorldEdit.jar"));
        assert_eq!(
            i,
            InfosLocales { id: Some("WorldEdit".into()), nom: Some("WorldEdit".into()), version: Some("7.3.9".into()), description: Some("Construire vite".into()), charge: ["bukkit", "spigot", "paper", "purpur", "folia"].map(String::from).to_vec() }
        );
        assert_eq!(sha1_fichier(&d.join("notes.txt")).as_deref(), Some("11f6ad8ec52a2984abaafd7c3b516503785c2072"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn lit_les_mods_et_les_datapacks() {
        let d = dossier("infos");
        jar(&d.join("a.jar"), &[("fabric.mod.json", r#"{"id":"sodium","name":"Sodium","version":"0.6.0","description":"Rapide"}"#)]);
        assert_eq!(infos_locales(&d.join("a.jar")).nom.as_deref(), Some("Sodium"));
        jar(
            &d.join("b.jar"),
            &[("META-INF/mods.toml", "modLoader=\"javafml\"\n[[mods]]\nmodId=\"jei\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"Just Enough Items\"\ndescription='''\nVoir les recettes\n'''\n")],
        );
        let i = infos_locales(&d.join("b.jar"));
        assert_eq!(i.nom.as_deref(), Some("Just Enough Items"));
        assert_eq!(i.version, None);
        jar(&d.join("c.zip"), &[("pack.mcmeta", r#"{"pack":{"pack_format":48,"description":[{"text":"§aTerralith "},{"text":"2.5"}]}}"#)]);
        assert_eq!(infos_locales(&d.join("c.zip")).description.as_deref(), Some("Terralith 2.5"));
        fs::create_dir_all(d.join("bukkit")).unwrap();
        fs::write(d.join("bukkit").join("pack.mcmeta"), "{}").unwrap();
        fs::create_dir_all(d.join("dp")).unwrap();
        fs::write(d.join("dp").join("pack.mcmeta"), r#"{"pack":{"description":"Mon pack"}}"#).unwrap();
        let l = lister(&d, Genre::Datapacks);
        assert_eq!(l.iter().map(|f| (f.nom.as_str(), f.dossier)).collect::<Vec<_>>(), [("c.zip", false), ("dp", true)]);
        assert_eq!(infos_locales(&d.join("dp")).description.as_deref(), Some("Mon pack"));
        assert_eq!(infos_locales(&d.join("absent.jar")), InfosLocales::default());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn fabrique_les_adresses() {
        let a = adresse_recherche(Genre::Plugins, "Paper", "1.21.4", "world edit", Some("management"), Tri::Pertinence, 0, PAGE);
        assert!(a.starts_with("https://api.modrinth.com/v2/search?query=world%20edit&facets="));
        assert!(a.contains("index=relevance") && a.ends_with("offset=0&limit=20"));
        let facettes = a.split("facets=").nth(1).unwrap().split('&').next().unwrap();
        assert!(facettes.contains("project_type%3Aplugin") && facettes.contains("categories%3Abukkit") && facettes.contains("versions%3A1.21.4"));
        assert!(facettes.contains("categories%3Amanagement"));
        let a = adresse_recherche(Genre::Mods, "Fabric", "1.21.1", "", Some("pas valide!"), Tri::Pertinence, 40, PAGE);
        assert!(a.contains("index=downloads") && a.contains("server_side%3Arequired") && !a.contains("valide") && a.contains("offset=40"));
        let a = adresse_recherche(Genre::Datapacks, "Vanilla", "1.21.4", "", None, Tri::Recents, 0, 500);
        assert!(a.contains("project_type%3Adatapack") && !a.contains("categories") && a.contains("index=newest") && a.ends_with("limit=100"));
        // Avec un texte, le tri choisi passe avant la pertinence.
        assert!(adresse_recherche(Genre::Plugins, "Paper", "1.21.4", "chat", None, Tri::MisAJour, 0, PAGE).contains("index=updated"));
        assert!(adresse_recherche(Genre::Plugins, "Paper", "1.21.4", "chat", None, Tri::MoinsTelecharges, 0, PAGE).contains("index=downloads"));
        assert_eq!([Tri::de("moins"), Tri::de("suivis"), Tri::de("?")], [Tri::MoinsTelecharges, Tri::Suivis, Tri::Pertinence]);
        assert_eq!(
            adresse_versions("1u6JkXh5", &loaders("Spigot"), "1.8.8"),
            "https://api.modrinth.com/v2/project/1u6JkXh5/version?loaders=%5B%22spigot%22%2C%22bukkit%22%5D&game_versions=%5B%221.8.8%22%5D"
        );
        assert!(id_valide("1u6JkXh5") && !id_valide("../x") && !id_valide(""));
        assert!(nom_fichier_valide("a-1.jar") && !nom_fichier_valide("../a.jar") && !nom_fichier_valide("C:a.jar") && !nom_fichier_valide(".jar"));
        assert!(!nom_fichier_valide("nul.jar") && !nom_fichier_valide("COM1.zip") && !nom_fichier_valide("a.jar ") && !nom_fichier_valide("a?.jar"));
        assert!(nom_fichier_valide("console-1.2.jar") && nom_fichier_valide("Nullable.jar"));
    }

    #[test]
    fn lit_les_reponses_du_catalogue() {
        let r = lire_recherche(
            br#"{"hits":[{"project_id":"1u6JkXh5","title":"WorldEdit","description":"Editeur","downloads":11974609,"color":14961708,"author":"EngineHub",
                "icon_url":"https://cdn.modrinth.com/data/1u6JkXh5/icon.png"},{"project_id":"../x","title":"Mauvais"},
                {"project_id":"P2","title":"Sans icone","icon_url":"https://ailleurs.com/i.png"}],"total_hits":2345,"offset":40}"#,
        )
        .unwrap();
        assert_eq!((r.total, r.decalage, r.projets.len()), (2345, 40, 2));
        assert_eq!(
            r.projets[0],
            Projet {
                id: "1u6JkXh5".into(),
                nom: "WorldEdit".into(),
                description: "Editeur".into(),
                couleur: Some(14961708),
                icone: Some("https://cdn.modrinth.com/data/1u6JkXh5/icon.png".into()),
                auteur: Some("EngineHub".into()),
                telechargements: 11974609
            }
        );
        assert_eq!((r.projets[1].icone.as_deref(), r.projets[1].auteur.as_deref()), (None, None));
        assert!(!icone_valide("https://cdn.modrinth.com/a\"onerror=\"x.png"));
        assert!(lire_recherche(b"pas du json").is_err());

        let version = r#"{"id":"V1","project_id":"P1","version_number":"7.3.9","game_versions":["1.21.4"],"loaders":["paper","bukkit"],
            "files":[{"url":"https://cdn.modrinth.com/a/b/source.jar","filename":"source.jar","primary":false,"hashes":{"sha1":"aa","sha512":"bb"}},
                     {"url":"https://cdn.modrinth.com/a/b/WorldEdit.jar","filename":"WorldEdit.jar","primary":true,"hashes":{"sha1":"AB12","sha512":"CD34"}}],
            "dependencies":[{"project_id":"DEP1","dependency_type":"required"},{"project_id":"OPT","dependency_type":"optional"},{"version_id":"x","dependency_type":"required"},{"project_id":"FAWE","dependency_type":"incompatible"}]}"#;
        let v = lire_versions(format!("[{version}]").as_bytes());
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].fichier.nom.as_str(), v[0].fichier.sha1.as_str(), v[0].numero.as_str()), ("WorldEdit.jar", "ab12", "7.3.9"));
        assert_eq!(v[0].dependances, ["DEP1"]);
        assert_eq!(v[0].incompatibles, ["FAWE"]);
        assert!(v[0].stable);
        let beta = version.replace(r#""id":"V1""#, r#""id":"V2","version_type":"beta""#);
        let v = ordre_essai(lire_versions(format!("[{beta},{version}]").as_bytes()));
        assert_eq!(v.iter().map(|x| (x.id.as_str(), x.stable)).collect::<Vec<_>>(), [("V1", true), ("V2", false)]);
        let m = lire_par_empreinte(format!(r#"{{"ABCD":{version}}}"#).as_bytes());
        assert_eq!(m.get("abcd").map(|x| x.id.as_str()), Some("V1"));
        // Un fichier qui n'est pas en https, ou au nom dangereux, est ignoré.
        let mauvais = version.replace("https://cdn.modrinth.com/a/b/WorldEdit.jar", "http://x/WorldEdit.jar");
        assert!(lire_versions(format!("[{mauvais}]").as_bytes()).is_empty());
        let p = lire_projets(br#"[{"id":"P1","title":"WorldEdit","description":"d","downloads":3}]"#);
        assert_eq!(p[0].nom, "WorldEdit");
    }

    /// Un faux .jar : des fichiers texte et des classes dont on donne la version (69 = Java 25).
    fn faux_jar(fichiers: &[(&str, &[u8])]) -> Vec<u8> {
        let mut o = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut o);
            for (nom, contenu) in fichiers {
                z.start_file(*nom, zip::write::SimpleFileOptions::default()).unwrap();
                std::io::Write::write_all(&mut z, contenu).unwrap();
            }
            z.finish().unwrap();
        }
        o.into_inner()
    }

    fn classe(majeure: u16) -> Vec<u8> {
        let mut c = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0];
        c.extend(majeure.to_be_bytes());
        c.extend([0; 8]);
        c
    }

    /// Un faux catalogue de `vrai` projets, dont le total annoncé est `annonce`.
    /// Au-delà de la fin, il ramène le décalage à la fin (comme un catalogue qui plafonne).
    fn faux_catalogue(vrai: u64, annonce: u64) -> impl FnMut(u64, u64) -> Result<Recherche, String> {
        move |decalage, nombre| {
            // Comme Modrinth : le décalage est arrondi à un multiple du nombre demandé.
            let decalage = decalage / nombre.max(1) * nombre.max(1);
            let decalage = if decalage >= vrai && decalage > 0 { vrai.saturating_sub(1) } else { decalage };
            let projets = (decalage..(decalage + nombre).min(vrai))
                .map(|i| Projet { id: format!("p{i}"), nom: String::new(), description: String::new(), couleur: None, icone: None, auteur: None, telechargements: vrai - i })
                .collect();
            Ok(Recherche { projets, total: annonce, decalage })
        }
    }

    #[test]
    fn lit_la_fin_de_la_liste() {
        let ids = |r: &(Vec<Projet>, u64, bool)| r.0.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        // Total juste : les 20 derniers, le dernier d'abord.
        let r = fin_de_liste(0, faux_catalogue(45, 45)).unwrap();
        assert_eq!((r.0.len(), r.0[0].id.as_str(), r.0[19].id.as_str(), r.1, r.2), (20, "p44", "p25", 45, true));
        let r = fin_de_liste(2, faux_catalogue(45, 45)).unwrap();
        assert_eq!((ids(&r), r.2), ((0..5).rev().map(|i| format!("p{i}")).collect::<Vec<_>>(), false));
        assert_eq!(fin_de_liste(3, faux_catalogue(45, 45)).unwrap().0.len(), 0);
        // Total annoncé trop grand (12 929 annoncés, 12 738 en vrai) : on retombe sur les vrais derniers.
        let r = fin_de_liste(0, faux_catalogue(12_738, 12_929)).unwrap();
        assert_eq!((r.0[0].id.as_str(), r.0.len(), r.1), ("p12737", 20, 12_738));
        // Total annoncé trop petit.
        let mut n = 0;
        let mut cat = faux_catalogue(100, 90);
        let r = fin_de_liste(0, move |d, k| {
            n += 1;
            let mut x = cat(d, k)?;
            if n > 1 {
                x.total = 100;
            }
            Ok(x)
        })
        .unwrap();
        assert_eq!((r.0[0].id.as_str(), r.1), ("p99", 100));
        // Un catalogue vide.
        assert_eq!(fin_de_liste(0, faux_catalogue(0, 0)).unwrap().0.len(), 0);
        // Le vrai cas vu chez Thomas : 12 826 plugins, total juste, décalage arrondi.
        let r = fin_de_liste(0, faux_catalogue(12_826, 12_826)).unwrap();
        assert_eq!((r.0[0].id.as_str(), r.0[19].id.as_str(), r.0.len(), r.2), ("p12825", "p12806", 20, true));
        let r = fin_de_liste(641, faux_catalogue(12_826, 12_826)).unwrap();
        assert_eq!((ids(&r), r.2), (vec!["p5".to_string(), "p4".into(), "p3".into(), "p2".into(), "p1".into(), "p0".into()], false));
        // Page 2 avec un total faux.
        let r = fin_de_liste(1, faux_catalogue(12_738, 12_929)).unwrap();
        assert_eq!((r.0[0].id.as_str(), r.0.len()), ("p12717", 20));
    }

    fn v(id: &str) -> VersionMr {
        VersionMr {
            id: id.into(),
            projet: "P".into(),
            numero: id.into(),
            stable: true,
            jeux: vec![],
            loaders: vec![],
            fichier: FichierMr { url: String::new(), nom: format!("{id}.jar"), sha1: String::new(), sha512: String::new() },
            dependances: vec![],
            incompatibles: vec![],
        }
    }

    #[test]
    fn choisir_avance_au_2e_essai_et_garde_le_java_demande() {
        // 9 versions pour Java 25, puis une pour Java 21 ; le serveur est en Java 21.
        let versions: Vec<VersionMr> = (0..10).map(|i| v(&format!("V{i}"))).collect();
        let java = |x: &VersionMr| if x.id == "V9" { 21 } else { 25 };
        let mut refusees = HashMap::new();
        let mut telecharges = 0;
        let mut dl = |x: &VersionMr| {
            telecharges += 1;
            Ok(((), Some(java(x))))
        };
        // 1er clic : 8 téléchargements, tous trop récents.
        assert!(matches!(choisir_version(versions.clone(), None, Some(21), &mut refusees, 8, &mut dl), Ok(Essai::TropRecente(25))));
        // 2e clic : les 8 déjà refusées ne comptent plus : on atteint V9.
        match choisir_version(versions.clone(), None, Some(21), &mut refusees, 8, &mut dl) {
            Ok(Essai::Version(x, ())) => assert_eq!(x.id, "V9"),
            _ => panic!("V9 attendue"),
        }
        assert_eq!(telecharges, 10);
        // Tout est refusé et en mémoire : on redit « Java 25 », sans rien télécharger.
        let mut rien = |_: &VersionMr| -> Result<((), Option<u32>), String> { panic!("pas de téléchargement") };
        assert!(matches!(choisir_version(versions[..9].to_vec(), None, Some(21), &mut refusees, 8, &mut rien), Ok(Essai::TropRecente(25))));
        // La version actuelle arrête tout.
        assert!(matches!(choisir_version(versions.clone(), Some("V0"), Some(21), &mut HashMap::new(), 8, &mut rien), Ok(Essai::DejaLa)));
        // Mise à jour : seulement ce qui est sorti après l'actuelle.
        let ids = |l: Vec<VersionMr>| l.into_iter().map(|x| x.id).collect::<Vec<_>>();
        assert_eq!(ids(plus_recentes(versions.clone(), "V2")), ["V0", "V1"]);
        assert_eq!(ids(plus_recentes(versions[..2].to_vec(), "inconnue")), ["V0", "V1"]);
    }

    #[test]
    fn lit_l_identifiant_dans_un_telechargement() {
        let j = faux_jar(&[("plugin.yml", b"name: ProtocolLib\nmain: a.B\n")]);
        assert_eq!(id_dans(&j).as_deref(), Some("protocollib"));
        let j = faux_jar(&[("fabric.mod.json", br#"{"id":"sodium","name":"Sodium"}"#)]);
        assert_eq!(id_dans(&j).as_deref(), Some("sodium"));
        assert_eq!(id_dans(b"pas un zip"), None);
    }

    #[test]
    fn repere_les_doublons() {
        let f = |n: &str, c: Option<&str>, a: bool| (n.to_string(), c.map(String::from), a);
        let d = doublons(&[
            f("WorldEdit-7.3.jar", Some("worldedit"), true),
            f("worldedit-bukkit-7.4.2.jar", Some("worldedit"), true),
            f("Vault.jar", Some("vault"), true),
            f("Vault-old.jar.disabled", Some("vault"), false),
            f("inconnu.jar", None, true),
            f("autre.jar", None, true),
        ]);
        assert_eq!(d.len(), 2);
        assert_eq!(d["WorldEdit-7.3.jar"], "worldedit-bukkit-7.4.2.jar");
        assert_eq!(d["worldedit-bukkit-7.4.2.jar"], "WorldEdit-7.3.jar");
    }

    #[test]
    fn sait_ou_vont_les_datapacks() {
        assert!(datapacks_possibles("1.13") && datapacks_possibles("1.21.11") && datapacks_possibles("26.1") && datapacks_possibles("24w14a"));
        assert!(!datapacks_possibles("1.12.2") && !datapacks_possibles("1.8.8") && !datapacks_possibles(""));
        assert_eq!(genres("Paper", "1.21.4"), [Genre::Plugins, Genre::Datapacks]);
        assert_eq!(genres("Spigot", "1.8.8"), [Genre::Plugins]);
        assert_eq!(genres("Fabric", "1.21.1"), [Genre::Mods, Genre::Datapacks]);
        assert_eq!(genres("Vanilla", "1.21.4"), [Genre::Datapacks]);
        assert_eq!(genres("Mohist", "1.20.1"), [Genre::Mods, Genre::Plugins, Genre::Datapacks]);
        assert_eq!(genres("Mohist", "1.12.2"), [Genre::Mods, Genre::Plugins]);
        assert!(genres("Inconnu", "1.20.1").is_empty());
        assert_eq!(loaders_genre(Genre::Datapacks, "Paper", "server.jar"), ["datapack"]);
        assert_eq!(loaders_genre(Genre::Plugins, "Paper", "server.jar"), ["paper", "spigot", "bukkit"]);
        assert_eq!(loaders_genre(Genre::Mods, "Youer", "server.jar"), ["neoforge"]);
        assert_eq!(loaders_genre(Genre::Mods, "Mohist", "server.jar"), ["forge"]);
        assert_eq!(loaders_genre(Genre::Mods, "Arclight", "arclight-fabric-1.21.1-1.0.1-8ec9529.jar"), ["fabric"]);
        assert_eq!(loaders_genre(Genre::Mods, "Arclight", "arclight-neoforge-1.21.1-1.0.1.jar"), ["neoforge"]);
        assert_eq!(loaders_genre(Genre::Plugins, "Arclight", "arclight-forge-1.20.1-1.0.6.jar"), ["spigot", "bukkit"]);
        assert_eq!(Genre::depuis_id("datapacks"), Some(Genre::Datapacks));
    }

    #[test]
    fn lit_le_java_d_un_jar() {
        // Plugin : la classe principale décide, même si une autre classe est plus récente.
        let j = faux_jar(&[
            ("plugin.yml", b"name: WorldEdit\nmain: com.sk89q.Plugin\nversion: 7.4\n"),
            ("com/sk89q/Plugin.class", &classe(69)),
            ("com/sk89q/Autre.class", &classe(52)),
        ]);
        assert_eq!(java_du_jar(&j), Some(25));
        // Mod Fabric : le point d'entrée.
        let j = faux_jar(&[("fabric.mod.json", br#"{"entrypoints":{"main":[{"value":"a.b.Mod"}]}}"#), ("a/b/Mod.class", &classe(65))]);
        assert_eq!(java_du_jar(&j), Some(21));
        // Sinon : la plus haute des classes, sans celles de META-INF/versions.
        let j = faux_jar(&[("x/A.class", &classe(52)), ("x/B.class", &classe(61)), ("META-INF/versions/21/x/A.class", &classe(65))]);
        assert_eq!(java_du_jar(&j), Some(17));
        assert_eq!(java_du_jar(b"pas un zip"), None);
        assert_eq!(java_du_jar(&faux_jar(&[("pack.mcmeta", b"{}")])), None);
    }

    #[test]
    fn repere_un_monde_casse_par_un_datapack_retire() {
        assert_eq!(lire_panne("15:00:25 WARN Missing data pack file/Terralith_1.21.11_v2.6.0.zip"), Some(Panne::PackManquant("Terralith_1.21.11_v2.6.0.zip".into())));
        assert_eq!(lire_panne("[15:00:25 WARN]: Missing data pack file/Terralith_1.21.11_v2.6.0.zip"), Some(Panne::PackManquant("Terralith_1.21.11_v2.6.0.zip".into())));
        assert_eq!(
            lire_panne("15:00:27 WARN Failed to load datapacks, can't proceed with server load. You can either fix your datapacks or reset to vanilla with --safeMode"),
            Some(Panne::MondeBloque)
        );
        assert_eq!(lire_panne("[15:00:25 INFO]: <KOTORO_YT> Missing data pack file/x.zip"), None);
        assert_eq!(nom_probable("Terralith_1.21.11_v2.6.0.zip"), "Terralith");
        assert_eq!(nom_probable("Incendium_1.21.x_v5.4.4.zip.disabled"), "Incendium");
        assert_eq!(nom_probable("dungeons-and-taverns-v4.4.4.zip"), "dungeons and taverns");
        assert_eq!(nom_probable("Tectonic-datapack-3.0.zip"), "Tectonic datapack");
    }

    #[test]
    fn sait_si_un_datapack_change_le_monde() {
        let d = dossier("monde");
        jar(&d.join("terre.zip"), &[("pack.mcmeta", "{}"), ("data/terralith/worldgen/biome/x.json", "{}")]);
        jar(&d.join("dim.zip"), &[("pack.mcmeta", "{}"), ("data/minecraft/dimension/overworld.json", "{}")]);
        jar(&d.join("recettes.zip"), &[("pack.mcmeta", "{}"), ("data/moi/recipe/x.json", "{}"), ("data/moi/functions/worldgen.mcfunction", "")]);
        assert!(change_le_monde(&d.join("terre.zip")));
        assert!(change_le_monde(&d.join("dim.zip")));
        assert!(!change_le_monde(&d.join("recettes.zip")));
        fs::create_dir_all(d.join("dossier/data/moi/worldgen")).unwrap();
        assert!(change_le_monde(&d.join("dossier")));
        // Les datapacks notés dans level.dat.
        use crate::nbt::{ecrire::*, Nbt};
        let niveau = groupe(vec![("Data", groupe(vec![("DataPacks", groupe(vec![("Enabled", Nbt::Liste(vec![Nbt::Texte("vanilla".into()), Nbt::Texte("file/terre.zip".into())]))]))]))]);
        fs::write(d.join("level.dat"), fichier(&niveau)).unwrap();
        assert_eq!(packs_du_monde(&d), vec!["vanilla", "file/terre.zip"]);
        assert!(packs_du_monde(&d.join("rien")).is_empty());
    }

    #[test]
    fn repere_les_plugins_en_panne() {
        // Paper.
        assert_eq!(
            lire_panne("[10:42:01 ERROR]: Could not load plugin 'worldedit-bukkit-7.4.6.jar' in folder 'plugins'"),
            Some(Panne::Fichier("worldedit-bukkit-7.4.6.jar".into()))
        );
        assert_eq!(
            lire_panne("org.bukkit.plugin.InvalidPluginException: java.lang.UnsupportedClassVersionError: com/sk89q/worldedit/bukkit/WorldEditPlugin has been compiled by a more recent version of the Java Runtime (class file version 69.0), this version of the Java Runtime only recognizes class file versions up to 65.0"),
            Some(Panne::Raison("Il lui faut Java 25.".into()))
        );
        // Spigot 1.8.
        assert_eq!(lire_panne("[12:00:00 ERROR]: Could not load 'plugins\\Essentials.jar' in folder 'plugins'"), Some(Panne::Fichier("essentials.jar".into())));
        assert_eq!(
            lire_panne("org.bukkit.plugin.UnknownDependencyException: Unknown dependency Vault. Please download and install Vault to run this plugin. [Vault]"),
            Some(Panne::Raison("Il lui faut Vault.".into()))
        );
        assert_eq!(lire_panne("Caused by: Unknown/missing dependency plugins: [Vault, ProtocolLib]"), Some(Panne::Raison("Il lui faut Vault, ProtocolLib.".into())));
        assert_eq!(lire_panne("[ERROR]: Error occurred while enabling WorldEdit v7.4.6+7429-abc (Is it up to date?)"), Some(Panne::Nom("worldedit".into())));
        assert_eq!(lire_panne("[10:42:01 ERROR]: Error occurred (in the plugin loader) while enabling My Plugin v1.0 (Is it up to date?)"), Some(Panne::Nom("my plugin".into())));
        assert_eq!(lire_panne("org.bukkit.plugin.InvalidPluginException: Unsupported API version 1.21.5"), Some(Panne::Raison("Prévu pour un Minecraft plus récent.".into())));
        assert_eq!(lire_panne("[10:42:01 INFO]: [WorldEdit] Enabling WorldEdit v7.4.6"), None);
        // Le chat ne compte pas.
        assert_eq!(lire_panne("[10:42:05 INFO]: <KOTORO_YT> ERROR Could not load 'WorldEdit.jar' lol"), None);
        assert_eq!(lire_panne("[10:42:05 INFO]: <KOTORO_YT> class file version 69.0"), None);
        assert_eq!(lire_panne("16:13:45 INFO <KOTORO_YT> ERROR Could not load 'WorldEdit.jar' lol"), None);
    }

    #[test]
    fn repere_les_plugins_qui_plantent_en_jeu() {
        // SkinsRestorer 15 sur Spigot 1.8.8 (vu chez Thomas le 18/11).
        assert_eq!(
            lire_panne("16:13:38 WARN org.apache.commons.lang.UnhandledException: Plugin SkinsRestorer v15.12.6 generated an exception while executing task 9"),
            Some(Panne::Plante("skinsrestorer".into()))
        );
        assert_eq!(
            lire_panne("Caused by: java.lang.NoSuchMethodError: org.bukkit.command.CommandSender.spigot()Lorg/bukkit/command/CommandSender$Spigot;"),
            Some(Panne::Raison("Pas fait pour cette version de Minecraft.".into()))
        );
        assert_eq!(lire_panne("16:13:47 ERROR [SkinsRestorer] An unhandled exception was thrown during command execution"), Some(Panne::Plante("skinsrestorer".into())));
        assert_eq!(lire_panne("[16:13:47 ERROR]: [SkinsRestorer] An unhandled exception was thrown during command execution"), Some(Panne::Plante("skinsrestorer".into())));
        assert_eq!(lire_panne("[12:00:00 ERROR]: Could not pass event PlayerJoinEvent to Essentials v2.21.0"), Some(Panne::Plante("essentials".into())));
        assert_eq!(
            lire_panne("[12:00:00 ERROR]: [STDERR] org.bukkit.command.CommandException: Unhandled exception executing command 'home' in plugin My Homes v1.2"),
            Some(Panne::Plante("my homes".into()))
        );
        assert_eq!(lire_panne("java.lang.NoClassDefFoundError: net/minecraft/server/v1_21_R1/EntityPlayer"), Some(Panne::Raison("Pas fait pour cette version de Minecraft.".into())));
        // Ce qui n'est pas la faute de Minecraft, et le chat.
        assert_eq!(lire_panne("java.lang.NoSuchMethodError: com.example.Truc.machin()V"), None);
        assert_eq!(lire_panne("[12:00:00 ERROR]: [Server thread] Something went wrong"), None);
        assert_eq!(lire_panne("[12:00:00 ERROR]: [STDERR] java.lang.RuntimeException: boom"), None);
        assert_eq!(lire_panne("[12:00:00 INFO]: <KOTORO_YT> Plugin Bidon v1 generated an exception"), None);
        assert_eq!(lire_panne("16:13:45 INFO <KOTORO_YT> Plugin Bidon v1 generated an exception"), None);
    }

    #[test]
    fn verifie_les_telechargements() {
        let octets = b"abc";
        let f = FichierMr {
            url: String::new(),
            nom: String::new(),
            sha1: "a9993e364706816aba3e25717850c26c9cd0d89d".into(),
            sha512: String::new(),
        };
        assert!(verifier(octets, &f));
        assert!(!verifier(b"abd", &f));
        let f512 = FichierMr { sha512: hex(&Sha512::digest(octets)), ..f.clone() };
        assert!(verifier(octets, &f512));
        assert!(!verifier(b"x", &FichierMr { sha1: String::new(), ..f }));
    }

    #[test]
    fn garde_les_liens() {
        let d = dossier("liens");
        let f = d.join("extensions.json");
        let mut l = Liens::default();
        l.noter("s1", "FabricAPI.jar", "Sodium");
        assert_eq!(l.pour("s1", "FabricAPI.jar.disabled").as_deref(), Some("Sodium"));
        l.renommer("s1", "FabricAPI.jar", "FabricAPI-2.jar");
        assert_eq!(l.pour("s1", "FabricAPI.jar"), None);
        l.enregistrer(&f);
        let l2 = Liens::charger(&f);
        assert_eq!(l2.pour("s1", "FabricAPI-2.jar").as_deref(), Some("Sodium"));
        let mut l3 = l2;
        l3.oublier("s1");
        assert_eq!(l3.pour("s1", "FabricAPI-2.jar"), None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn cache_un_quart_d_heure() {
        let mut c = Cache::default();
        c.noter("ab", "Paper", "1.21.4", None, None, None);
        assert!(c.lire("ab", "Paper", "1.21.4").is_some());
        assert!(c.lire("ab", "Paper", "1.21.5").is_none());
    }
}
