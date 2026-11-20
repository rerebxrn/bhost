// La fiche d'un joueur (onglet Joueurs) : temps de jeu, première venue, morts, position.
// Tout est lu dans le monde : playerdata/<uuid>.dat (26.1+ : players/data) et stats/<uuid>.json (26.1+ : players/stats).
// Le serveur écrit ces fichiers de temps en temps (et à la déconnexion) : c'est « au dernier enregistrement ».
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use crate::nbt::{self, Nbt};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fiche {
    /// Temps passé sur le serveur, en secondes.
    pub temps_jeu: Option<u64>,
    /// Première venue (millisecondes depuis 1970).
    pub premiere: Option<u64>,
    pub morts: Option<u64>,
    /// Position (x, y, z), arrondie au bloc.
    pub position: Option<[i64; 3]>,
    /// overworld, the_nether, the_end…
    pub dimension: Option<String>,
    /// survie, creatif, aventure, spectateur.
    pub mode: Option<String>,
    /// Quand le serveur a écrit ces informations (millisecondes).
    pub enregistre: Option<u64>,
}

fn premier_existant(chemins: [PathBuf; 2]) -> Option<PathBuf> {
    chemins.into_iter().find(|p| p.is_file())
}

fn millisecondes(t: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    t.ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Les nombres d'un fichier de statistiques (1.13+ : rangés par catégorie ; avant : « stat.deaths »…).
fn statistique(json: &serde_json::Value, nouveaux: &[&str], ancien: &str) -> Option<u64> {
    let custom = json.get("stats").and_then(|s| s.get("minecraft:custom"));
    nouveaux.iter().find_map(|n| custom.and_then(|c| c.get(*n)).and_then(|v| v.as_u64())).or_else(|| json.get(ancien).and_then(|v| v.as_u64()))
}

/// La fiche d'un joueur (identifiant avec tirets) dans le monde `monde`. `None` s'il n'y est jamais venu.
pub fn lire(monde: &Path, uuid: &str) -> Option<Fiche> {
    // Seulement un vrai identifiant : il sert à fabriquer un chemin.
    crate::listes::avec_tirets(uuid).filter(|u| u == uuid)?;
    let dat = premier_existant([monde.join("players").join("data").join(format!("{uuid}.dat")), monde.join("playerdata").join(format!("{uuid}.dat"))]);
    let stats = premier_existant([monde.join("players").join("stats").join(format!("{uuid}.json")), monde.join("stats").join(format!("{uuid}.json"))]);
    if dat.is_none() && stats.is_none() {
        return None;
    }
    let mut fiche = Fiche::default();
    if let Some(chemin) = &stats {
        if let Some(json) = fs::read(chemin).ok().and_then(|o| serde_json::from_slice::<serde_json::Value>(&o).ok()) {
            fiche.temps_jeu = statistique(&json, &["minecraft:play_time", "minecraft:play_one_minute"], "stat.playOneMinute").map(|ticks| ticks / 20);
            fiche.morts = statistique(&json, &["minecraft:deaths"], "stat.deaths");
        }
    }
    if let Some(chemin) = &dat {
        let meta = fs::metadata(chemin).ok();
        fiche.enregistre = meta.as_ref().and_then(|m| millisecondes(m.modified()));
        fiche.premiere = meta.as_ref().and_then(|m| millisecondes(m.created()));
        if let Some(j) = nbt::lire_fichier(chemin) {
            let pos = j.get("Pos").and_then(Nbt::entiers).filter(|p| p.len() == 3);
            fiche.position = pos.map(|p| [p[0], p[1], p[2]]);
            fiche.dimension = match j.get("Dimension") {
                Some(Nbt::Texte(t)) => Some(t.trim_start_matches("minecraft:").to_string()),
                Some(autre) => autre.nombre().map(|n| match n {
                    -1 => "the_nether".to_string(),
                    1 => "the_end".to_string(),
                    _ => "overworld".to_string(),
                }),
                None => None,
            };
            fiche.mode = j.get("playerGameType").and_then(Nbt::nombre).and_then(|n| ["survie", "creatif", "aventure", "spectateur"].get(n as usize)).map(|m| m.to_string());
        }
    }
    Some(fiche)
}

/// Le point d'apparition du monde (x, y, z), lu dans level.dat (SpawnX… ou « spawn » depuis 1.21.9).
pub fn apparition(monde: &Path) -> Option<[i64; 3]> {
    let niveau = nbt::lire_fichier(&monde.join("level.dat"))?;
    let d = niveau.get("Data")?;
    if let Some(pos) = d.chemin(&["spawn", "pos"]).and_then(Nbt::entiers).filter(|p| p.len() == 3) {
        return Some([pos[0], pos[1], pos[2]]);
    }
    let n = |k: &str| d.get(k).and_then(Nbt::nombre);
    Some([n("SpawnX")?, n("SpawnY")?, n("SpawnZ")?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::ecrire::{fichier, groupe};

    #[test]
    fn lit_une_fiche_recente() {
        let monde = std::env::temp_dir().join(format!("bhost-fiche-{}", std::process::id()));
        let _ = fs::remove_dir_all(&monde);
        let uuid = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
        assert!(lire(&monde, uuid).is_none());
        fs::create_dir_all(monde.join("players/data")).unwrap();
        fs::create_dir_all(monde.join("players/stats")).unwrap();
        let joueur = groupe(vec![
            ("Pos", Nbt::Liste(vec![Nbt::Double(120.7), Nbt::Double(64.0), Nbt::Double(-339.2)])),
            ("Dimension", Nbt::Texte("minecraft:the_nether".into())),
            ("playerGameType", Nbt::Entier(1)),
        ]);
        fs::write(monde.join(format!("players/data/{uuid}.dat")), fichier(&joueur)).unwrap();
        fs::write(monde.join(format!("players/stats/{uuid}.json")), r#"{"stats":{"minecraft:custom":{"minecraft:play_time":2976000,"minecraft:deaths":12}},"DataVersion":4800}"#).unwrap();
        let f = lire(&monde, uuid).unwrap();
        assert_eq!(f.temps_jeu, Some(148_800)); // 41 h 20
        assert_eq!(f.morts, Some(12));
        assert_eq!(f.position, Some([120, 64, -340]));
        assert_eq!(f.dimension.as_deref(), Some("the_nether"));
        assert_eq!(f.mode.as_deref(), Some("creatif"));
        assert!(f.enregistre.is_some());
        let _ = fs::remove_dir_all(&monde);
    }

    #[test]
    fn lit_une_vieille_fiche() {
        let monde = std::env::temp_dir().join(format!("bhost-fiche-vieille-{}", std::process::id()));
        let _ = fs::remove_dir_all(&monde);
        fs::create_dir_all(monde.join("playerdata")).unwrap();
        fs::create_dir_all(monde.join("stats")).unwrap();
        let uuid = "b50ad385-829d-3141-a216-7e7d7539ba7f";
        let joueur = groupe(vec![("Dimension", Nbt::Entier(-1))]);
        fs::write(monde.join(format!("playerdata/{uuid}.dat")), fichier(&joueur)).unwrap();
        fs::write(monde.join(format!("stats/{uuid}.json")), r#"{"stat.playOneMinute":72000,"stat.deaths":3}"#).unwrap();
        let f = lire(&monde, uuid).unwrap();
        assert_eq!((f.temps_jeu, f.morts), (Some(3600), Some(3)));
        assert_eq!(f.dimension.as_deref(), Some("the_nether"));
        // Point d'apparition : ancien format, puis nouveau.
        let vieux = groupe(vec![("Data", groupe(vec![("SpawnX", Nbt::Entier(5)), ("SpawnY", Nbt::Entier(70)), ("SpawnZ", Nbt::Entier(-8))]))]);
        fs::write(monde.join("level.dat"), fichier(&vieux)).unwrap();
        assert_eq!(apparition(&monde), Some([5, 70, -8]));
        let neuf = groupe(vec![("Data", groupe(vec![("spawn", groupe(vec![("pos", Nbt::Entiers(vec![1, 2, 3]))]))]))]);
        fs::write(monde.join("level.dat"), fichier(&neuf)).unwrap();
        assert_eq!(apparition(&monde), Some([1, 2, 3]));
        let _ = fs::remove_dir_all(&monde);
    }
}
