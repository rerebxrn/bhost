// Les modpacks de Modrinth (fichiers .mrpack) : un pack tout prêt, installé en un clic.
//
// Un .mrpack est une archive .zip avec :
// - modrinth.index.json : la version de Minecraft, le chargeur de mods (Fabric, Forge, NeoForge) et sa version,
//   et la liste des mods à télécharger (adresse, empreintes, côté client / serveur) ;
// - overrides/ (pour tous) et server-overrides/ (pour le serveur seulement) : réglages et fichiers à copier tels quels.
// Format : https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack
//
// Le serveur est un serveur Fabric / Forge / NeoForge comme les autres : BHOST installe le chargeur demandé par le pack,
// puis pose ses mods et ses réglages.

use crate::extensions::{encoder, id_valide, Projet, API};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};

/// Un pack et ses fichiers font rarement plus de quelques centaines de Mo.
pub const TAILLE_PACK: u64 = 500 * 1024 * 1024;
/// Un fichier du pack (un mod) : 200 Mo au plus.
pub const TAILLE_FICHIER: u64 = 200 * 1024 * 1024;
/// Résultats par page de recherche.
pub const PAGE: u64 = 20;
/// Les chargeurs que BHOST sait installer, avec leur nom dans un .mrpack et dans Modrinth.
const CHARGEURS: [(&str, &str, &str); 3] = [("fabric-loader", "fabric", "Fabric"), ("forge", "forge", "Forge"), ("neoforge", "neoforge", "NeoForge")];
/// Les seuls sites d'où un .mrpack peut faire télécharger un fichier (règle de Modrinth).
const SITES: [&str; 4] = ["https://cdn.modrinth.com/", "https://github.com/", "https://raw.githubusercontent.com/", "https://gitlab.com/"];

/// L'adresse de recherche des modpacks qui tournent sur un serveur.
pub fn adresse_recherche(texte: &str, decalage: u64) -> String {
    let facettes = vec![
        vec!["project_type:modpack".to_string()],
        CHARGEURS.iter().map(|(_, m, _)| format!("categories:{m}")).collect(),
        // Un pack « client seulement » ne démarre pas en serveur.
        vec!["server_side:required".into(), "server_side:optional".into()],
    ];
    let texte = texte.trim();
    let index = if texte.is_empty() { "downloads" } else { "relevance" };
    let facettes = serde_json::to_string(&facettes).unwrap_or_default();
    format!("{API}/search?query={}&facets={}&index={index}&offset={decalage}&limit={PAGE}", encoder(texte), encoder(&facettes))
}

/// L'adresse des versions d'un pack, pour les chargeurs que BHOST sait installer.
pub fn adresse_versions(projet: &str) -> String {
    let chargeurs = serde_json::to_string(&CHARGEURS.iter().map(|(_, m, _)| *m).collect::<Vec<_>>()).unwrap_or_default();
    format!("{API}/project/{projet}/version?loaders={}", encoder(&chargeurs))
}

/// Le fichier .mrpack d'une version de pack.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionPack {
    pub numero: String,
    pub url: String,
    pub sha512: String,
}

/// Choisit la version à installer : la plus récente stable, sinon la plus récente (la réponse va de la plus récente à la plus ancienne).
pub fn choisir_version(octets: &[u8]) -> Option<VersionPack> {
    let v: Value = serde_json::from_slice(octets).ok()?;
    let versions: Vec<(VersionPack, bool)> = v
        .as_array()?
        .iter()
        .filter_map(|x| {
            let stable = x.get("version_type").and_then(Value::as_str).is_none_or(|t| t == "release");
            let fichiers = x.get("files")?.as_array()?;
            let f = fichiers.iter().find(|f| f.get("filename").and_then(Value::as_str).is_some_and(|n| n.ends_with(".mrpack")))?;
            let url = f.get("url")?.as_str()?.to_string();
            let sha512 = f.get("hashes")?.get("sha512")?.as_str()?.to_ascii_lowercase();
            let numero = x.get("version_number").and_then(Value::as_str).unwrap_or("").chars().take(40).collect();
            (url.starts_with("https://cdn.modrinth.com/") && sha512.len() == 128).then_some((VersionPack { numero, url, sha512 }, stable))
        })
        .collect();
    let stable = versions.iter().position(|(_, s)| *s).unwrap_or(0);
    versions.into_iter().nth(stable).map(|(v, _)| v)
}

/// Un fichier à télécharger, demandé par le pack.
#[derive(Debug, Clone, PartialEq)]
pub struct FichierPack {
    /// Chemin dans le dossier du serveur (« mods/sodium.jar »), déjà vérifié.
    pub chemin: PathBuf,
    pub adresses: Vec<String>,
    pub sha512: String,
    pub taille: u64,
}

/// Ce que dit modrinth.index.json.
#[derive(Debug, Clone, PartialEq)]
pub struct Index {
    pub nom: String,
    /// La version de Minecraft.
    pub minecraft: String,
    /// « Fabric », « Forge » ou « NeoForge ».
    pub logiciel: String,
    /// La version du chargeur demandée par le pack.
    pub chargeur: String,
    /// Les fichiers pour le serveur (ceux « client seulement » sont laissés de côté).
    pub fichiers: Vec<FichierPack>,
}

/// Un chemin relatif sûr, sans « .. » ni lettre de lecteur : il reste dans le dossier du serveur.
pub fn chemin_sur(chemin: &str) -> Option<PathBuf> {
    if chemin.is_empty() || chemin.len() > 400 || chemin.contains(['\\', ':', '\0']) || chemin.starts_with('/') {
        return None;
    }
    let mut propre = PathBuf::new();
    for c in Path::new(chemin).components() {
        match c {
            Component::Normal(n) => {
                let n = n.to_str()?;
                if n.chars().any(|c| matches!(c, '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()) || n.ends_with(['.', ' ']) {
                    return None;
                }
                let base = n.split('.').next().unwrap_or("").to_ascii_lowercase();
                if ["con", "prn", "aux", "nul"].contains(&base.as_str()) || (base.len() == 4 && (base.starts_with("com") || base.starts_with("lpt")) && base.ends_with(|c: char| c.is_ascii_digit())) {
                    return None;
                }
                propre.push(n);
            }
            _ => return None,
        }
    }
    (propre.components().count() > 0).then_some(propre)
}

/// Lit modrinth.index.json. Refuse un pack pour un chargeur que BHOST ne sait pas installer (Quilt…).
pub fn lire_index(texte: &[u8]) -> Result<Index, String> {
    let illisible = || "Modpack illisible.".to_string();
    let v: Value = serde_json::from_slice(texte).map_err(|_| illisible())?;
    if v.get("game").and_then(Value::as_str) != Some("minecraft") || v.get("formatVersion").and_then(Value::as_u64) != Some(1) {
        return Err(illisible());
    }
    let deps = v.get("dependencies").and_then(Value::as_object).ok_or_else(illisible)?;
    let sur = |t: &str| !t.is_empty() && t.len() <= 64 && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'));
    let minecraft = deps.get("minecraft").and_then(Value::as_str).filter(|t| sur(t)).ok_or_else(illisible)?.to_string();
    let (logiciel, chargeur) = CHARGEURS
        .iter()
        .find_map(|(cle, _, nom)| deps.get(*cle).and_then(Value::as_str).map(|v| (nom.to_string(), v.to_string())))
        .ok_or_else(|| {
            if deps.contains_key("quilt-loader") { "Ce modpack demande Quilt : pas encore pris en charge.".to_string() } else { "Ce modpack n’a pas de chargeur de mods connu.".to_string() }
        })?;
    if !sur(&chargeur) {
        return Err(illisible());
    }
    let mut fichiers = Vec::new();
    for f in v.get("files").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        // « unsupported » côté serveur : un mod pour le client seulement (minimap, shaders…).
        if f.get("env").and_then(|e| e.get("server")).and_then(Value::as_str) == Some("unsupported") {
            continue;
        }
        let chemin = f.get("path").and_then(Value::as_str).and_then(chemin_sur).ok_or("Modpack refusé : un fichier sort du dossier du serveur.")?;
        let adresses: Vec<String> = f
            .get("downloads")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).filter(|u| SITES.iter().any(|s| u.starts_with(s))).map(String::from).collect())
            .unwrap_or_default();
        let sha512 = f.get("hashes").and_then(|h| h.get("sha512")).and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
        if adresses.is_empty() || sha512.len() != 128 || !sha512.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Modpack refusé : un fichier vient d’un site inconnu ou n’a pas d’empreinte.".into());
        }
        let taille = f.get("fileSize").and_then(Value::as_u64).unwrap_or(0);
        if taille > TAILLE_FICHIER {
            return Err("Modpack refusé : un fichier est trop gros.".into());
        }
        fichiers.push(FichierPack { chemin, adresses, sha512, taille });
    }
    let nom = v.get("name").and_then(Value::as_str).unwrap_or("Modpack").chars().take(60).collect();
    Ok(Index { nom, minecraft, logiciel, chargeur, fichiers })
}

/// Vrai si `octets` a bien l'empreinte SHA-512 `attendue` (en hexa).
pub fn verifier(octets: &[u8], attendue: &str) -> bool {
    let calcul: String = Sha512::digest(octets).iter().map(|b| format!("{b:02x}")).collect();
    calcul == attendue
}

/// Ouvre le .mrpack et lit son index.
pub fn ouvrir<R: Read + Seek>(pack: R) -> Result<(zip::ZipArchive<R>, Index), String> {
    let mut archive = zip::ZipArchive::new(pack).map_err(|_| "Modpack illisible.".to_string())?;
    let mut texte = Vec::new();
    archive
        .by_name("modrinth.index.json")
        .map_err(|_| "Modpack illisible.".to_string())?
        .take(16 * 1024 * 1024)
        .read_to_end(&mut texte)
        .map_err(|_| "Modpack illisible.".to_string())?;
    let index = lire_index(&texte)?;
    Ok((archive, index))
}

/// Copie overrides/ puis server-overrides/ (qui l'emporte) dans le dossier du serveur. Renvoie le nombre de fichiers.
pub fn poser_reglages<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, dossier: &Path) -> Result<usize, String> {
    let mut total: u64 = 0;
    let mut nombre = 0;
    for prefixe in ["overrides/", "server-overrides/"] {
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).map_err(|_| "Modpack illisible.".to_string())?;
            let Some(reste) = f.name().ok().and_then(|n| n.strip_prefix(prefixe).map(String::from)) else { continue };
            if f.is_dir() || reste.is_empty() {
                continue;
            }
            let chemin = chemin_sur(&reste).ok_or("Modpack refusé : un fichier sort du dossier du serveur.")?;
            let cible = dossier.join(&chemin);
            if let Some(parent) = cible.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("Écriture impossible ({e})."))?;
            }
            let mut sortie = std::fs::File::create(&cible).map_err(|e| format!("Écriture impossible ({e})."))?;
            let reste_permis = TAILLE_PACK.saturating_sub(total);
            let copie = std::io::copy(&mut (&mut f).take(reste_permis + 1), &mut sortie).map_err(|_| "Modpack illisible.".to_string())?;
            total += copie;
            if total > TAILLE_PACK {
                return Err("Modpack trop gros.".into());
            }
            nombre += 1;
        }
    }
    Ok(nombre)
}

/// Télécharge et pose les fichiers du pack (empreinte vérifiée). `telecharger(adresse)` fait le téléchargement.
pub fn poser_fichiers(index: &Index, dossier: &Path, telecharger: &dyn Fn(&str) -> Result<Vec<u8>, String>, avance: &mut dyn FnMut(u8)) -> Result<(), String> {
    let n = index.fichiers.len().max(1);
    for (i, f) in index.fichiers.iter().enumerate() {
        let mut dernier = String::from("Le téléchargement a échoué. Réessaie.");
        let mut fait = false;
        for adresse in &f.adresses {
            match telecharger(adresse) {
                Ok(octets) if verifier(&octets, &f.sha512) => {
                    let cible = dossier.join(&f.chemin);
                    if let Some(parent) = cible.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| format!("Écriture impossible ({e})."))?;
                    }
                    std::fs::write(&cible, &octets).map_err(|e| format!("Écriture impossible ({e})."))?;
                    fait = true;
                    break;
                }
                Ok(_) => dernier = "Fichier abîmé pendant le téléchargement. Réessaie.".into(),
                Err(e) => dernier = e,
            }
        }
        if !fait {
            let nom = f.chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            return Err(format!("{nom} : {dernier}"));
        }
        avance(((i + 1) * 100 / n) as u8);
    }
    Ok(())
}

/// Vrai si l'identifiant d'un projet Modrinth est sûr (il sert dans des adresses).
pub fn projet_valide(id: &str) -> bool {
    id_valide(id)
}

/// Les projets d'une page de recherche (même lecture que pour les mods et plugins).
/// Les packs marqués « client seulement » sont retirés (le filtre du catalogue en laisse passer).
pub fn lire_recherche(octets: &[u8]) -> Result<(Vec<Projet>, u64), String> {
    let mut v: Value = serde_json::from_slice(octets).map_err(|_| "Réponse illisible.".to_string())?;
    if let Some(hits) = v.get_mut("hits").and_then(Value::as_array_mut) {
        hits.retain(|h| h.get("server_side").and_then(Value::as_str) != Some("unsupported"));
    }
    let octets = serde_json::to_vec(&v).map_err(|_| "Réponse illisible.".to_string())?;
    crate::extensions::lire_recherche(&octets).map(|r| (r.projets, r.total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const H: &str = "aa";

    fn sha(o: &[u8]) -> String {
        Sha512::digest(o).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn index(fichiers: &str, deps: &str) -> String {
        format!(r#"{{"formatVersion":1,"game":"minecraft","versionId":"1.0","name":"Pack","files":[{fichiers}],"dependencies":{{{deps}}}}}"#)
    }

    #[test]
    fn retire_les_packs_client() {
        let r = r#"{"hits":[{"project_id":"aaa","title":"A","server_side":"required","downloads":3},{"project_id":"bbb","title":"B","server_side":"unsupported"}],"total_hits":2,"offset":0}"#;
        let (p, _) = lire_recherche(r.as_bytes()).unwrap();
        assert_eq!(p.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["aaa"]);
    }

    #[test]
    fn chemins() {
        assert_eq!(chemin_sur("mods/a.jar"), Some(PathBuf::from("mods").join("a.jar")));
        assert_eq!(chemin_sur("config/.hidden/x.toml"), Some(PathBuf::from("config").join(".hidden").join("x.toml")));
        for faux in ["../a.jar", "mods/../../a", "/etc/passwd", "C:/x", "mods\\a.jar", "", "mods/con.txt", "a/b./c", "./"] {
            assert_eq!(chemin_sur(faux), None, "{faux}");
        }
    }

    #[test]
    fn lit_un_index() {
        let f = format!(
            r#"{{"path":"mods/a.jar","hashes":{{"sha1":"x","sha512":"{}"}},"env":{{"client":"required","server":"required"}},"downloads":["https://cdn.modrinth.com/data/a.jar"],"fileSize":10}},
               {{"path":"mods/minimap.jar","hashes":{{"sha512":"{}"}},"env":{{"client":"required","server":"unsupported"}},"downloads":["https://cdn.modrinth.com/m.jar"],"fileSize":10}}"#,
            H.repeat(64),
            H.repeat(64)
        );
        let i = lire_index(index(&f, r#""minecraft":"1.20.1","fabric-loader":"0.16.10""#).as_bytes()).unwrap();
        assert_eq!((i.minecraft.as_str(), i.logiciel.as_str(), i.chargeur.as_str()), ("1.20.1", "Fabric", "0.16.10"));
        assert_eq!(i.fichiers.len(), 1);
        assert_eq!(i.fichiers[0].chemin, PathBuf::from("mods").join("a.jar"));
        let i = lire_index(index("", r#""minecraft":"1.21.1","neoforge":"21.1.77""#).as_bytes()).unwrap();
        assert_eq!(i.logiciel, "NeoForge");
        assert!(lire_index(index("", r#""minecraft":"1.20.1","quilt-loader":"0.26""#).as_bytes()).unwrap_err().contains("Quilt"));
    }

    #[test]
    fn refuse_les_mauvais_fichiers() {
        let deps = r#""minecraft":"1.20.1","forge":"47.3.0""#;
        let mauvais_site = format!(r#"{{"path":"mods/a.jar","hashes":{{"sha512":"{}"}},"downloads":["https://exemple.com/a.jar"]}}"#, H.repeat(64));
        assert!(lire_index(index(&mauvais_site, deps).as_bytes()).is_err());
        let sortie = format!(r#"{{"path":"../../a.jar","hashes":{{"sha512":"{}"}},"downloads":["https://cdn.modrinth.com/a.jar"]}}"#, H.repeat(64));
        assert!(lire_index(index(&sortie, deps).as_bytes()).is_err());
        let sans_empreinte = r#"{"path":"mods/a.jar","hashes":{},"downloads":["https://cdn.modrinth.com/a.jar"]}"#;
        assert!(lire_index(index(sans_empreinte, deps).as_bytes()).is_err());
    }

    #[test]
    fn choisit_la_derniere_stable() {
        let v = |n: &str, t: &str| format!(r#"{{"version_number":"{n}","version_type":"{t}","files":[{{"filename":"p.mrpack","url":"https://cdn.modrinth.com/p.mrpack","hashes":{{"sha512":"{}"}}}}]}}"#, H.repeat(64));
        let reponse = format!("[{},{},{}]", v("3.0-beta", "beta"), v("2.0", "release"), v("1.0", "release"));
        assert_eq!(choisir_version(reponse.as_bytes()).unwrap().numero, "2.0");
        let reponse = format!("[{}]", v("3.0-beta", "beta"));
        assert_eq!(choisir_version(reponse.as_bytes()).unwrap().numero, "3.0-beta");
        assert_eq!(choisir_version(b"[]"), None);
    }

    #[test]
    fn installe_un_pack() {
        let dossier = std::env::temp_dir().join(format!("bhost-modpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).unwrap();
        let mod_a = b"mod a".to_vec();
        let f = format!(r#"{{"path":"mods/a.jar","hashes":{{"sha512":"{}"}},"downloads":["https://cdn.modrinth.com/a.jar"]}}"#, sha(&mod_a));
        let texte = index(&f, r#""minecraft":"1.20.1","fabric-loader":"0.16.10""#);
        // L'archive .mrpack.
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        zip.start_file("modrinth.index.json", o).unwrap();
        zip.write_all(texte.as_bytes()).unwrap();
        zip.start_file("overrides/config/x.toml", o).unwrap();
        zip.write_all(b"client").unwrap();
        zip.start_file("server-overrides/config/x.toml", o).unwrap();
        zip.write_all(b"serveur").unwrap();
        let octets = zip.finish().unwrap().into_inner();
        let (mut archive, index) = ouvrir(std::io::Cursor::new(octets)).unwrap();
        assert_eq!(poser_reglages(&mut archive, &dossier).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(dossier.join("config").join("x.toml")).unwrap(), "serveur");
        poser_fichiers(&index, &dossier, &|_| Ok(mod_a.clone()), &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(dossier.join("mods").join("a.jar")).unwrap(), mod_a);
        assert!(poser_fichiers(&index, &dossier, &|_| Ok(b"autre".to_vec()), &mut |_| {}).unwrap_err().contains("abîmé"));
        let _ = std::fs::remove_dir_all(&dossier);
    }
}

