// Les listes de joueurs d'un serveur : opérateurs (ops.json), liste blanche (whitelist.json), bannis (banned-players.json).
// Serveur allumé : BHOST passe par les commandes (op, whitelist add, ban…) et le serveur réécrit ces fichiers.
// Serveur arrêté : BHOST les modifie lui-même, au format de Minecraft.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use md5::{Digest, Md5};
use serde::Serialize;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const OPS: &str = "ops.json";
pub const BLANCHE: &str = "whitelist.json";
pub const BANNIS: &str = "banned-players.json";

/// Un joueur d'une liste.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inscrit {
    pub nom: String,
    /// Bannis : depuis quand (texte de Minecraft, « 2026-11-17 18:27:00 +0100 »).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quand: Option<String>,
    /// Bannis : le motif.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif: Option<String>,
}

/// Un pseudo acceptable : 1 à 16 lettres, chiffres ou tirets bas (un point devant pour Bedrock).
pub fn pseudo_valide(nom: &str) -> bool {
    let n = nom.strip_prefix('.').unwrap_or(nom);
    (1..=16).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn lire(dossier: &Path, fichier: &str) -> Vec<Value> {
    fs::read(dossier.join(fichier)).ok().and_then(|o| serde_json::from_slice::<Vec<Value>>(&o).ok()).unwrap_or_default()
}

fn nom_de(v: &Value) -> Option<&str> {
    v.get("name").and_then(|n| n.as_str())
}

/// Les joueurs d'une liste.
pub fn inscrits(dossier: &Path, fichier: &str) -> Vec<Inscrit> {
    lire(dossier, fichier)
        .iter()
        .filter_map(|v| {
            Some(Inscrit {
                nom: nom_de(v)?.to_string(),
                quand: v.get("created").and_then(|c| c.as_str()).map(String::from),
                motif: v.get("reason").and_then(|c| c.as_str()).filter(|r| !r.is_empty() && *r != "Banned by an operator.").map(String::from),
            })
        })
        .collect()
}

/// « 069a79f444e94726a5befca90e38aaf5 » → « 069a79f4-44e9-4726-a5be-fca90e38aaf5 ».
/// None si ce n'est pas un identifiant (32 chiffres hexadécimaux, avec ou sans tirets).
pub fn avec_tirets(uuid: &str) -> Option<String> {
    let u: String = uuid.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
    (u.len() == 32 && u.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("{}-{}-{}-{}-{}", &u[..8], &u[8..12], &u[12..16], &u[16..20], &u[20..]))
}

/// L'identifiant d'un joueur sur un serveur « hors ligne » (sans vérification des comptes) :
/// celui que fabrique Minecraft à partir de « OfflinePlayer:<pseudo> ».
pub fn uuid_hors_ligne(nom: &str) -> String {
    let mut o: [u8; 16] = Md5::digest(format!("OfflinePlayer:{nom}").as_bytes()).into();
    o[6] = (o[6] & 0x0f) | 0x30; // version 3
    o[8] = (o[8] & 0x3f) | 0x80; // variante IETF
    avec_tirets(&o.iter().map(|b| format!("{b:02x}")).collect::<String>()).unwrap_or_default()
}

/// L'identifiant d'un joueur déjà venu sur ce serveur (usercache.json), avec tirets.
pub fn uuid_connu(dossier: &Path, nom: &str) -> Option<String> {
    lire(dossier, "usercache.json")
        .iter()
        .find(|v| nom_de(v).is_some_and(|n| n.eq_ignore_ascii_case(nom)))
        .and_then(|v| v.get("uuid")?.as_str())
        .and_then(avec_tirets)
}

/// La date au format de Minecraft : « 2026-11-17 18:27:00 +0000 ».
pub fn date_java(maintenant: SystemTime) -> String {
    let horodatage = crate::sauvegarde::horodatage(maintenant); // « 2026-11-17_18-27 »
    let secondes = maintenant.duration_since(UNIX_EPOCH).map(|d| d.as_secs() % 60).unwrap_or(0);
    let (jour, heure) = horodatage.split_once('_').unwrap_or((&horodatage, "00-00"));
    format!("{jour} {}:{secondes:02} +0000", heure.replace('-', ":"))
}

/// Ce qu'on peut faire à une liste quand le serveur est arrêté.
pub enum Changement<'a> {
    Ajouter { nom: &'a str, uuid: &'a str, motif: Option<&'a str> },
    Retirer { nom: &'a str },
}

/// Modifie une liste (serveur arrêté). Ajouter un joueur déjà présent ne fait rien.
pub fn modifier(dossier: &Path, fichier: &str, changement: Changement, maintenant: SystemTime) -> Result<(), String> {
    let mut liste = lire(dossier, fichier);
    match changement {
        Changement::Retirer { nom } => liste.retain(|v| !nom_de(v).is_some_and(|n| n.eq_ignore_ascii_case(nom))),
        Changement::Ajouter { nom, uuid, motif } => {
            if liste.iter().any(|v| nom_de(v).is_some_and(|n| n.eq_ignore_ascii_case(nom))) {
                return Ok(());
            }
            let uuid = avec_tirets(uuid).ok_or("Identifiant du joueur invalide.")?;
            liste.push(match fichier {
                OPS => json!({ "uuid": uuid, "name": nom, "level": 4, "bypassesPlayerLimit": false }),
                BANNIS => json!({
                    "uuid": uuid, "name": nom, "created": date_java(maintenant), "source": "BHOST",
                    "expires": "forever", "reason": motif.filter(|m| !m.trim().is_empty()).unwrap_or("Banned by an operator.")
                }),
                _ => json!({ "uuid": uuid, "name": nom }),
            });
        }
    }
    let texte = serde_json::to_string_pretty(&liste).map_err(|e| e.to_string())?;
    crate::garde::ecrire_atomique(&dossier.join(fichier), (texte + "\n").as_bytes()).map_err(|e| format!("Impossible d’écrire {fichier}. {e}"))
}

/// Un motif de bannissement propre : une ligne, 100 lettres au plus.
pub fn motif_propre(motif: &str) -> String {
    motif.chars().filter(|c| !c.is_control()).take(100).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn dossier(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-listes-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn fabrique_les_identifiants() {
        assert_eq!(avec_tirets("069A79F444E94726A5BEFCA90E38AAF5").as_deref(), Some("069a79f4-44e9-4726-a5be-fca90e38aaf5"));
        // Pas un identifiant : refusé (et pas de plantage sur les lettres accentuées).
        assert_eq!(avec_tirets("éééééééééééééééé"), None);
        assert_eq!(avec_tirets("../../x"), None);
        // Calculé par Minecraft pour « Notch » sur un serveur hors ligne.
        assert_eq!(uuid_hors_ligne("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(date_java(UNIX_EPOCH + Duration::from_secs(1_794_940_025)), "2026-11-17 18:27:05 +0000");
    }

    #[test]
    fn verifie_les_pseudos() {
        assert!(pseudo_valide("KOTORO_YT") && pseudo_valide(".Bedrock1") && pseudo_valide("a"));
        assert!(!pseudo_valide("") && !pseudo_valide("un pseudo") && !pseudo_valide("abcdefghijklmnopq") && !pseudo_valide("x;stop"));
    }

    #[test]
    fn modifie_les_listes() {
        let d = dossier("modifier");
        let t = UNIX_EPOCH + Duration::from_secs(1_794_940_025);
        fs::write(d.join(OPS), "[]\n").unwrap();
        modifier(&d, OPS, Changement::Ajouter { nom: "Alex", uuid: "069a79f444e94726a5befca90e38aaf5", motif: None }, t).unwrap();
        modifier(&d, OPS, Changement::Ajouter { nom: "alex", uuid: "x", motif: None }, t).unwrap(); // déjà là
        let ops = lire(&d, OPS);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["uuid"], "069a79f4-44e9-4726-a5be-fca90e38aaf5");
        assert_eq!(ops[0]["level"], 4);
        modifier(&d, BANNIS, Changement::Ajouter { nom: "Tricheur", uuid: &uuid_hors_ligne("Tricheur"), motif: Some("triche") }, t).unwrap();
        let bannis = inscrits(&d, BANNIS);
        assert_eq!(bannis, [Inscrit { nom: "Tricheur".into(), quand: Some("2026-11-17 18:27:05 +0000".into()), motif: Some("triche".into()) }]);
        modifier(&d, BANNIS, Changement::Retirer { nom: "TRICHEUR" }, t).unwrap();
        assert!(inscrits(&d, BANNIS).is_empty());
        modifier(&d, BLANCHE, Changement::Ajouter { nom: "Camille", uuid: &uuid_hors_ligne("Camille"), motif: None }, t).unwrap();
        // Un identifiant qui n'en est pas un : refusé.
        assert!(modifier(&d, BLANCHE, Changement::Ajouter { nom: "Zoe", uuid: "abc", motif: None }, t).is_err());
        assert_eq!(inscrits(&d, BLANCHE)[0].nom, "Camille");
        // usercache.json : l'identifiant d'un joueur déjà venu.
        fs::write(d.join("usercache.json"), r#"[{"name":"KOTORO_YT","uuid":"11111111-2222-4333-8444-555555555555","expiresOn":"x"}]"#).unwrap();
        assert_eq!(uuid_connu(&d, "kotoro_yt").as_deref(), Some("11111111-2222-4333-8444-555555555555"));
        assert_eq!(uuid_connu(&d, "Autre"), None);
        let _ = fs::remove_dir_all(&d);
    }
}
