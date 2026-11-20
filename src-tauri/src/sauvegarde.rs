// Les sauvegardes d'un serveur : des archives .zip rangées dans %APPDATA%\BHOST\sauvegardes\<serveur>\.
// Chaque archive contient un petit fichier « bhost-sauvegarde.json » qui dit ce qu'elle contient et pourquoi elle existe.
// Sert aussi pour « Télécharger en .zip » d'un monde.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;

/// Le fichier de description, à la racine de chaque sauvegarde.
pub const FICHIER_META: &str = "bhost-sauvegarde.json";

/// Ce qu'on ne met pas dans l'archive : le verrou du monde (le serveur le recrée).
fn a_sauter(nom: &str) -> bool {
    nom.eq_ignore_ascii_case("session.lock")
}

/// Ce qu'on ne sauvegarde jamais dans le dossier d'un serveur : journaux, caches, fichiers de travail.
/// `libraries` est gardé pour Forge et NeoForge récents (il faut ses fichiers pour démarrer).
pub fn exclu(nom: &str, garder_bibliotheques: bool) -> bool {
    let n = nom.to_ascii_lowercase();
    matches!(n.as_str(), "logs" | "crash-reports" | "cache" | "versions" | ".fabric" | ".bhost-import" | "bhost-installation.log")
        || (n == "libraries" && !garder_bibliotheques)
}

/// Pourquoi et quoi : écrit dans chaque sauvegarde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// auto, manuelle, maj (avant un changement de version), restauration (avant), reinit (avant).
    pub genre: String,
    /// tout (le serveur entier) ou monde.
    pub quoi: String,
    /// Le monde joué au moment de la sauvegarde (level-name).
    #[serde(default)]
    pub monde: String,
    #[serde(default)]
    pub logiciel: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub jar: String,
    /// « Passage à Paper 26.3 »…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etiquette: Option<String>,
}

/// Tous les fichiers d'un dossier, avec leur chemin dans l'archive et leur poids.
fn lister_fichiers(dossier: &Path, dans_archive: &str, sortie: &mut Vec<(PathBuf, String, u64)>) {
    // Trop profond (dossier piégé) : on s'arrête là plutôt que de faire déborder la pile.
    if dans_archive.matches('/').count() > crate::garde::PROFONDEUR_MAX {
        return;
    }
    let Ok(entrees) = fs::read_dir(dossier) else { return };
    for e in entrees.flatten() {
        let nom = e.file_name().to_string_lossy().into_owned();
        if a_sauter(&nom) {
            continue;
        }
        let chemin = format!("{dans_archive}/{nom}");
        match e.file_type() {
            Ok(t) if t.is_dir() => lister_fichiers(&e.path(), &chemin, sortie),
            Ok(t) if t.is_file() => sortie.push((e.path(), chemin, e.metadata().map(|m| m.len()).unwrap_or(0))),
            _ => {} // liens : ignorés
        }
    }
}

/// Met les `elements` (dossier ou fichier sur le disque, nom dans l'archive) dans le fichier `archive`,
/// avec la description `meta` s'il y en a une. On écrit d'abord un « .part », renommé seulement si tout est bon.
pub fn zipper_avec(elements: &[(PathBuf, String)], meta: Option<&Meta>, archive: &Path, avance: &mut dyn FnMut(u8)) -> Result<(), String> {
    let mut fichiers = Vec::new();
    for (chemin, nom) in elements {
        // Les liens et jonctions ne sont pas suivis.
        match fs::symlink_metadata(chemin) {
            Ok(m) if m.is_file() => fichiers.push((chemin.clone(), nom.clone(), m.len())),
            Ok(m) if m.is_dir() => lister_fichiers(chemin, nom, &mut fichiers),
            _ => {}
        }
    }
    let total = fichiers.iter().map(|f| f.2).sum::<u64>().max(1);
    let partiel = archive.with_extension("zip.part");
    let ecrire_err = |e: std::io::Error| format!("Impossible d'écrire l'archive ({e}).");
    let resultat = (|| {
        if let Some(parent) = archive.parent() {
            fs::create_dir_all(parent).map_err(ecrire_err)?;
        }
        let mut zip = zip::ZipWriter::new(fs::File::create(&partiel).map_err(ecrire_err)?);
        let zip_err = |e: zip::result::ZipError| format!("Archive impossible ({e}).");
        if let Some(meta) = meta {
            zip.start_file(FICHIER_META, SimpleFileOptions::default()).map_err(zip_err)?;
            zip.write_all(&serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?).map_err(ecrire_err)?;
        }
        let (mut fait, mut dernier) = (0u64, u8::MAX);
        let mut tampon = vec![0u8; 256 * 1024];
        for (source, nom, taille) in &fichiers {
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(*taille >= u32::MAX as u64);
            // Un fichier peut disparaître pendant qu'on travaille (serveur allumé) : on le saute.
            let Ok(mut entree) = fs::File::open(source) else { continue };
            zip.start_file(nom.as_str(), options).map_err(zip_err)?;
            loop {
                let n = entree.read(&mut tampon).map_err(|e| format!("Lecture impossible ({e})."))?;
                if n == 0 {
                    break;
                }
                zip.write_all(&tampon[..n]).map_err(ecrire_err)?;
                fait += n as u64;
                let p = (fait * 100 / total).min(99) as u8;
                if p != dernier {
                    dernier = p;
                    avance(p);
                }
            }
        }
        zip.finish().map_err(zip_err)?;
        fs::rename(&partiel, archive).map_err(ecrire_err)?;
        avance(100);
        Ok(())
    })();
    if resultat.is_err() {
        let _ = fs::remove_file(&partiel);
    }
    resultat
}

/// Met des dossiers dans une archive, sans description (« Télécharger en .zip » d'un monde).
pub fn zipper(elements: &[(PathBuf, String)], archive: &Path, avance: &mut dyn FnMut(u8)) -> Result<(), String> {
    zipper_avec(elements, None, archive, avance)
}

/// Ce qu'il faut mettre dans une sauvegarde « tout » : le dossier du serveur, sans les exclus.
pub fn elements_du_serveur(dossier: &Path, garder_bibliotheques: bool) -> Vec<(PathBuf, String)> {
    let mut elements: Vec<(PathBuf, String)> = fs::read_dir(dossier)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.path(), e.file_name().to_string_lossy().into_owned()))
        .filter(|(_, n)| !exclu(n, garder_bibliotheques))
        .collect();
    elements.sort_by(|a, b| a.1.cmp(&b.1));
    elements
}

/// « 2026-11-17_18-27 » (heure UTC) : pour nommer une sauvegarde.
pub fn horodatage(maintenant: SystemTime) -> String {
    let secondes = maintenant.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (jours, reste) = ((secondes / 86_400) as i64, secondes % 86_400);
    // Jours depuis 1970 → date (algorithme de Howard Hinnant).
    let z = jours + 719_468;
    let ere = z.div_euclid(146_097);
    let jde = z - ere * 146_097;
    let ade = (jde - jde / 1460 + jde / 36_524 - jde / 146_096) / 365;
    let jda = jde - (365 * ade + ade / 4 - ade / 100);
    let mp = (5 * jda + 2) / 153;
    let jour = jda - (153 * mp + 2) / 5 + 1;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    let annee = ade + ere * 400 + i64::from(mois <= 2);
    format!("{annee:04}-{mois:02}-{jour:02}_{:02}-{:02}", reste / 3600, reste % 3600 / 60)
}

/// Un nom libre pour une nouvelle sauvegarde : « 2026-11-17_18-27 manuelle.zip » (puis « … manuelle-2.zip »).
pub fn nom_libre(dossier: &Path, maintenant: SystemTime, genre: &str) -> PathBuf {
    let base = format!("{} {}", horodatage(maintenant), crate::creation::nom_de_dossier(genre));
    (1..100)
        .map(|i| dossier.join(if i == 1 { format!("{base}.zip") } else { format!("{base}-{i}.zip") }))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dossier.join(format!("{base}.zip")))
}

/// Un nom de sauvegarde acceptable (un fichier .zip du dossier, sans chemin).
pub fn nom_valide(nom: &str) -> bool {
    nom.to_ascii_lowercase().ends_with(".zip") && nom.len() > 4 && nom.len() <= 120 && !nom.contains(['/', '\\', ':']) && !nom.starts_with('.')
}

/// Lit la description d'une sauvegarde. Les archives d'avant (sans description) : un monde, « avant » quelque chose.
pub fn lire_meta(archive: &Path) -> Option<Meta> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).ok()?).ok()?;
    if let Ok(entree) = zip.by_name(FICHIER_META) {
        let mut texte = String::new();
        entree.take(64 * 1024).read_to_string(&mut texte).ok()?;
        return serde_json::from_str(&texte).ok();
    }
    let nom = archive.file_name()?.to_string_lossy().to_lowercase();
    Some(Meta { genre: if nom.contains("reinitialisation") { "reinit" } else { "maj" }.into(), quoi: "monde".into(), ..Default::default() })
}

/// Une sauvegarde, pour l'onglet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sauvegarde {
    /// Le nom du fichier (c'est lui qui la désigne).
    pub fichier: String,
    /// Faite à cette heure (millisecondes).
    pub quand: u64,
    pub taille: u64,
    pub genre: String,
    pub quoi: String,
    pub etiquette: Option<String>,
    /// Épinglée : jamais effacée par le nettoyage automatique.
    pub epinglee: bool,
}

/// Les sauvegardes d'un dossier, de la plus récente à la plus ancienne.
pub fn lister(dossier: &Path, epinglees: &[String]) -> Vec<Sauvegarde> {
    let mut liste: Vec<Sauvegarde> = fs::read_dir(dossier)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let fichier = e.file_name().to_string_lossy().into_owned();
            if !nom_valide(&fichier) || !e.file_type().ok()?.is_file() {
                return None;
            }
            let meta = e.metadata().ok()?;
            let quand = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
            let m = lire_meta(&e.path()).unwrap_or_default();
            Some(Sauvegarde { epinglee: epinglees.contains(&fichier), fichier, quand, taille: meta.len(), genre: m.genre, quoi: m.quoi, etiquette: m.etiquette })
        })
        .collect();
    liste.sort_by(|a, b| b.quand.cmp(&a.quand).then(b.fichier.cmp(&a.fichier)));
    liste
}

/// Les sauvegardes automatiques en trop : on garde les `garder` plus récentes (les épinglées ne comptent pas).
pub fn en_trop(liste: &[Sauvegarde], garder: usize) -> Vec<String> {
    liste.iter().filter(|s| s.genre == "auto" && !s.epinglee).skip(garder).map(|s| s.fichier.clone()).collect()
}

/// Remet le contenu d'une sauvegarde dans `dossier`. `seulement` : les dossiers du haut à reprendre (le monde),
/// sinon tout. Les fichiers déjà là sont remplacés. Les chemins qui voudraient sortir du dossier sont refusés.
pub fn restaurer(archive: &Path, dossier: &Path, seulement: Option<&[String]>, avance: &mut dyn FnMut(u8)) -> Result<(), String> {
    let illisible = |e: zip::result::ZipError| format!("Sauvegarde abîmée ({e}).");
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| format!("Sauvegarde introuvable ({e})."))?).map_err(illisible)?;
    crate::garde::verifier_archive(&zip)?;
    let mut budget = crate::garde::budget_extraction(dossier);
    let total = zip.len().max(1);
    for i in 0..zip.len() {
        let mut entree = zip.by_index(i).map_err(illisible)?;
        let Some(relatif) = entree.enclosed_name() else { return Err("Sauvegarde suspecte : un chemin sort du dossier.".into()) };
        crate::garde::verifier_chemin(&relatif)?;
        let premier = relatif.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        if premier == FICHIER_META || seulement.is_some_and(|s| !s.contains(&premier)) {
            continue;
        }
        // « Monde seulement » : seulement ce qui est dans les dossiers des mondes, jamais un fichier du haut.
        if seulement.is_some() && relatif.components().count() < 2 {
            continue;
        }
        let chemin = dossier.join(&relatif);
        // Jamais d'écriture à travers un lien ou une jonction qui sortirait du serveur.
        crate::garde::dans(dossier, &chemin)?;
        if entree.is_dir() {
            fs::create_dir_all(&chemin).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = chemin.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            crate::garde::pas_un_lien(&chemin)?;
            let mut sortie = fs::File::create(&chemin).map_err(|e| format!("Impossible d'écrire {} ({e}).", relatif.display()))?;
            crate::garde::copier_limite(&mut entree, &mut sortie, &mut budget)?;
        }
        avance((i * 100 / total) as u8);
    }
    avance(100);
    Ok(())
}

/// L'archive contient ce fichier (« world/level.dat »).
pub fn contient(archive: &Path, nom: &str) -> bool {
    fs::File::open(archive).ok().and_then(|f| zip::ZipArchive::new(f).ok()).is_some_and(|mut z| z.by_name(nom).is_ok())
}

/// Les dossiers et fichiers du haut d'une sauvegarde (pour savoir quels mondes elle contient).
pub fn dossiers_du_haut(archive: &Path) -> Vec<String> {
    let Some(mut zip) = fs::File::open(archive).ok().and_then(|f| zip::ZipArchive::new(f).ok()) else { return Vec::new() };
    let mut noms: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok()?.enclosed_name()?.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()))
        .filter(|n| n != FICHIER_META)
        .collect();
    noms.sort();
    noms.dedup();
    noms
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-sauvegarde-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn date_lisible() {
        assert_eq!(horodatage(UNIX_EPOCH), "1970-01-01_00-00");
        // 17 novembre 2026, 18 h 27 UTC.
        assert_eq!(horodatage(UNIX_EPOCH + Duration::from_secs(1_794_940_020)), "2026-11-17_18-27");
        assert_eq!(horodatage(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29_00-00");
    }

    #[test]
    fn met_des_dossiers_dans_un_zip() {
        let d = dossier("zip");
        let monde = d.join("world");
        fs::create_dir_all(monde.join("region")).unwrap();
        fs::write(monde.join("level.dat"), b"niveau").unwrap();
        fs::write(monde.join("session.lock"), b"verrou").unwrap();
        fs::write(monde.join("region").join("r.0.0.mca"), vec![7u8; 5000]).unwrap();
        fs::create_dir_all(d.join("world_nether")).unwrap();
        fs::write(d.join("world_nether").join("level.dat"), b"enfer").unwrap();
        let archive = d.join("sauvegardes").join("test.zip");
        let mut etapes = Vec::new();
        zipper(&[(monde.clone(), "world".into()), (d.join("world_nether"), "world_nether".into())], &archive, &mut |p| etapes.push(p)).unwrap();
        assert_eq!(etapes.last(), Some(&100));
        assert!(!d.join("sauvegardes").join("test.zip.part").exists());
        let sortie = d.join("relu");
        crate::java::extraire(&archive, &sortie).unwrap();
        assert_eq!(fs::read(sortie.join("world").join("region").join("r.0.0.mca")).unwrap().len(), 5000);
        assert_eq!(fs::read(sortie.join("world_nether").join("level.dat")).unwrap(), b"enfer");
        assert!(!sortie.join("world").join("session.lock").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn sauvegarde_et_restaure_un_serveur() {
        let d = dossier("serveur");
        let s = d.join("serveur");
        fs::create_dir_all(s.join("world")).unwrap();
        fs::create_dir_all(s.join("plugins")).unwrap();
        fs::create_dir_all(s.join("logs")).unwrap();
        fs::create_dir_all(s.join("libraries")).unwrap();
        fs::write(s.join("world").join("level.dat"), b"v1").unwrap();
        fs::write(s.join("plugins").join("a.jar"), b"plugin").unwrap();
        fs::write(s.join("server.properties"), b"motd=x\n").unwrap();
        fs::write(s.join("logs").join("latest.log"), b"journal").unwrap();
        let elements = elements_du_serveur(&s, false);
        assert_eq!(elements.iter().map(|e| e.1.as_str()).collect::<Vec<_>>(), ["plugins", "server.properties", "world"]);
        let meta = Meta { genre: "manuelle".into(), quoi: "tout".into(), monde: "world".into(), logiciel: "Paper".into(), version: "1.21.4".into(), jar: "server.jar".into(), etiquette: None };
        let rangement = d.join("sauvegardes");
        let archive = nom_libre(&rangement, UNIX_EPOCH, "manuelle");
        assert!(archive.ends_with("1970-01-01_00-00 manuelle.zip"));
        zipper_avec(&elements, Some(&meta), &archive, &mut |_| {}).unwrap();
        assert!(nom_libre(&rangement, UNIX_EPOCH, "manuelle").ends_with("1970-01-01_00-00 manuelle-2.zip"));
        assert_eq!(lire_meta(&archive), Some(meta));
        assert_eq!(dossiers_du_haut(&archive), ["plugins", "server.properties", "world"]);
        // Le monde change, un plugin est mis à jour : on ne restaure que le monde.
        fs::write(s.join("world").join("level.dat"), b"v2").unwrap();
        fs::write(s.join("plugins").join("a.jar"), b"plugin v2").unwrap();
        restaurer(&archive, &s, Some(&["world".to_string()]), &mut |_| {}).unwrap();
        assert_eq!(fs::read(s.join("world").join("level.dat")).unwrap(), b"v1");
        assert_eq!(fs::read(s.join("plugins").join("a.jar")).unwrap(), b"plugin v2");
        assert!(!s.join(FICHIER_META).exists());
        // Tout.
        restaurer(&archive, &s, None, &mut |_| {}).unwrap();
        assert_eq!(fs::read(s.join("plugins").join("a.jar")).unwrap(), b"plugin");
        // La liste.
        let liste = lister(&rangement, &["1970-01-01_00-00 manuelle.zip".to_string()]);
        assert_eq!(liste.len(), 1);
        assert_eq!((liste[0].genre.as_str(), liste[0].quoi.as_str(), liste[0].epinglee), ("manuelle", "tout", true));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn efface_les_automatiques_en_trop() {
        let s = |f: &str, genre: &str, epinglee: bool| Sauvegarde { fichier: f.into(), quand: 0, taille: 0, genre: genre.into(), quoi: "tout".into(), etiquette: None, epinglee };
        let liste = [s("5", "auto", false), s("4", "manuelle", false), s("3", "auto", true), s("2", "auto", false), s("1", "auto", false)];
        assert_eq!(en_trop(&liste, 1), ["2", "1"]);
        assert!(en_trop(&liste, 5).is_empty());
    }

    #[test]
    fn refuse_les_noms_bizarres() {
        assert!(nom_valide("2026-11-17_18-27 manuelle.zip"));
        assert!(!nom_valide("../x.zip") && !nom_valide("a\\b.zip") && !nom_valide("x.txt") && !nom_valide(".zip"));
    }
}
