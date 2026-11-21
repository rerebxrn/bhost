// Le code de conduite (Minecraft 1.21.9 et plus) : montré aux joueurs à leur arrivée, à accepter pour jouer.
// Un seul texte : BHOST l'écrit dans codeofconduct/fr_fr.txt et en_us.txt (celui que Minecraft prend
// pour les autres langues), et règle enable-code-of-conduct. Texte vide : coupé.

use crate::serveurs;
use std::path::Path;

pub const LONGUEUR_MAX: usize = 4000;
const FICHIERS: [&str; 2] = ["fr_fr.txt", "en_us.txt"];

/// 1.21.9 et plus (26.1…). Les snapshots ne comptent pas.
pub fn possible(version: &str) -> bool {
    if version.contains('w') && !version.contains('-') {
        return false;
    }
    let n: Vec<u32> = version.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).take(3).filter_map(|p| p.parse().ok()).collect();
    match n.as_slice() {
        [a, ..] if *a >= 2 => true,
        [1, b, ..] if *b > 21 => true,
        [1, 21, c, ..] => *c >= 9,
        _ => false,
    }
}

/// Le texte actuel (vide : pas de code de conduite).
pub fn lire(dossier: &Path) -> String {
    let actif = serveurs::lire_proprietes(dossier).iter().any(|(k, v)| k == "enable-code-of-conduct" && v == "true");
    if !actif {
        return String::new();
    }
    let ici = dossier.join("codeofconduct");
    FICHIERS
        .iter()
        .find_map(|f| std::fs::read_to_string(ici.join(f)).ok())
        .or_else(|| std::fs::read_dir(&ici).ok()?.flatten().find_map(|e| std::fs::read_to_string(e.path()).ok()))
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Écrit le texte (vide : coupé). Compte au prochain démarrage.
pub fn ecrire(dossier: &Path, texte: &str) -> Result<String, String> {
    let texte = texte.replace("\r\n", "\n").trim().to_string();
    if texte.chars().count() > LONGUEUR_MAX {
        return Err(format!("{LONGUEUR_MAX} lettres au plus."));
    }
    let ici = dossier.join("codeofconduct");
    let erreur = |e: std::io::Error| format!("Enregistrement impossible ({e}).");
    if texte.is_empty() {
        for f in FICHIERS {
            let _ = std::fs::remove_file(ici.join(f));
        }
        let _ = std::fs::remove_dir(&ici);
    } else {
        std::fs::create_dir_all(&ici).map_err(erreur)?;
        for f in FICHIERS {
            std::fs::write(ici.join(f), format!("{texte}\n")).map_err(erreur)?;
        }
    }
    serveurs::modifier_proprietes(dossier, &[("enable-code-of-conduct", (!texte.is_empty()).to_string())])?;
    Ok(texte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(possible("1.21.9") && possible("1.21.11") && possible("26.1") && possible("26.1-snapshot-2"));
        assert!(!possible("1.21.8") && !possible("1.20.6") && !possible("25w34a") && !possible("1.8.9"));
    }

    #[test]
    fn ecrit_puis_coupe() {
        let d = std::env::temp_dir().join(format!("bhost-conduite-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("server.properties"), "motd=Salut\nenable-code-of-conduct=false\n").unwrap();
        assert_eq!(lire(&d), "");
        assert_eq!(ecrire(&d, "  Sois sympa.\r\nPas de triche.  ").unwrap(), "Sois sympa.\nPas de triche.");
        assert_eq!(lire(&d), "Sois sympa.\nPas de triche.");
        assert_eq!(std::fs::read_to_string(d.join("codeofconduct/en_us.txt")).unwrap(), "Sois sympa.\nPas de triche.\n");
        assert!(std::fs::read_to_string(d.join("server.properties")).unwrap().contains("enable-code-of-conduct=true"));
        ecrire(&d, "").unwrap();
        assert_eq!(lire(&d), "");
        assert!(!d.join("codeofconduct").exists());
        assert!(ecrire(&d, &"a".repeat(LONGUEUR_MAX + 1)).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
