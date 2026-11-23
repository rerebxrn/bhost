// Paramètres › Joueurs Bedrock : les joueurs sur console et téléphone rejoignent un serveur Java.
// Geyser traduit le jeu Bedrock, Floodgate les laisse entrer sans compte Java,
// ViaVersion (serveurs à plugins) accepte la version de Minecraft que Geyser imite.
//   Paper, Purpur, Spigot (1.20.5 et plus) : Geyser et Floodgate de download.geysermc.org, ViaVersion du catalogue.
//   Fabric, NeoForge : Geyser et Floodgate du catalogue (seulement pour la dernière version de Minecraft).
// Ici : ce qui est possible, les fichiers à reconnaître, le port Bedrock dans la config de Geyser.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use crate::extensions::{Fichier, Genre};
use std::path::Path;

/// Le port Bedrock habituel (en UDP ; Geyser s'en sert aussi en TCP pour la connexion NetherNet).
pub const PORT_BEDROCK: u16 = 19132;

/// Les projets du catalogue (Modrinth).
pub const GEYSER: &str = "wKkoqHrH";
pub const FLOODGATE: &str = "bWrNNfkb";
pub const VIAVERSION: &str = "P1OZGk5p";

/// Les fichiers de GeyserMC : la dernière construction d'un projet (« geyser », « floodgate »).
pub fn adresse_build(projet: &str) -> String {
    format!("https://download.geysermc.org/v2/projects/{projet}/versions/latest/builds/latest")
}

pub fn adresse_fichier(projet: &str, plateforme: &str) -> String {
    format!("{}/downloads/{plateforme}", adresse_build(projet))
}

/// Le nom et l'empreinte SHA-256 d'un fichier dans la réponse de GeyserMC.
pub fn lire_build(json: &[u8], plateforme: &str) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_slice(json).ok()?;
    let d = v.get("downloads")?.get(plateforme)?;
    let nom = d.get("name")?.as_str()?;
    let sha = d.get("sha256")?.as_str()?.to_ascii_lowercase();
    let propre = !nom.is_empty() && nom.len() < 80 && nom.to_ascii_lowercase().ends_with(".jar") && nom.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    (propre && sha.len() == 64 && sha.chars().all(|c| c.is_ascii_hexdigit())).then(|| (nom.to_string(), sha))
}

/// « 1.21.4 » → [1, 21, 4] (« 26.1 » → [26, 1]).
fn nombres(version: &str) -> Vec<u32> {
    version.split('.').map_while(|n| n.parse().ok()).collect()
}

/// Joueurs Bedrock possibles sur ce serveur ? Ok(où vont Geyser et Floodgate), sinon la raison (courte).
pub fn possible(logiciel: &str, version: &str) -> Result<Genre, &'static str> {
    match logiciel {
        "Paper" | "Purpur" | "Spigot" => {
            let n = nombres(version);
            let recent = match n.as_slice() {
                [1, mineure, reste @ ..] => *mineure > 20 || (*mineure == 20 && reste.first().copied().unwrap_or(0) >= 5),
                [annee, ..] => *annee >= 26,
                [] => false,
            };
            if recent { Ok(Genre::Plugins) } else { Err("Il faut Minecraft 1.20.5 ou plus récent.") }
        }
        "Fabric" | "NeoForge" => Ok(Genre::Mods),
        "Forge" => Err("Pas possible avec Forge."),
        "Vanilla" => Err("Pas possible avec Vanilla."),
        "Mohist" | "Youer" | "Arclight" => Err("Pas possible avec ce logiciel."),
        _ => Err("Pas possible avec ce logiciel."),
    }
}

/// La plateforme de GeyserMC pour ce logiciel.
pub fn plateforme(logiciel: &str) -> &'static str {
    match logiciel {
        "Fabric" => "fabric",
        "NeoForge" => "neoforge",
        _ => "spigot",
    }
}

/// Où Geyser range sa config, depuis le dossier du serveur.
pub fn config_geyser(serveur: &Path, logiciel: &str) -> std::path::PathBuf {
    match logiciel {
        "Fabric" => serveur.join("config").join("Geyser-Fabric").join("config.yml"),
        "NeoForge" => serveur.join("config").join("Geyser-NeoForge").join("config.yml"),
        _ => serveur.join("plugins").join("Geyser-Spigot").join("config.yml"),
    }
}

/// Les fichiers de Geyser, Floodgate et ViaVersion parmi ceux du dossier (actifs ou coupés).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Pieces {
    pub geyser: Option<Fichier>,
    pub floodgate: Option<Fichier>,
    pub via: Option<Fichier>,
}

impl Pieces {
    pub fn lire(fichiers: &[Fichier]) -> Pieces {
        // Un actif plutôt qu'un coupé, si les deux existent.
        let trouver = |debut: &str| {
            let mut l: Vec<&Fichier> = fichiers.iter().filter(|f| !f.dossier && f.nom.to_ascii_lowercase().starts_with(debut)).collect();
            l.sort_by_key(|f| !f.actif);
            l.first().map(|f| (*f).clone())
        };
        Pieces { geyser: trouver("geyser"), floodgate: trouver("floodgate"), via: trouver("viaversion") }
    }

    /// Geyser et Floodgate sont là et actifs : les joueurs Bedrock peuvent venir.
    pub fn actif(&self) -> bool {
        self.geyser.as_ref().is_some_and(|f| f.actif) && self.floodgate.as_ref().is_some_and(|f| f.actif)
    }
}

/// Un port Bedrock pas encore pris par un autre serveur de BHOST.
pub fn port_libre(pris: &[u16]) -> u16 {
    (PORT_BEDROCK..PORT_BEDROCK + 200).find(|p| !pris.contains(p)).unwrap_or(PORT_BEDROCK)
}

/// Change le port Bedrock dans la config de Geyser (« bedrock: » puis « port: » juste en dessous).
/// Renvoie le nouveau texte, ou None s'il n'y a rien à changer (ou rien trouvé).
pub fn regler_port(texte: &str, port: u16) -> Option<String> {
    let lignes: Vec<&str> = texte.lines().collect();
    let debut = lignes.iter().position(|l| l.trim_end() == "bedrock:")?;
    let mut retrait_enfants: Option<usize> = None;
    for (i, ligne) in lignes.iter().enumerate().skip(debut + 1) {
        let contenu = ligne.trim_start();
        if contenu.is_empty() || contenu.starts_with('#') {
            continue;
        }
        let retrait = ligne.len() - contenu.len();
        if retrait == 0 {
            return None; // fin de la section « bedrock: » sans « port: »
        }
        let enfants = *retrait_enfants.get_or_insert(retrait);
        if retrait != enfants {
            continue; // plus profond (« signaling: » …)
        }
        let Some(valeur) = contenu.strip_prefix("port:") else { continue };
        let valeur = valeur.split('#').next().unwrap_or("").trim();
        if valeur == port.to_string() {
            return None;
        }
        let mut nouvelles: Vec<String> = lignes.iter().map(|l| l.to_string()).collect();
        nouvelles[i] = format!("{}port: {port}", " ".repeat(retrait));
        let mut sortie = nouvelles.join("\n");
        if texte.ends_with('\n') {
            sortie.push('\n');
        }
        return Some(sortie);
    }
    None
}

/// Les joueurs Bedrock sont autorisés sur ce serveur (Geyser et Floodgate actifs).
pub fn actif(serveur: &Path, logiciel: &str, version: &str) -> bool {
    let Ok(genre) = possible(logiciel, version) else { return false };
    let dossier = crate::extensions::dossier(serveur, genre);
    Pieces::lire(&crate::extensions::lister(&dossier, genre)).actif()
}

/// Juste avant le démarrage : si les joueurs Bedrock sont autorisés, met le bon port dans la config de Geyser
/// (elle existe après le 1er démarrage). Renvoie le port, à donner aussi à Geyser par la variable GEYSER_BEDROCK_PORT.
pub fn preparer(serveur: &Path, logiciel: &str, version: &str, port: Option<u16>) -> Option<u16> {
    let port = port?;
    if !actif(serveur, logiciel, version) {
        return None;
    }
    let config = config_geyser(serveur, logiciel);
    if let Some(nouveau) = std::fs::read_to_string(&config).ok().and_then(|t| regler_port(&t, port)) {
        let _ = std::fs::write(&config, nouveau);
    }
    Some(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(nom: &str, actif: bool) -> Fichier {
        Fichier { nom: nom.into(), actif, dossier: false }
    }

    #[test]
    fn sait_ce_qui_est_possible() {
        assert_eq!(possible("Paper", "1.21.11"), Ok(Genre::Plugins));
        assert_eq!(possible("Spigot", "1.20.5"), Ok(Genre::Plugins));
        assert_eq!(possible("Purpur", "26.3"), Ok(Genre::Plugins));
        assert!(possible("Paper", "1.20.4").is_err());
        assert!(possible("Spigot", "1.8.8").is_err());
        assert_eq!(possible("Fabric", "1.21.1"), Ok(Genre::Mods));
        assert_eq!(possible("NeoForge", "26.3"), Ok(Genre::Mods));
        assert_eq!(possible("Forge", "1.20.1"), Err("Pas possible avec Forge."));
        assert!(possible("Vanilla", "26.3").is_err());
        assert!(possible("Mohist", "1.20.1").is_err());
        assert_eq!(plateforme("Purpur"), "spigot");
        assert_eq!(plateforme("NeoForge"), "neoforge");
        let d = Path::new("C:/s");
        assert_eq!(config_geyser(d, "Paper"), d.join("plugins").join("Geyser-Spigot").join("config.yml"));
        assert_eq!(config_geyser(d, "Fabric"), d.join("config").join("Geyser-Fabric").join("config.yml"));
    }

    #[test]
    fn lit_la_reponse_de_geysermc() {
        let json = br#"{"project_id":"geyser","version":"2.12.1","build":1304,"downloads":{
            "spigot":{"name":"Geyser-Spigot.jar","sha256":"92205FD804D566C5C1165197FC4FD586ABC04B4E35B0270315EF7A57F693F33B"},
            "fabric":{"name":"../Geyser-Fabric.jar","sha256":"dcccfeedc640cdd1f124903d2571763765fdb1c9ae8afe7b89d0e21a5c5e7bb5"},
            "neoforge":{"name":"Geyser-NeoForge.jar","sha256":"pas une empreinte"}}}"#;
        assert_eq!(lire_build(json, "spigot"), Some(("Geyser-Spigot.jar".into(), "92205fd804d566c5c1165197fc4fd586abc04b4e35b0270315ef7a57f693f33b".into())));
        assert_eq!(lire_build(json, "fabric"), None);
        assert_eq!(lire_build(json, "neoforge"), None);
        assert_eq!(lire_build(json, "velocity"), None);
        assert_eq!(adresse_fichier("floodgate", "spigot"), "https://download.geysermc.org/v2/projects/floodgate/versions/latest/builds/latest/downloads/spigot");
    }

    #[test]
    fn reconnait_les_pieces() {
        let p = Pieces::lire(&[f("Geyser-Spigot.jar", true), f("floodgate-spigot.jar.disabled", false), f("WorldEdit.jar", true)]);
        assert!(p.geyser.is_some() && p.via.is_none());
        assert!(!p.actif());
        let p = Pieces::lire(&[f("Geyser-Spigot.jar", true), f("floodgate-spigot.jar.disabled", false), f("floodgate-spigot.jar", true), f("ViaVersion-5.5.jar", true)]);
        assert!(p.actif());
        assert_eq!(p.floodgate.unwrap().nom, "floodgate-spigot.jar");
        assert_eq!(p.via.unwrap().nom, "ViaVersion-5.5.jar");
    }

    #[test]
    fn prepare_le_demarrage() {
        let d = std::env::temp_dir().join(format!("bhost-bedrock-{}", std::process::id()));
        let plugins = d.join("plugins");
        std::fs::create_dir_all(plugins.join("Geyser-Spigot")).unwrap();
        std::fs::write(plugins.join("Geyser-Spigot.jar"), b"x").unwrap();
        std::fs::write(plugins.join("floodgate-spigot.jar.disabled"), b"x").unwrap();
        std::fs::write(plugins.join("Geyser-Spigot").join("config.yml"), "bedrock:\n  port: 19132\n").unwrap();
        assert!(!actif(&d, "Paper", "1.21.11"));
        assert_eq!(preparer(&d, "Paper", "1.21.11", Some(19133)), None);
        std::fs::rename(plugins.join("floodgate-spigot.jar.disabled"), plugins.join("floodgate-spigot.jar")).unwrap();
        assert!(actif(&d, "Paper", "1.21.11"));
        assert!(!actif(&d, "Forge", "1.20.1"));
        assert_eq!(preparer(&d, "Paper", "1.21.11", None), None);
        assert_eq!(preparer(&d, "Paper", "1.21.11", Some(19133)), Some(19133));
        assert_eq!(std::fs::read_to_string(plugins.join("Geyser-Spigot").join("config.yml")).unwrap(), "bedrock:\n  port: 19133\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn choisit_un_port_libre() {
        assert_eq!(port_libre(&[]), 19132);
        assert_eq!(port_libre(&[19132, 19133]), 19134);
    }

    #[test]
    fn change_le_port_dans_la_config() {
        let config = "# Geyser\nbedrock:\n  # L’adresse\n  address: 0.0.0.0\n  signaling:\n    port: 0\n  port: 19132\n  clone-remote-port: false\njava:\n  port: 25565\n";
        let nouvelle = regler_port(config, 19133).unwrap();
        assert!(nouvelle.contains("\n  port: 19133\n"));
        assert!(nouvelle.contains("    port: 0\n") && nouvelle.contains("java:\n  port: 25565\n"));
        assert_eq!(regler_port(&nouvelle, 19133), None);
        // Ancienne config (« remote: »), port écrit avec un commentaire.
        let vieille = "bedrock:\n  address: 0.0.0.0\n  port: 19132 # le port\nremote:\n  port: 25565";
        assert_eq!(regler_port(vieille, 19140).unwrap(), "bedrock:\n  address: 0.0.0.0\n  port: 19140\nremote:\n  port: 25565");
        assert_eq!(regler_port("java:\n  port: 25565\n", 19133), None);
        assert_eq!(regler_port("bedrock:\n  address: 0.0.0.0\nremote:\n  port: 25565\n", 19133), None);
    }
}
