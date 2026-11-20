// Les mondes d'un serveur (onglet Paramètres › Mondes) : les lister, en créer, en importer depuis le jeu solo.
// Un monde est un dossier du serveur qui contient « level.dat ». Le monde joué est celui de « level-name » (server.properties).
// Paper, Spigot… rangent le Nether et l'End à côté : « world_nether » et « world_the_end » font partie du monde « world ».
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use crate::nbt::{self, Nbt};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Le petit fichier posé dans un « Nouveau monde » pas encore joué (le serveur le créera au démarrage).
const MARQUE_NOUVEAU: &str = ".bhost-monde";

/// Un monde, pour l'interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Monde {
    /// Le nom de son dossier (c'est lui qui sert à le désigner).
    pub dossier: String,
    /// Le nom donné dans le jeu (LevelName), ou celui du dossier.
    pub nom: String,
    /// La dernière version de Minecraft qui l'a ouvert.
    pub version: Option<String>,
    /// Poids en octets (Nether et End compris).
    pub taille: u64,
    /// Dernière partie (millisecondes depuis 1970).
    pub joue: Option<u64>,
    /// survie, creatif, aventure, spectateur ou hardcore (mondes solo).
    pub mode: Option<String>,
    /// C'est le monde joué par le serveur.
    pub actuel: bool,
}

/// Le dossier du monde joué : « level-name » dans server.properties (« world » par défaut).
pub fn niveau(serveur: &Path) -> String {
    crate::serveurs::lire_proprietes(serveur)
        .into_iter()
        .find(|(k, _)| k == "level-name")
        .map(|(_, v)| v)
        .filter(|v| nom_valide(v))
        .unwrap_or_else(|| "world".into())
}

/// Un nom de dossier de monde acceptable : un seul dossier, sans chemin.
pub fn nom_valide(nom: &str) -> bool {
    !nom.is_empty()
        && nom.len() <= 100
        && nom != "."
        && nom != ".."
        && !nom.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
}

/// Le poids d'un dossier, en octets.
pub fn taille(dossier: &Path) -> u64 {
    taille_jusqu_a(dossier, 0)
}

fn taille_jusqu_a(dossier: &Path, profondeur: usize) -> u64 {
    if profondeur > crate::garde::PROFONDEUR_MAX {
        return 0;
    }
    let Ok(entrees) = fs::read_dir(dossier) else { return 0 };
    entrees
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => taille_jusqu_a(&e.path(), profondeur + 1),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Le dossier du monde, et ceux du Nether et de l'End à côté (Paper, Spigot…), s'ils existent.
pub fn dossiers(serveur: &Path, nom: &str) -> Vec<(PathBuf, String)> {
    [nom.to_string(), format!("{nom}_nether"), format!("{nom}_the_end")]
        .into_iter()
        .map(|n| (serveur.join(&n), n))
        .filter(|(d, _)| d.is_dir())
        .collect()
}

/// Ce que dit le level.dat d'un monde : nom, version, dernière partie, mode de jeu.
fn lire_infos(monde: &Path, sortie: &mut Monde) {
    let Some(niveau) = nbt::lire_fichier(&monde.join("level.dat")) else { return };
    let Some(d) = niveau.get("Data") else { return };
    if let Some(nom) = d.get("LevelName").and_then(Nbt::texte).filter(|n| !n.trim().is_empty()) {
        sortie.nom = nom.trim().to_string();
    }
    let version = d.get("Version");
    sortie.version = version.and_then(|v| v.get("Name").or_else(|| v.get("id"))).and_then(Nbt::texte).map(String::from);
    sortie.joue = d.get("LastPlayed").and_then(Nbt::nombre).and_then(|n| u64::try_from(n).ok());
    let hardcore = d.get("hardcore").and_then(Nbt::booleen).unwrap_or(false);
    sortie.mode = if hardcore {
        Some("hardcore".into())
    } else {
        d.get("GameType").and_then(Nbt::nombre).and_then(|n| ["survie", "creatif", "aventure", "spectateur"].get(n as usize)).map(|m| m.to_string())
    };
}

/// Lit un monde (dossier `nom` dans `parent`).
fn lire_monde(parent: &Path, nom: &str, actuel: bool, avec_voisins: bool) -> Monde {
    let dossier = parent.join(nom);
    let mut monde = Monde { dossier: nom.into(), nom: nom.into(), version: None, taille: 0, joue: None, mode: None, actuel };
    lire_infos(&dossier, &mut monde);
    monde.taille = if avec_voisins { dossiers(parent, nom).iter().map(|(d, _)| taille(d)).sum() } else { taille(&dossier) };
    monde
}

/// Les mondes d'un serveur : le monde joué en premier, puis les autres par nom.
pub fn lister(serveur: &Path) -> Vec<Monde> {
    let actuel = niveau(serveur);
    let mut noms: Vec<String> = fs::read_dir(serveur)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| nom_valide(n))
        .filter(|n| serveur.join(n).join("level.dat").is_file() || serveur.join(n).join(MARQUE_NOUVEAU).is_file())
        .collect();
    // world_nether et world_the_end vont avec world.
    let tous = noms.clone();
    noms.retain(|n| {
        let base = n.strip_suffix("_nether").or_else(|| n.strip_suffix("_the_end"));
        !base.is_some_and(|b| tous.iter().any(|x| x == b) || *b == actuel)
    });
    if !noms.contains(&actuel) {
        noms.push(actuel.clone()); // pas encore créé : le serveur le fera au démarrage
    }
    noms.sort_by_key(|n| (*n != actuel, n.to_lowercase()));
    noms.iter().map(|n| lire_monde(serveur, n, *n == actuel, true)).collect()
}

/// Les mondes du jeu solo (dossier « saves » du launcher), du plus récemment joué au plus ancien.
pub fn solo(saves: &Path) -> Vec<Monde> {
    let mut mondes: Vec<Monde> = fs::read_dir(saves)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("level.dat").is_file())
        .map(|e| lire_monde(saves, &e.file_name().to_string_lossy(), false, false))
        .collect();
    mondes.sort_by(|a, b| b.joue.cmp(&a.joue));
    mondes
}

/// Un nom de dossier libre dans le serveur : « ile-paradisiaque », sinon « ile-paradisiaque-2 »…
pub fn nom_libre(serveur: &Path, voulu: &str) -> String {
    let base = crate::creation::nom_de_dossier(voulu);
    let base = if base == "serveur" { "monde".to_string() } else { base };
    let libre = |n: &str| dossiers(serveur, n).is_empty() && !serveur.join(n).exists();
    (1..1000).map(|i| if i == 1 { base.clone() } else { format!("{base}-{i}") }).find(|n| libre(n)).unwrap_or(base)
}

/// Prépare un « Nouveau monde » : un dossier vide (le serveur le remplira quand il deviendra le monde joué).
pub fn nouveau(serveur: &Path) -> Result<String, String> {
    let nom = nom_libre(serveur, "nouveau-monde");
    let dossier = serveur.join(&nom);
    fs::create_dir(&dossier).map_err(|e| format!("Impossible de créer le monde ({e})."))?;
    marquer(&dossier)?;
    Ok(nom)
}

/// Marque un dossier vide comme monde à créer (il reste dans la liste).
pub fn marquer(dossier: &Path) -> Result<(), String> {
    fs::write(dossier.join(MARQUE_NOUVEAU), "Monde prepare par BHOST : le serveur le creera.\n").map_err(|e| format!("Impossible de créer le monde ({e})."))
}

/// Le dossier qui contient level.dat : `dossier` lui-même, ou un sous-dossier (archive .zip qui range tout dans un dossier).
pub fn racine(dossier: &Path) -> Option<PathBuf> {
    if dossier.join("level.dat").is_file() {
        return Some(dossier.to_path_buf());
    }
    let sous: Vec<PathBuf> = fs::read_dir(dossier).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    sous.iter().find(|p| p.join("level.dat").is_file()).cloned().or_else(|| sous.iter().find_map(|p| racine_profonde(p)))
}

fn racine_profonde(dossier: &Path) -> Option<PathBuf> {
    fs::read_dir(dossier).ok()?.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.join("level.dat").is_file())
}

/// Le nom du monde à importer (LevelName), ou celui du dossier.
pub fn nom_du_monde(monde: &Path) -> String {
    let mut m = Monde { dossier: String::new(), nom: String::new(), version: None, taille: 0, joue: None, mode: None, actuel: false };
    lire_infos(monde, &mut m);
    if m.nom.is_empty() {
        monde.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "monde".into())
    } else {
        m.nom
    }
}

/// La version de Minecraft qui a ouvert ce monde en dernier.
pub fn version_du_monde(monde: &Path) -> Option<String> {
    let mut m = Monde { dossier: String::new(), nom: String::new(), version: None, taille: 0, joue: None, mode: None, actuel: false };
    lire_infos(monde, &mut m);
    m.version
}

/// Copie un monde (dossier qui contient level.dat) dans le serveur. Renvoie le nom de son dossier.
pub fn copier(source: &Path, serveur: &Path, avance: &mut dyn FnMut(u8)) -> Result<String, String> {
    let nom = nom_libre(serveur, &nom_du_monde(source));
    let cible = serveur.join(&nom);
    fs::create_dir(&cible).map_err(|e| format!("Impossible de créer le dossier ({e})."))?;
    let resultat = crate::copie::copier(source, &cible, avance);
    if resultat.is_err() {
        let _ = fs::remove_dir_all(&cible);
    }
    resultat.map(|_| nom)
}

/// La graine du monde (pour le recréer à l'identique). Rangée à des endroits différents selon la version.
pub fn graine(monde: &Path) -> Option<i64> {
    if let Some(r) = nbt::lire_fichier(&monde.join("data").join("minecraft").join("world_gen_settings.dat")) {
        if let Some(g) = r.chemin(&["data", "seed"]).or_else(|| r.get("seed")).and_then(Nbt::nombre) {
            return Some(g);
        }
    }
    let niveau = nbt::lire_fichier(&monde.join("level.dat"))?;
    niveau.chemin(&["Data", "WorldGenSettings", "seed"]).or_else(|| niveau.chemin(&["Data", "RandomSeed"])).and_then(Nbt::nombre)
}

/// Les nombres d'une version « 1.21.4 » ou « 26.1.2 ». `None` pour une snapshot.
fn nombres(version: &str) -> Option<Vec<u32>> {
    version.trim().split('.').map(|n| n.parse().ok()).collect()
}

/// Vrai si un monde ouvert par `monde` est plus récent que le serveur `serveur` : il ne pourra pas s'ouvrir.
pub fn trop_recent(monde: &str, serveur: &str) -> bool {
    match (nombres(monde), nombres(serveur)) {
        (Some(m), Some(s)) => m > s,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::ecrire::{fichier, groupe};

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-mondes-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn level_dat(dossier: &Path, nom: &str, version: &str, joue: i64) {
        fs::create_dir_all(dossier).unwrap();
        let racine = groupe(vec![(
            "Data",
            groupe(vec![
                ("LevelName", Nbt::Texte(nom.into())),
                ("LastPlayed", Nbt::Long(joue)),
                ("GameType", Nbt::Entier(1)),
                ("Version", groupe(vec![("Name", Nbt::Texte(version.into()))])),
                ("WorldGenSettings", groupe(vec![("seed", Nbt::Long(-42))])),
            ]),
        )]);
        fs::write(dossier.join("level.dat"), fichier(&racine)).unwrap();
    }

    #[test]
    fn liste_les_mondes_d_un_serveur() {
        let s = dossier("liste");
        fs::write(s.join("server.properties"), "level-name=survie\nmotd=x\n").unwrap();
        level_dat(&s.join("survie"), "Survie", "1.21.4", 10);
        level_dat(&s.join("survie_nether"), "Survie", "1.21.4", 10);
        fs::write(s.join("survie_nether").join("gros"), vec![0u8; 100]).unwrap();
        level_dat(&s.join("ancien"), "Monde de l'été", "1.21.1", 5);
        fs::create_dir_all(s.join("plugins")).unwrap();
        fs::create_dir_all(s.join("logs")).unwrap();
        let liste = lister(&s);
        assert_eq!(liste.iter().map(|m| m.dossier.as_str()).collect::<Vec<_>>(), ["survie", "ancien"]);
        assert!(liste[0].actuel && !liste[1].actuel);
        assert_eq!(liste[1].nom, "Monde de l'été");
        assert_eq!(liste[0].version.as_deref(), Some("1.21.4"));
        assert_eq!(liste[0].mode.as_deref(), Some("creatif"));
        assert!(liste[0].taille >= 100); // le Nether compte
        assert_eq!(graine(&s.join("survie")), Some(-42));
        // Un nouveau monde vide apparaît, avec un nom libre.
        assert_eq!(nouveau(&s).unwrap(), "nouveau-monde");
        assert_eq!(nouveau(&s).unwrap(), "nouveau-monde-2");
        assert_eq!(lister(&s).len(), 4);
        let _ = fs::remove_dir_all(&s);
    }

    #[test]
    fn monde_joue_pas_encore_cree() {
        let s = dossier("neuf");
        let liste = lister(&s);
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0], Monde { dossier: "world".into(), nom: "world".into(), version: None, taille: 0, joue: None, mode: None, actuel: true });
        let _ = fs::remove_dir_all(&s);
    }

    #[test]
    fn importe_un_monde_solo() {
        let d = dossier("solo");
        let saves = d.join("saves");
        level_dat(&saves.join("Ile"), "Île paradisiaque", "26.1", 200);
        level_dat(&saves.join("Base"), "Ma première base", "1.21.4", 300);
        fs::write(saves.join("Ile").join("session.lock"), b"x").unwrap();
        fs::create_dir_all(saves.join("Pas un monde")).unwrap();
        let liste = solo(&saves);
        assert_eq!(liste.iter().map(|m| m.nom.as_str()).collect::<Vec<_>>(), ["Ma première base", "Île paradisiaque"]);
        let serveur = d.join("serveur");
        fs::create_dir_all(&serveur).unwrap();
        let nom = copier(&saves.join("Ile"), &serveur, &mut |_| {}).unwrap();
        assert_eq!(nom, "ile-paradisiaque");
        assert!(serveur.join("ile-paradisiaque").join("level.dat").is_file());
        assert!(!serveur.join("ile-paradisiaque").join("session.lock").exists());
        assert_eq!(copier(&saves.join("Ile"), &serveur, &mut |_| {}).unwrap(), "ile-paradisiaque-2");
        // Une archive qui range le monde dans un sous-dossier.
        let zip = d.join("zip");
        level_dat(&zip.join("MonMonde"), "Zip", "1.20.1", 1);
        assert_eq!(racine(&zip), Some(zip.join("MonMonde")));
        assert_eq!(racine(&saves.join("Pas un monde")), None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compare_les_versions() {
        assert!(trop_recent("26.1", "1.21.11"));
        assert!(trop_recent("1.21.4", "1.21.1"));
        assert!(!trop_recent("1.21.1", "1.21.4"));
        assert!(!trop_recent("1.21.4", "1.21.4"));
        assert!(!trop_recent("26.4-snapshot-1", "26.3"));
    }

    #[test]
    fn refuse_les_noms_dangereux() {
        assert!(nom_valide("world") && nom_valide("Monde de l'été"));
        assert!(!nom_valide("..") && !nom_valide("a/b") && !nom_valide("a\\b") && !nom_valide("C:") && !nom_valide(""));
    }
}
