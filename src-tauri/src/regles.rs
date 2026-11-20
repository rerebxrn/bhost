// Les règles du jeu (« gamerules ») de l'onglet Paramètres.
// Leurs noms ont changé en 1.21.11 : « keepInventory » est devenu « keep_inventory », « doFireTick » un rayon en blocs…
// On lit leur valeur dans le monde, et on les change avec la commande « gamerule » (tout de suite si le serveur tourne).
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use crate::nbt::{self, Nbt};
use serde::Serialize;
use std::path::Path;

/// Comment une règle se règle.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Genre {
    /// Vrai ou faux.
    Interrupteur,
    /// Vrai ou faux, mais dans l'autre sens (« Afficher les coordonnées » = reducedDebugInfo à faux).
    Inverse,
    /// Avant 1.21.11 : vrai ou faux. Depuis : un rayon en blocs (0 = le feu ne se propage plus).
    Feu,
    /// Un nombre de 0 à 100.
    Pourcent,
}

struct Regle {
    /// Le nom donné par BHOST (le même pour toutes les versions).
    cle: &'static str,
    ancien: &'static str,
    nouveau: &'static str,
    /// La première version qui l'a.
    depuis: [u32; 3],
    genre: Genre,
    /// La valeur de Minecraft quand le monde ne dit rien (1 = vrai).
    defaut: i64,
}

/// Rayon du feu par défaut depuis la 1.21.11.
const RAYON_FEU: i64 = 128;

/// Dans l'ordre de la maquette.
const REGLES: [Regle; 9] = [
    Regle { cle: "garder", ancien: "keepInventory", nouveau: "keep_inventory", depuis: [1, 4, 2], genre: Genre::Interrupteur, defaut: 0 },
    Regle { cle: "jour", ancien: "doDaylightCycle", nouveau: "advance_time", depuis: [1, 6, 1], genre: Genre::Interrupteur, defaut: 1 },
    Regle { cle: "meteo", ancien: "doWeatherCycle", nouveau: "advance_weather", depuis: [1, 11, 0], genre: Genre::Interrupteur, defaut: 1 },
    Regle { cle: "degats", ancien: "mobGriefing", nouveau: "mob_griefing", depuis: [1, 4, 2], genre: Genre::Interrupteur, defaut: 1 },
    Regle { cle: "feu", ancien: "doFireTick", nouveau: "fire_spread_radius_around_player", depuis: [1, 4, 2], genre: Genre::Feu, defaut: 1 },
    Regle { cle: "monstres", ancien: "doMobSpawning", nouveau: "spawn_mobs", depuis: [1, 4, 2], genre: Genre::Interrupteur, defaut: 1 },
    Regle { cle: "progres", ancien: "announceAdvancements", nouveau: "show_advancement_messages", depuis: [1, 12, 0], genre: Genre::Interrupteur, defaut: 1 },
    Regle { cle: "coordonnees", ancien: "reducedDebugInfo", nouveau: "reduced_debug_info", depuis: [1, 8, 0], genre: Genre::Inverse, defaut: 1 },
    Regle { cle: "dormir", ancien: "playersSleepingPercentage", nouveau: "players_sleeping_percentage", depuis: [1, 17, 0], genre: Genre::Pourcent, defaut: 100 },
];

/// Ce que l'onglet affiche : chaque règle que la version connaît, avec sa valeur (1/0 pour vrai/faux, ou un nombre).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegleVue {
    pub cle: &'static str,
    pub valeur: i64,
}

/// Les nombres d'une version : « 1.21.11 » → [1, 21, 11], « 26.4-snapshot-8 » → [26, 4, 0], « 1.21.11-pre1 » → [1, 21, 11].
/// Les snapshots « 25w44a » donnent [année, semaine] précédés de 0 : [0, 25, 44].
fn nombres(version: &str) -> Option<[u32; 3]> {
    let base = version.trim().split('-').next().unwrap_or("");
    if let Some((annee, reste)) = base.split_once('w') {
        let semaine: String = reste.chars().take_while(|c| c.is_ascii_digit()).collect();
        return Some([0, annee.parse().ok()?, semaine.parse().ok()?]);
    }
    let n: Vec<u32> = base.split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    Some([*n.first()?, n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0)])
}

/// Vrai si la version utilise les nouveaux noms (1.21.11, snapshot 25w44a, et après).
fn nouveaux_noms(version: &str) -> bool {
    match nombres(version) {
        Some([0, annee, semaine]) => (annee, semaine) >= (25, 44),
        Some(n) => n >= [1, 21, 11],
        None => false,
    }
}

/// Vrai si la version a cette règle. Une version inconnue les a toutes ; une snapshot « 21w… » ou plus récente aussi.
fn connue(version: &str, regle: &Regle) -> bool {
    match nombres(version) {
        Some([0, annee, _]) => annee >= 21 || regle.depuis < [1, 17, 0],
        Some(n) => n >= regle.depuis,
        None => true,
    }
}

/// La valeur d'une règle lue dans le monde, ramenée à la façon de BHOST (1/0, ou un nombre).
fn valeur_lue(regle: &Regle, lue: &Nbt) -> Option<i64> {
    Some(match regle.genre {
        Genre::Interrupteur => i64::from(lue.booleen()?),
        Genre::Inverse => i64::from(!lue.booleen()?),
        // Ancien nom : vrai/faux. Nouveau : un rayon (0 = éteint).
        Genre::Feu => i64::from(match lue {
            Nbt::Texte(t) if t == "true" || t == "false" => t == "true",
            autre => autre.nombre()? != 0,
        }),
        Genre::Pourcent => lue.nombre()?,
    })
}

/// Les règles écrites dans le monde, où qu'elles soient rangées selon la version :
/// 26.1 et après : data/minecraft/game_rules.dat ; avant : dans level.dat (« game_rules » ou « GameRules »).
fn regles_du_monde(monde: &Path) -> Option<Nbt> {
    if let Some(racine) = nbt::lire_fichier(&monde.join("data").join("minecraft").join("game_rules.dat")) {
        return Some(racine.get("data").cloned().unwrap_or(racine));
    }
    let niveau = nbt::lire_fichier(&monde.join("level.dat"))?;
    let donnees = niveau.get("Data")?;
    donnees.get("game_rules").or_else(|| donnees.get("GameRules")).cloned()
}

/// Les règles que la version connaît, avec leur valeur dans le monde `monde` (ou celle par défaut).
pub fn lire(monde: &Path, version: &str) -> Vec<RegleVue> {
    let ecrites = regles_du_monde(monde);
    REGLES
        .iter()
        .filter(|r| connue(version, r))
        .map(|r| {
            let lue = ecrites.as_ref().and_then(|e| {
                [r.nouveau.to_string(), format!("minecraft:{}", r.nouveau), r.ancien.to_string()].iter().find_map(|nom| e.get(nom)).cloned()
            });
            RegleVue { cle: r.cle, valeur: lue.and_then(|l| valeur_lue(r, &l)).unwrap_or(r.defaut) }
        })
        .collect()
}

/// Vrai si `cle` est une règle de BHOST et `valeur` lui convient.
pub fn valide(cle: &str, valeur: i64) -> bool {
    REGLES.iter().find(|r| r.cle == cle).is_some_and(|r| match r.genre {
        Genre::Pourcent => (0..=100).contains(&valeur),
        _ => valeur == 0 || valeur == 1,
    })
}

/// La commande à taper dans la console du serveur. `None` si la version n'a pas cette règle.
pub fn commande(version: &str, cle: &str, valeur: i64) -> Option<String> {
    let r = REGLES.iter().find(|r| r.cle == cle).filter(|r| connue(version, r))?;
    let nouveau = nouveaux_noms(version);
    let nom = if nouveau { r.nouveau } else { r.ancien };
    let texte = match r.genre {
        Genre::Interrupteur => (valeur != 0).to_string(),
        Genre::Inverse => (valeur == 0).to_string(),
        Genre::Feu if nouveau => (if valeur != 0 { RAYON_FEU } else { 0 }).to_string(),
        Genre::Feu => (valeur != 0).to_string(),
        Genre::Pourcent => valeur.clamp(0, 100).to_string(),
    };
    Some(format!("gamerule {nom} {texte}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::ecrire::{fichier, groupe};
    use std::fs;

    fn valeurs(v: &[RegleVue]) -> Vec<(&'static str, i64)> {
        v.iter().map(|r| (r.cle, r.valeur)).collect()
    }

    #[test]
    fn lit_les_versions() {
        assert_eq!(nombres("1.21.11"), Some([1, 21, 11]));
        assert_eq!(nombres("1.8"), Some([1, 8, 0]));
        assert_eq!(nombres("26.4-snapshot-8"), Some([26, 4, 0]));
        assert_eq!(nombres("1.21.11-pre2"), Some([1, 21, 11]));
        assert_eq!(nombres("25w44a"), Some([0, 25, 44]));
        assert_eq!(nombres(""), None);
        assert!(nouveaux_noms("1.21.11") && nouveaux_noms("26.3") && nouveaux_noms("25w44a") && nouveaux_noms("26.4-snapshot-1"));
        assert!(!nouveaux_noms("1.21.10") && !nouveaux_noms("1.8.8") && !nouveaux_noms("25w43a") && !nouveaux_noms(""));
    }

    #[test]
    fn ecrit_la_bonne_commande() {
        assert_eq!(commande("1.8.8", "garder", 1).unwrap(), "gamerule keepInventory true");
        assert_eq!(commande("26.3", "garder", 0).unwrap(), "gamerule keep_inventory false");
        assert_eq!(commande("1.21.11", "jour", 0).unwrap(), "gamerule advance_time false");
        assert_eq!(commande("1.21.4", "feu", 0).unwrap(), "gamerule doFireTick false");
        assert_eq!(commande("26.3", "feu", 0).unwrap(), "gamerule fire_spread_radius_around_player 0");
        assert_eq!(commande("26.3", "feu", 1).unwrap(), "gamerule fire_spread_radius_around_player 128");
        // « Afficher les coordonnées » = reducedDebugInfo à faux.
        assert_eq!(commande("1.20.1", "coordonnees", 1).unwrap(), "gamerule reducedDebugInfo false");
        assert_eq!(commande("26.3", "coordonnees", 0).unwrap(), "gamerule reduced_debug_info true");
        assert_eq!(commande("1.20.1", "dormir", 50).unwrap(), "gamerule playersSleepingPercentage 50");
        // Règles trop récentes pour la version.
        assert!(commande("1.8.8", "meteo", 1).is_none());
        assert!(commande("1.16.5", "dormir", 50).is_none());
        assert!(commande("1.8.8", "inconnue", 1).is_none());
    }

    #[test]
    fn verifie_les_valeurs() {
        assert!(valide("garder", 1) && valide("garder", 0) && valide("dormir", 0) && valide("dormir", 100));
        assert!(!valide("garder", 2) && !valide("dormir", 101) && !valide("dormir", -1) && !valide("rien", 1));
    }

    fn dossier(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-regles-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lit_les_regles_d_un_vieux_monde() {
        let d = dossier("vieux");
        let rien = lire(&d, "1.8.8");
        // Pas de level.dat : les valeurs par défaut. La 1.8 n'a ni météo, ni progrès, ni sommeil.
        assert_eq!(valeurs(&rien), [("garder", 0), ("jour", 1), ("degats", 1), ("feu", 1), ("monstres", 1), ("coordonnees", 1)]);
        let niveau = groupe(vec![(
            "Data",
            groupe(vec![(
                "GameRules",
                groupe(vec![
                    ("keepInventory", Nbt::Texte("true".into())),
                    ("doFireTick", Nbt::Texte("false".into())),
                    ("reducedDebugInfo", Nbt::Texte("true".into())),
                ]),
            )]),
        )]);
        fs::write(d.join("level.dat"), fichier(&niveau)).unwrap();
        let lues = lire(&d, "1.8.8");
        assert_eq!(valeurs(&lues), [("garder", 1), ("jour", 1), ("degats", 1), ("feu", 0), ("monstres", 1), ("coordonnees", 0)]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn lit_les_regles_d_un_monde_recent() {
        // 1.21.11 : « game_rules » dans level.dat, avec des valeurs typées.
        let d = dossier("recent");
        let niveau = groupe(vec![(
            "Data",
            groupe(vec![(
                "game_rules",
                groupe(vec![
                    ("minecraft:keep_inventory", Nbt::Octet(1)),
                    ("minecraft:fire_spread_radius_around_player", Nbt::Entier(0)),
                    ("minecraft:players_sleeping_percentage", Nbt::Entier(30)),
                ]),
            )]),
        )]);
        fs::write(d.join("level.dat"), fichier(&niveau)).unwrap();
        let lues = valeurs(&lire(&d, "1.21.11"));
        assert!(lues.contains(&("garder", 1)) && lues.contains(&("feu", 0)) && lues.contains(&("dormir", 30)) && lues.contains(&("meteo", 1)));
        assert_eq!(lues.len(), 9);
        // 26.1 et après : data/minecraft/game_rules.dat.
        let dat = d.join("data").join("minecraft");
        fs::create_dir_all(&dat).unwrap();
        let regles = groupe(vec![("DataVersion", Nbt::Entier(4800)), ("data", groupe(vec![("advance_time", Nbt::Octet(0)), ("reduced_debug_info", Nbt::Octet(1))]))]);
        fs::write(dat.join("game_rules.dat"), fichier(&regles)).unwrap();
        let lues = valeurs(&lire(&d, "26.3"));
        assert!(lues.contains(&("jour", 0)) && lues.contains(&("coordonnees", 0)) && lues.contains(&("garder", 0)));
        let _ = fs::remove_dir_all(&d);
    }
}
