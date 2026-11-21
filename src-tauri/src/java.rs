// Java automatique : chaque version de Minecraft veut sa version de Java.
// BHOST choisit la bonne, et la télécharge chez Adoptium (Eclipse Temurin, gratuit et libre) si elle manque.
// Elle est rangée dans %APPDATA%\BHOST\java\<version>\ : rien n'est installé dans Windows.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test), sans Internet.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Taille maximale d'un Java téléchargé : 400 Mo (un JRE fait environ 50 Mo, un JDK jusqu'à 210 Mo).
const TAILLE_MAX: u64 = 400 * 1024 * 1024;

/// La version de Java qu'il faut pour une version de Minecraft. `None` si la version est inconnue.
///   1.16.5 et avant : 8 · 1.17 à 1.20.4 : 17 · 1.20.5 à 1.21.x : 21 · 26.1 et après : 25
pub fn requise(minecraft: &str) -> Option<u32> {
    let nombres: Vec<u32> = minecraft.trim().split('.').map(|n| n.parse().ok()).collect::<Option<_>>()?;
    match nombres.as_slice() {
        [annee, ..] if *annee >= 26 => Some(25),
        [1, mineure, reste @ ..] => {
            let correctif = reste.first().copied().unwrap_or(0);
            Some(match mineure {
                0..=16 => 8,
                17..=19 => 17,
                20 if correctif <= 4 => 17,
                _ => 21,
            })
        }
        _ => None,
    }
}

/// Le dossier d'un Java installé par BHOST : « 21 » pour un JRE (faire tourner les serveurs),
/// « jdk-21 » pour un JDK (compiler : BuildTools en a besoin pour fabriquer Spigot).
fn dossier_version(racine: &Path, version: u32, jdk: bool) -> PathBuf {
    racine.join(if jdk { format!("jdk-{version}") } else { version.to_string() })
}

/// Le programme java.exe d'une version, s'il est déjà installé.
pub fn installe(racine: &Path, version: u32, jdk: bool) -> Option<PathBuf> {
    let bin = dossier_version(racine, version, jdk).join("bin");
    let programme = bin.join(if cfg!(windows) { "java.exe" } else { "java" });
    // Un JDK sans javac (archive abîmée…) ne sert à rien : on le refait.
    let complet = !jdk || bin.join(if cfg!(windows) { "javac.exe" } else { "javac" }).is_file();
    (programme.is_file() && complet).then_some(programme)
}

/// Le système pour lequel on demande Java : Windows (le logiciel), Linux (BHOST OS).
const SYSTEME: &str = if cfg!(windows) { "windows" } else { "linux" };

/// L'adresse où Adoptium décrit le dernier Java (JRE ou JDK, 64 bits) d'une version.
pub fn adresse_api(version: u32, jdk: bool) -> String {
    let sorte = if jdk { "jdk" } else { "jre" };
    format!("https://api.adoptium.net/v3/assets/latest/{version}/hotspot?architecture=x64&image_type={sorte}&os={SYSTEME}&vendor=eclipse")
}

/// Le fichier à télécharger, tel que décrit par Adoptium.
#[derive(Debug, PartialEq)]
pub struct Paquet {
    pub adresse: String,
    /// Empreinte SHA-256 attendue, en minuscules.
    pub empreinte: String,
    pub taille: u64,
}

/// Lit la réponse d'Adoptium : on prend l'archive .zip (Windows) ou .tar.gz (Linux), pas l'installeur .msi.
pub fn lire_paquet(reponse: &[u8]) -> Option<Paquet> {
    let json: serde_json::Value = serde_json::from_slice(reponse).ok()?;
    let paquet = json.as_array()?.first()?.get("binary")?.get("package")?;
    let adresse = paquet.get("link")?.as_str()?;
    let empreinte = paquet.get("checksum")?.as_str()?.to_ascii_lowercase();
    let taille = paquet.get("size")?.as_u64()?;
    let bon = adresse.starts_with("https://") && (adresse.ends_with(".zip") || adresse.ends_with(".tar.gz")) && empreinte.len() == 64 && empreinte.chars().all(|c| c.is_ascii_hexdigit());
    (bon && taille <= TAILLE_MAX).then(|| Paquet { adresse: adresse.to_string(), empreinte, taille })
}

/// Recopie un flux dans un fichier en calculant son empreinte, et prévient de l'avancement (de 0 à 100).
/// Renvoie l'empreinte SHA-256 en minuscules.
pub fn recopier(mut flux: impl Read, fichier: &Path, taille: u64, avancement: &mut dyn FnMut(u8)) -> Result<String, String> {
    let mut sortie = fs::File::create(fichier).map_err(|e| format!("Impossible d'écrire le fichier ({e})."))?;
    let mut hachage = Sha256::new();
    let mut tampon = vec![0u8; 64 * 1024];
    let (mut lu, mut dernier) = (0u64, u8::MAX);
    loop {
        let n = flux.read(&mut tampon).map_err(|e| format!("Téléchargement coupé ({e})."))?;
        if n == 0 {
            break;
        }
        lu += n as u64;
        if lu > TAILLE_MAX {
            return Err("Fichier trop gros.".into());
        }
        hachage.update(&tampon[..n]);
        sortie.write_all(&tampon[..n]).map_err(|e| format!("Impossible d'écrire le fichier ({e})."))?;
        let pourcent = if taille > 0 { (lu * 100 / taille).min(99) as u8 } else { 0 };
        if pourcent != dernier {
            dernier = pourcent;
            avancement(pourcent);
        }
    }
    sortie.flush().map_err(|e| format!("Impossible d'écrire le fichier ({e})."))?;
    Ok(hachage.finalize().iter().map(|o| format!("{o:02x}")).collect())
}

/// Ouvre l'archive dans `destination`. Les chemins qui voudraient sortir du dossier sont refusés.
pub fn extraire(archive: &Path, destination: &Path) -> Result<(), String> {
    if archive.to_string_lossy().ends_with(".tar.gz") {
        return extraire_tar(archive, destination);
    }
    let illisible = |e: zip::result::ZipError| format!("Archive abîmée ({e}).");
    let fichier = fs::File::open(archive).map_err(|e| format!("Archive introuvable ({e})."))?;
    let mut zip = zip::ZipArchive::new(fichier).map_err(illisible)?;
    crate::garde::verifier_archive(&zip)?;
    let mut budget = crate::garde::budget_extraction(destination);
    for i in 0..zip.len() {
        let mut entree = zip.by_index(i).map_err(illisible)?;
        let Some(relatif) = entree.enclosed_name() else { return Err("Archive suspecte : un chemin sort du dossier.".into()) };
        crate::garde::verifier_chemin(&relatif)?;
        let chemin = destination.join(relatif);
        if entree.is_dir() {
            fs::create_dir_all(&chemin).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = chemin.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut sortie = fs::File::create(&chemin).map_err(|e| e.to_string())?;
        crate::garde::copier_limite(&mut entree, &mut sortie, &mut budget)?;
    }
    Ok(())
}

/// La même chose pour une archive .tar.gz (le Java de Linux). Les liens sont ignorés :
/// Java n'en a pas besoin, et un lien pourrait pointer hors du dossier.
fn extraire_tar(archive: &Path, destination: &Path) -> Result<(), String> {
    let abimee = |e: io::Error| format!("Archive abîmée ({e}).");
    let fichier = fs::File::open(archive).map_err(|e| format!("Archive introuvable ({e})."))?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(fichier));
    let mut budget = crate::garde::budget_extraction(destination);
    for (n, entree) in tar.entries().map_err(abimee)?.enumerate() {
        if n >= crate::garde::ENTREES_MAX {
            return Err("Archive trop grosse : trop de fichiers.".into());
        }
        let mut entree = entree.map_err(abimee)?;
        let relatif = entree.path().map_err(abimee)?.into_owned();
        if !relatif.components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)) {
            return Err("Archive suspecte : un chemin sort du dossier.".into());
        }
        crate::garde::verifier_chemin(&relatif)?;
        let chemin = destination.join(&relatif);
        let sorte = entree.header().entry_type();
        if sorte.is_dir() {
            fs::create_dir_all(&chemin).map_err(|e| e.to_string())?;
            continue;
        }
        if !sorte.is_file() {
            continue;
        }
        if let Some(parent) = chemin.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mode = entree.header().mode().unwrap_or(0o644);
        let mut sortie = fs::File::create(&chemin).map_err(|e| e.to_string())?;
        crate::garde::copier_limite(&mut entree, &mut sortie, &mut budget)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // On garde « exécutable » (bin/java), jamais les droits spéciaux.
            fs::set_permissions(&chemin, fs::Permissions::from_mode(mode & 0o755)).map_err(|e| e.to_string())?;
        }
        #[cfg(not(unix))]
        let _ = mode;
    }
    Ok(())
}

/// Trouve le dossier qui contient bin/java(.exe) : l'archive d'Adoptium range tout dans un sous-dossier.
fn racine_java(dossier: &Path) -> Option<PathBuf> {
    let programme = if cfg!(windows) { "java.exe" } else { "java" };
    if dossier.join("bin").join(programme).is_file() {
        return Some(dossier.to_path_buf());
    }
    fs::read_dir(dossier).ok()?.flatten().map(|e| e.path()).find(|p| p.join("bin").join(programme).is_file())
}

/// Installe un Java déjà téléchargé (archive vérifiée) à sa place définitive.
pub fn ranger(archive: &Path, racine: &Path, version: u32, jdk: bool) -> Result<PathBuf, String> {
    let provisoire = racine.join(format!(".{}{version}-en-cours", if jdk { "jdk-" } else { "" }));
    let _ = fs::remove_dir_all(&provisoire);
    fs::create_dir_all(&provisoire).map_err(|e| e.to_string())?;
    let resultat = (|| {
        extraire(archive, &provisoire)?;
        let trouve = racine_java(&provisoire).ok_or("Java introuvable dans l'archive.")?;
        let final_ = dossier_version(racine, version, jdk);
        let _ = fs::remove_dir_all(&final_);
        fs::rename(&trouve, &final_).map_err(|e| e.to_string())?;
        installe(racine, version, jdk).ok_or_else(|| "Java introuvable après l'installation.".to_string())
    })();
    let _ = fs::remove_dir_all(&provisoire);
    resultat
}

/// Télécharge et installe un Java. `ouvrir` donne le contenu d'une adresse (la réponse d'Adoptium,
/// puis l'archive) ; `avancement` reçoit le pourcentage du téléchargement.
pub fn installer(
    racine: &Path,
    version: u32,
    jdk: bool,
    lire_texte: &dyn Fn(&str) -> Result<Vec<u8>, String>,
    ouvrir: &dyn Fn(&str) -> Result<Box<dyn Read>, String>,
    avancement: &mut dyn FnMut(u8),
) -> Result<PathBuf, String> {
    let reponse = lire_texte(&adresse_api(version, jdk))?;
    let paquet = lire_paquet(&reponse).ok_or("Adoptium n'a pas donné de Java pour ce système.")?;
    fs::create_dir_all(racine).map_err(|e| e.to_string())?;
    let extension = if paquet.adresse.ends_with(".tar.gz") { "tar.gz" } else { "zip" };
    let archive = racine.join(format!(".{}{version}.{extension}", if jdk { "jdk-" } else { "" }));
    let resultat = (|| {
        let empreinte = recopier(ouvrir(&paquet.adresse)?, &archive, paquet.taille, avancement)?;
        if empreinte != paquet.empreinte {
            return Err("Le fichier téléchargé est abîmé. Réessaie.".to_string());
        }
        avancement(100);
        ranger(&archive, racine, version, jdk)
    })();
    let _ = fs::remove_file(&archive);
    resultat
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn choisit_la_bonne_version() {
        assert_eq!(requise("1.8.8"), Some(8));
        assert_eq!(requise("1.12.2"), Some(8));
        assert_eq!(requise("1.16.5"), Some(8));
        assert_eq!(requise("1.17"), Some(17));
        assert_eq!(requise("1.17.1"), Some(17));
        assert_eq!(requise("1.18.2"), Some(17));
        assert_eq!(requise("1.20.4"), Some(17));
        assert_eq!(requise("1.20.5"), Some(21));
        assert_eq!(requise("1.21.11"), Some(21));
        assert_eq!(requise("26.1"), Some(25));
        assert_eq!(requise("26.3.1"), Some(25));
        assert_eq!(requise(""), None);
        assert_eq!(requise("24w14a"), None);
    }

    /// La vraie réponse d'Adoptium (Java 21, 17/11/2026), raccourcie.
    const REPONSE: &str = r#"[{"binary":{"architecture":"x64","image_type":"jre",
        "installer":{"checksum":"6a85618938fb3a1419455447a9250e15480da1d31fc5b399eb15212a905d50dc","link":"https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jre_x64_windows_hotspot_21.0.13_9.msi","size":35004416},
        "os":"windows","package":{"checksum":"2332c0b4cff6bed722da3567c91d5e47568bcba863fa4945d0a8d6fc76158628","link":"https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jre_x64_windows_hotspot_21.0.13_9.zip","name":"OpenJDK21U-jre_x64_windows_hotspot_21.0.13_9.zip","size":49106397}},
        "release_name":"jdk-21.0.13+9","vendor":"eclipse","version":{"major":21}}]"#;

    #[test]
    fn lit_la_reponse_d_adoptium() {
        let paquet = lire_paquet(REPONSE.as_bytes()).unwrap();
        assert_eq!(paquet.adresse, "https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.13%2B9/OpenJDK21U-jre_x64_windows_hotspot_21.0.13_9.zip");
        assert_eq!(paquet.empreinte, "2332c0b4cff6bed722da3567c91d5e47568bcba863fa4945d0a8d6fc76158628");
        assert_eq!(paquet.taille, 49106397);
        assert_eq!(lire_paquet(b"[]"), None);
        assert_eq!(lire_paquet(REPONSE.replace("https://github", "http://github").as_bytes()), None);
        assert_eq!(adresse_api(8, false), format!("https://api.adoptium.net/v3/assets/latest/8/hotspot?architecture=x64&image_type=jre&os={SYSTEME}&vendor=eclipse"));
        assert_eq!(adresse_api(21, true), format!("https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=x64&image_type=jdk&os={SYSTEME}&vendor=eclipse"));
    }

    fn dossier_test(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-java-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Une fausse archive comme celles d'Adoptium : tout dans un sous-dossier « jdk-21.0.13+9-jre ».
    fn fausse_archive(chemins: &[&str]) -> Vec<u8> {
        let mut octets = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut octets));
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for chemin in chemins {
                zip.start_file(*chemin, options).unwrap();
                zip.write_all(b"contenu").unwrap();
            }
            zip.finish().unwrap();
        }
        octets
    }

    fn empreinte(octets: &[u8]) -> String {
        Sha256::digest(octets).iter().map(|o| format!("{o:02x}")).collect()
    }

    fn programme() -> &'static str {
        if cfg!(windows) { "java.exe" } else { "java" }
    }

    #[test]
    fn telecharge_verifie_et_range() {
        let racine = dossier_test("ok");
        let archive = fausse_archive(&[&format!("jdk-21.0.13+9-jre/bin/{}", programme()), "jdk-21.0.13+9-jre/lib/modules", "jdk-21.0.13+9-jre/release"]);
        let reponse = REPONSE.replace("2332c0b4cff6bed722da3567c91d5e47568bcba863fa4945d0a8d6fc76158628", &empreinte(&archive)).replace("49106397", &archive.len().to_string());
        let mut etapes = Vec::new();
        let chemin = installer(
            &racine,
            21,
            false,
            &|adresse| {
                assert_eq!(adresse, adresse_api(21, false));
                Ok(reponse.clone().into_bytes())
            },
            &|adresse| {
                assert!(adresse.ends_with(".zip"));
                Ok(Box::new(Cursor::new(archive.clone())))
            },
            &mut |p| etapes.push(p),
        )
        .unwrap();
        assert_eq!(chemin, racine.join("21").join("bin").join(programme()));
        assert!(racine.join("21").join("lib").join("modules").is_file());
        assert_eq!(installe(&racine, 21, false), Some(chemin));
        assert_eq!(installe(&racine, 21, true), None);
        assert_eq!(etapes.last(), Some(&100));
        // Rien ne traîne : ni l'archive, ni le dossier provisoire.
        let restes: Vec<_> = fs::read_dir(&racine).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(restes, vec![std::ffi::OsString::from("21")]);
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn refuse_un_fichier_abime() {
        let racine = dossier_test("abime");
        let archive = fausse_archive(&[&format!("jdk/bin/{}", programme())]);
        let erreur = installer(
            &racine,
            21,
            false,
            &|_| Ok(REPONSE.as_bytes().to_vec()),
            &|_| Ok(Box::new(Cursor::new(archive.clone()))),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(erreur.contains("abîmé"), "{erreur}");
        assert_eq!(installe(&racine, 21, false), None);
        assert_eq!(fs::read_dir(&racine).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn refuse_une_archive_qui_sort_du_dossier() {
        let racine = dossier_test("suspecte");
        let fichier = racine.join("a.zip");
        fs::write(&fichier, fausse_archive(&["../../evade.txt"])).unwrap();
        assert!(ranger(&fichier, &racine, 8, false).is_err());
        assert!(!racine.parent().unwrap().join("evade.txt").exists());
        let _ = fs::remove_dir_all(&racine);
    }

    /// Une archive .tar.gz comme celles de Linux : bin/java exécutable, un lien (ignoré).
    fn faux_tar(mauvais: bool) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        {
            let mut tar = tar::Builder::new(&mut gz);
            let mut ajouter = |chemin: &str, mode: u32| {
                let mut h = tar::Header::new_gnu();
                h.set_size(7);
                h.set_mode(mode);
                h.set_entry_type(tar::EntryType::Regular);
                // set_path refuse « .. » : on écrit le nom à la main pour fabriquer une archive piégée.
                let nom = &mut h.as_old_mut().name;
                nom[..chemin.len()].copy_from_slice(chemin.as_bytes());
                h.set_cksum();
                tar.append(&h, &b"contenu"[..]).unwrap();
            };
            ajouter("jdk-21-jre/bin/java", 0o4755);
            ajouter("jdk-21-jre/release", 0o644);
            if mauvais {
                ajouter("../dehors.txt", 0o644);
            }
            let mut lien = tar::Header::new_gnu();
            lien.set_entry_type(tar::EntryType::Symlink);
            lien.set_size(0);
            tar.append_link(&mut lien, "jdk-21-jre/lien", "/etc/passwd").unwrap();
            tar.finish().unwrap();
        }
        gz.finish().unwrap()
    }

    #[test]
    fn ouvre_le_java_de_linux() {
        let racine = dossier_test("tar");
        let fichier = racine.join(".21.tar.gz");
        fs::write(&fichier, faux_tar(false)).unwrap();
        let java = ranger(&fichier, &racine, 21, false).unwrap();
        assert_eq!(fs::read(&java).unwrap(), b"contenu");
        assert!(!racine.join("21/lien").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&java).unwrap().permissions().mode() & 0o7777, 0o755);
        }
        let piege = racine.join(".8.tar.gz");
        fs::write(&piege, faux_tar(true)).unwrap();
        assert!(ranger(&piege, &racine, 8, false).is_err());
        assert!(!racine.parent().unwrap().join("dehors.txt").exists());
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn sans_java_dans_l_archive_c_est_une_erreur() {
        let racine = dossier_test("vide");
        let fichier = racine.join("a.zip");
        fs::write(&fichier, fausse_archive(&["jdk/LISEZMOI.txt"])).unwrap();
        assert!(ranger(&fichier, &racine, 8, false).is_err());
        assert_eq!(installe(&racine, 8, false), None);
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn range_un_jdk_a_part_et_exige_javac() {
        let racine = dossier_test("jdk");
        let javac = if cfg!(windows) { "javac.exe" } else { "javac" };
        // Un JDK sans javac n'est pas accepté.
        let incomplet = racine.join("a.zip");
        fs::write(&incomplet, fausse_archive(&[&format!("jdk-21/bin/{}", programme())])).unwrap();
        assert!(ranger(&incomplet, &racine, 21, true).is_err());
        let complet = racine.join("b.zip");
        fs::write(&complet, fausse_archive(&[&format!("jdk-21/bin/{}", programme()), &format!("jdk-21/bin/{javac}")])).unwrap();
        let chemin = ranger(&complet, &racine, 21, true).unwrap();
        assert_eq!(chemin, racine.join("jdk-21").join("bin").join(programme()));
        assert_eq!(installe(&racine, 21, true), Some(chemin));
        // Le JRE du même numéro reste à part.
        assert_eq!(installe(&racine, 21, false), None);
        let _ = fs::remove_dir_all(&racine);
    }
}
