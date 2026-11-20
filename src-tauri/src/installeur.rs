// Fabric, Forge et NeoForge : ces serveurs ne se téléchargent pas d'un bloc.
// On télécharge leur « installateur » officiel (vérifié avec son empreinte SHA-1), puis on le lance dans le dossier du serveur.
//   Fabric   : https://meta.fabricmc.net (versions) + installateur sur maven.fabricmc.net → fabric-server-launch.jar
//   Forge    : promotions_slim.json (versions) + installateur sur maven.minecraftforge.net → libraries/…/win_args.txt
//   NeoForge : maven-metadata.xml (versions) + installateur sur maven.neoforged.net → libraries/…/win_args.txt
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test), sans Internet.

use crate::creation::{Empreinte, Fichier, VersionMc};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const LOGICIELS: [&str; 3] = ["Fabric", "Forge", "NeoForge"];

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";

/// L'adresse de la liste des versions.
pub fn adresse_liste(logiciel: &str) -> Option<&'static str> {
    match logiciel {
        "Fabric" => Some("https://meta.fabricmc.net/v2/versions/game"),
        "Forge" => Some("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json"),
        "NeoForge" => Some("https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml"),
        _ => None,
    }
}

/// « 1.21.11 » → [1, 21, 11]. `None` si ce ne sont pas que des chiffres.
fn nombres(version: &str) -> Option<Vec<u32>> {
    version.split('.').map(|n| n.parse().ok()).collect()
}

/// Les versions numérotées, de la plus récente à la plus ancienne, sans doublon.
fn trier(versions: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<(Vec<u32>, String)> = versions.into_iter().filter_map(|id| Some((nombres(&id)?, id))).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.dedup_by(|a, b| a.0 == b.0);
    v.into_iter().map(|(_, id)| id).collect()
}

/// Les <version> d'un fichier maven-metadata.xml.
fn versions_maven(xml: &str) -> Vec<String> {
    xml.split("<version>").skip(1).filter_map(|morceau| morceau.split_once("</version>").map(|(v, _)| v.trim().to_string())).collect()
}

/// La version de Minecraft d'une version de NeoForge :
///   « 21.1.267 » → 1.21.1 · « 21.0.5 » → 1.21 · « 20.4.237 » → 1.20.4
///   « 26.1.2.127 » → 26.1.2 · « 26.3.0.110 » → 26.3 · « 21.4.111-beta » → 1.21.4 (instable)
/// `None` pour les versions de test (alpha, snapshots) qu'on ne propose pas.
pub fn minecraft_de_neoforge(neoforge: &str) -> Option<(String, bool)> {
    let (base, suffixe) = neoforge.split_once('-').unwrap_or((neoforge, ""));
    if !(suffixe.is_empty() || suffixe == "beta") {
        return None;
    }
    let n = nombres(base)?;
    let mc = match n.as_slice() {
        [annee, mineure, correctif, _] if *annee >= 26 => {
            if *correctif == 0 { format!("{annee}.{mineure}") } else { format!("{annee}.{mineure}.{correctif}") }
        }
        [majeure, mineure, _] if (20..26).contains(majeure) => {
            if *mineure == 0 { format!("1.{majeure}") } else { format!("1.{majeure}.{mineure}") }
        }
        _ => return None,
    };
    Some((mc, suffixe == "beta"))
}

/// Lit la liste des versions de Minecraft que le logiciel sait faire tourner.
pub fn lire_versions(logiciel: &str, reponse: &[u8], snapshots: bool) -> Result<Vec<VersionMc>, String> {
    let illisible = || "Liste des versions illisible.".to_string();
    let stable = |id: String, instable: bool| VersionMc { java: crate::java::requise(&id), id, instable };
    let liste: Vec<VersionMc> = match logiciel {
        "Fabric" => {
            let json: serde_json::Value = serde_json::from_slice(reponse).map_err(|_| illisible())?;
            json.as_array()
                .ok_or_else(illisible)?
                .iter()
                .filter_map(|v| Some((v.get("version")?.as_str()?.to_string(), v.get("stable")?.as_bool()?)))
                .filter(|(_, ok)| *ok || snapshots)
                .map(|(id, ok)| {
                    let java = crate::java::requise(&id).or_else(|| crate::java::requise(id.split('-').next().unwrap_or(""))).or(Some(25));
                    VersionMc { id, java, instable: !ok }
                })
                .collect()
        }
        "Forge" => {
            let json: serde_json::Value = serde_json::from_slice(reponse).map_err(|_| illisible())?;
            let promos = json.get("promos").and_then(|p| p.as_object()).ok_or_else(illisible)?;
            let mc = promos.keys().filter_map(|k| k.rsplit_once('-').map(|(mc, _)| mc.to_string()));
            trier(mc).into_iter().map(|id| stable(id, false)).collect()
        }
        "NeoForge" => {
            let xml = std::str::from_utf8(reponse).map_err(|_| illisible())?;
            let mut par_version: Vec<(String, bool)> = Vec::new(); // (Minecraft, n'a que des bêtas)
            for (mc, beta) in versions_maven(xml).iter().filter_map(|v| minecraft_de_neoforge(v)) {
                match par_version.iter_mut().find(|(m, _)| *m == mc) {
                    Some(entree) => entree.1 &= beta,
                    None => par_version.push((mc, beta)),
                }
            }
            trier(par_version.iter().map(|(m, _)| m.clone()))
                .into_iter()
                .map(|mc| {
                    let beta = par_version.iter().any(|(m, b)| *m == mc && *b);
                    stable(mc, beta)
                })
                .collect()
        }
        _ => return Err("Logiciel pas encore pris en charge.".into()),
    };
    if liste.is_empty() {
        return Err(illisible());
    }
    Ok(liste)
}

/// Ce qu'il faut pour installer un serveur.
#[derive(Debug, Clone, PartialEq)]
pub struct Installation {
    /// L'installateur à télécharger.
    pub fichier: Fichier,
    /// Ce qu'on lui donne (après « -jar installateur.jar »).
    pub arguments: Vec<String>,
    /// La version exacte du logiciel (« 61.2.4 », « 0.20.1 »…), pour retrouver ses fichiers.
    pub build: String,
}

/// L'empreinte SHA-1 publiée à côté d'un fichier sur un dépôt Maven (« fichier.jar.sha1 »).
fn sha1_maven(adresse: &str, lire: &dyn Fn(&str) -> Result<Vec<u8>, String>) -> Result<Empreinte, String> {
    let texte = lire(&format!("{adresse}.sha1"))?;
    let texte = String::from_utf8_lossy(&texte);
    let hex = texte.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if hex.len() == 40 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(Empreinte::Sha1(hex))
    } else {
        Err("Empreinte illisible.".into())
    }
}

/// Trouve l'installateur pour un logiciel et une version de Minecraft.
pub fn trouver(logiciel: &str, mc: &str, lire: &dyn Fn(&str) -> Result<Vec<u8>, String>) -> Result<Installation, String> {
    let introuvable = || format!("{logiciel} {mc} introuvable.");
    // La version ne sert que dans des adresses : on refuse tout ce qui n'est pas « chiffres, lettres, points, tirets ».
    if mc.is_empty() || !mc.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(introuvable());
    }
    let json = |octets: Vec<u8>| serde_json::from_slice::<serde_json::Value>(&octets).map_err(|_| introuvable());
    let sur = |t: &str| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    match logiciel {
        "Fabric" => {
            // Le dernier « loader » stable, et le dernier installateur stable.
            let loaders = json(lire("https://meta.fabricmc.net/v2/versions/loader")?)?;
            let loader = loaders
                .as_array()
                .and_then(|l| l.iter().find(|v| v.get("stable").and_then(|s| s.as_bool()) == Some(true)).or(l.first()))
                .and_then(|v| v.get("version")?.as_str())
                .filter(|v| sur(v))
                .ok_or_else(introuvable)?
                .to_string();
            let installateurs = json(lire("https://meta.fabricmc.net/v2/versions/installer")?)?;
            let inst = installateurs
                .as_array()
                .and_then(|l| l.iter().find(|v| v.get("stable").and_then(|s| s.as_bool()) == Some(true)).or(l.first()))
                .ok_or_else(introuvable)?;
            let adresse = inst
                .get("url")
                .and_then(|u| u.as_str())
                .filter(|u| u.starts_with("https://maven.fabricmc.net/"))
                .ok_or_else(introuvable)?
                .to_string();
            let mut arguments = vec!["server".into(), "-mcversion".into(), mc.to_string(), "-loader".into(), loader.clone(), "-downloadMinecraft".into()];
            if nombres(mc).is_none() {
                arguments.push("-snapshot".into());
            }
            Ok(Installation { fichier: Fichier { empreinte: sha1_maven(&adresse, lire)?, adresse, taille: 0, java: None }, arguments, build: loader })
        }
        "Forge" => {
            let promos = json(lire(adresse_liste("Forge").unwrap())?)?;
            let promo = |genre: &str| promos.get("promos").and_then(|p| p.get(format!("{mc}-{genre}"))).and_then(|v| v.as_str()).filter(|v| sur(v)).map(String::from);
            let forge = promo("recommended").or_else(|| promo("latest")).ok_or_else(introuvable)?;
            // Les très vieilles versions (1.7.10, 1.8.9…) ont un nom en plus à la fin : « 1.7.10-10.13.4.1614-1.7.10 ».
            for build in [format!("{mc}-{forge}"), format!("{mc}-{forge}-{mc}")] {
                let adresse = format!("{FORGE_MAVEN}/{build}/forge-{build}-installer.jar");
                if let Ok(empreinte) = sha1_maven(&adresse, lire) {
                    return Ok(Installation { fichier: Fichier { adresse, empreinte, taille: 0, java: None }, arguments: vec!["--installServer".into()], build });
                }
            }
            Err(introuvable())
        }
        "NeoForge" => {
            let xml = lire(adresse_liste("NeoForge").unwrap())?;
            let toutes = versions_maven(&String::from_utf8_lossy(&xml));
            // La plus récente pour cette version de Minecraft : stable si possible, sinon bêta.
            let candidates: Vec<(Vec<u32>, bool, String)> = toutes
                .into_iter()
                .filter(|v| sur(v))
                .filter_map(|v| {
                    let (m, beta) = minecraft_de_neoforge(&v)?;
                    if m != mc {
                        return None;
                    }
                    Some((nombres(v.split('-').next().unwrap_or(""))?, beta, v))
                })
                .collect();
            let choisie = candidates
                .iter()
                .filter(|c| !c.1)
                .max_by(|a, b| a.0.cmp(&b.0))
                .or_else(|| candidates.iter().max_by(|a, b| a.0.cmp(&b.0)))
                .map(|c| c.2.clone())
                .ok_or_else(introuvable)?;
            let adresse = format!("{NEOFORGE_MAVEN}/{choisie}/neoforge-{choisie}-installer.jar");
            Ok(Installation { fichier: Fichier { empreinte: sha1_maven(&adresse, lire)?, adresse, taille: 0, java: None }, arguments: vec!["--installServer".into()], build: choisie })
        }
        _ => Err("Logiciel pas encore pris en charge.".into()),
    }
}

/// Le fichier des arguments de Forge et NeoForge récents (ils n'ont plus de .jar à lancer).
fn fichier_arguments() -> &'static str {
    if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" }
}

/// Ce qu'il faut lancer après l'installation : un .jar, ou « @libraries/…/win_args.txt » (Forge et NeoForge récents).
pub fn lancement(dossier: &Path, logiciel: &str, build: &str) -> Option<String> {
    match logiciel {
        "Fabric" => dossier.join("fabric-server-launch.jar").is_file().then(|| "fabric-server-launch.jar".to_string()),
        "Forge" | "NeoForge" => {
            let relatif = if logiciel == "Forge" { format!("libraries/net/minecraftforge/forge/{build}") } else { format!("libraries/net/neoforged/neoforge/{build}") };
            let arguments = format!("{relatif}/{}", fichier_arguments());
            if dossier.join(&arguments).is_file() {
                return Some(format!("@{arguments}"));
            }
            // Vieux Forge (1.16.5 et avant) : un .jar dans le dossier.
            [format!("forge-{build}.jar"), format!("forge-{build}-universal.jar")].into_iter().find(|j| dossier.join(j).is_file())
        }
        _ => None,
    }
}

/// Lance l'installateur (`java -jar installateur.jar …`) dans le dossier du serveur et attend qu'il ait fini.
/// Ce qu'il écrit est gardé dans « bhost-installation.log ». 20 minutes au plus.
pub fn executer(java: &str, dossier: &Path, installateur: &str, arguments: &[String]) -> Result<(), String> {
    executer_suivi(java, dossier, installateur, arguments, 20, &dossier.join("bhost-installation.log"), &mut |_| {})
}

/// Lance `java -jar <outil> <arguments>` dans `dossier`, écrit tout ce qu'il dit dans `journal`,
/// et passe chaque ligne à `sur_ligne` (pour suivre l'avancement). `minutes` au plus.
pub fn executer_suivi(
    java: &str,
    dossier: &Path,
    outil: &str,
    arguments: &[String],
    minutes: u64,
    journal: &Path,
    sur_ligne: &mut dyn FnMut(&str),
) -> Result<(), String> {
    use std::io::{BufRead, Write};
    let mut ecrit = std::fs::File::create(journal).map_err(|e| format!("Impossible d'écrire le journal ({e})."))?;
    let mut commande = Command::new(java);
    commande.arg("-jar").arg(outil).args(arguments).current_dir(dossier).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        commande.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut enfant = commande.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound { "Java est introuvable sur ce PC.".to_string() } else { format!("L'installation n'a pas pu démarrer ({e}).") }
    })?;
    // Deux lecteurs (sortie et erreurs) envoient leurs lignes ici.
    let (envoi, reception) = std::sync::mpsc::channel::<String>();
    let lecteurs: Vec<_> = [enfant.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>), enfant.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)]
        .into_iter()
        .flatten()
        .map(|flux| {
            let envoi = envoi.clone();
            std::thread::spawn(move || {
                let mut lecteur = std::io::BufReader::new(flux);
                let mut tampon = Vec::new();
                while matches!(lecteur.read_until(b'\n', &mut tampon), Ok(n) if n > 0) {
                    let _ = envoi.send(crate::processus::nettoyer(&tampon).trim_end().to_string());
                    tampon.clear();
                }
            })
        })
        .collect();
    drop(envoi);
    let limite = Instant::now() + Duration::from_secs(minutes * 60);
    let mut traiter = |ligne: String| {
        let _ = writeln!(ecrit, "{ligne}");
        sur_ligne(&ligne);
    };
    let fin = loop {
        loop {
            match reception.recv_timeout(Duration::from_millis(200)) {
                Ok(ligne) => traiter(ligne),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                // Les deux lecteurs ont fini : on attend juste la fin du programme.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(Duration::from_millis(200));
                    break;
                }
            }
        }
        match enfant.try_wait() {
            Ok(Some(code)) => break Ok(code),
            Ok(None) if Instant::now() > limite => {
                let _ = enfant.kill();
                break Err("L'installation est trop longue. Réessaie.".to_string());
            }
            Ok(None) => {}
            Err(e) => break Err(e.to_string()),
        }
    };
    for l in lecteurs {
        let _ = l.join();
    }
    while let Ok(ligne) = reception.try_recv() {
        traiter(ligne);
    }
    match fin? {
        code if code.success() => Ok(()),
        _ => Err("L'installation a échoué. Détails : bhost-installation.log.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn faux<'a>(pages: &'a [(&'a str, &'a str)], vues: &'a RefCell<Vec<String>>) -> impl Fn(&str) -> Result<Vec<u8>, String> + 'a {
        move |adresse| {
            vues.borrow_mut().push(adresse.to_string());
            pages.iter().find(|(a, _)| *a == adresse).map(|(_, t)| t.as_bytes().to_vec()).ok_or_else(|| format!("404 {adresse}"))
        }
    }

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn traduit_les_versions_de_neoforge() {
        assert_eq!(minecraft_de_neoforge("21.1.267"), Some(("1.21.1".into(), false)));
        assert_eq!(minecraft_de_neoforge("21.0.5-beta"), Some(("1.21".into(), true)));
        assert_eq!(minecraft_de_neoforge("20.4.237"), Some(("1.20.4".into(), false)));
        assert_eq!(minecraft_de_neoforge("26.1.2.127"), Some(("26.1.2".into(), false)));
        assert_eq!(minecraft_de_neoforge("26.3.0.110"), Some(("26.3".into(), false)));
        assert_eq!(minecraft_de_neoforge("26.1.0.0-alpha.1+snapshot-1"), None);
        assert_eq!(minecraft_de_neoforge("47.1.3"), None);
    }

    #[test]
    fn lit_les_listes() {
        let fabric = br#"[{"version":"26.4-snapshot-8","stable":false},{"version":"26.3","stable":true},{"version":"1.21.11","stable":true},{"version":"1.14","stable":true}]"#;
        let v = lire_versions("Fabric", fabric, false).unwrap();
        assert_eq!(v.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["26.3", "1.21.11", "1.14"]);
        assert_eq!(v[0].java, Some(25));
        let v = lire_versions("Fabric", fabric, true).unwrap();
        assert_eq!(v[0], VersionMc { id: "26.4-snapshot-8".into(), java: Some(25), instable: true });

        let forge = br#"{"homepage":"x","promos":{"1.7.10-latest":"10.13.4.1614","1.21.11-latest":"61.2.5","1.21.11-recommended":"61.2.4","26.3-latest":"66.1.8","1.8.9-recommended":"11.15.1.2318"}}"#;
        let v = lire_versions("Forge", forge, false).unwrap();
        assert_eq!(v.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["26.3", "1.21.11", "1.8.9", "1.7.10"]);
        assert_eq!(v[3].java, Some(8));

        let neo = b"<metadata><versioning><versions><version>21.1.265</version><version>21.4.111-beta</version><version>26.3.0.110</version><version>26.1.0.0-alpha.1+snapshot-1</version><version>21.1.267</version></versions></versioning></metadata>";
        let v = lire_versions("NeoForge", neo, false).unwrap();
        assert_eq!(v.iter().map(|v| (v.id.as_str(), v.instable)).collect::<Vec<_>>(), [("26.3", false), ("1.21.4", true), ("1.21.1", false)]);
        assert!(lire_versions("NeoForge", b"rien", false).is_err());
    }

    #[test]
    fn trouve_l_installateur_fabric() {
        let pages = [
            ("https://meta.fabricmc.net/v2/versions/loader", r#"[{"version":"0.20.2-beta","stable":false},{"version":"0.20.1","stable":true}]"#),
            (
                "https://meta.fabricmc.net/v2/versions/installer",
                r#"[{"url":"https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.1.2/fabric-installer-1.1.2.jar","version":"1.1.2","stable":true}]"#,
            ),
            ("https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.1.2/fabric-installer-1.1.2.jar.sha1", SHA),
        ];
        let vues = RefCell::new(Vec::new());
        let i = trouver("Fabric", "1.21.11", &faux(&pages, &vues)).unwrap();
        assert_eq!(i.fichier.empreinte, Empreinte::Sha1(SHA.into()));
        assert_eq!(i.arguments, ["server", "-mcversion", "1.21.11", "-loader", "0.20.1", "-downloadMinecraft"]);
        assert_eq!(i.build, "0.20.1");
        let i = trouver("Fabric", "26.4-snapshot-8", &faux(&pages, &vues)).unwrap();
        assert_eq!(i.arguments.last().unwrap(), "-snapshot");
    }

    #[test]
    fn trouve_l_installateur_forge() {
        let promos = r#"{"promos":{"1.21.11-latest":"61.2.5","1.21.11-recommended":"61.2.4","1.7.10-latest":"10.13.4.1614"}}"#;
        let sha_neuf = format!("{SHA}  forge-1.21.11-61.2.4-installer.jar\n");
        let pages = [
            ("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json", promos),
            ("https://maven.minecraftforge.net/net/minecraftforge/forge/1.21.11-61.2.4/forge-1.21.11-61.2.4-installer.jar.sha1", sha_neuf.as_str()),
            ("https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-installer.jar.sha1", SHA),
        ];
        let vues = RefCell::new(Vec::new());
        let i = trouver("Forge", "1.21.11", &faux(&pages, &vues)).unwrap();
        assert_eq!(i.build, "1.21.11-61.2.4"); // la recommandée, pas la dernière
        assert_eq!(i.fichier.adresse, "https://maven.minecraftforge.net/net/minecraftforge/forge/1.21.11-61.2.4/forge-1.21.11-61.2.4-installer.jar");
        assert_eq!(i.arguments, ["--installServer"]);
        let i = trouver("Forge", "1.7.10", &faux(&pages, &vues)).unwrap();
        assert_eq!(i.build, "1.7.10-10.13.4.1614-1.7.10");
        assert!(trouver("Forge", "1.2.5", &faux(&pages, &vues)).is_err());
        assert!(trouver("Forge", "../x", &faux(&pages, &vues)).is_err());
    }

    #[test]
    fn trouve_l_installateur_neoforge() {
        let xml = "<versions><version>21.1.266</version><version>21.1.267</version><version>21.1.268-beta</version><version>21.4.110-beta</version><version>21.4.111-beta</version></versions>";
        let pages = [
            ("https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml", xml),
            ("https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.267/neoforge-21.1.267-installer.jar.sha1", SHA),
            ("https://maven.neoforged.net/releases/net/neoforged/neoforge/21.4.111-beta/neoforge-21.4.111-beta-installer.jar.sha1", SHA),
        ];
        let vues = RefCell::new(Vec::new());
        assert_eq!(trouver("NeoForge", "1.21.1", &faux(&pages, &vues)).unwrap().build, "21.1.267");
        assert_eq!(trouver("NeoForge", "1.21.4", &faux(&pages, &vues)).unwrap().build, "21.4.111-beta");
        assert!(trouver("NeoForge", "1.20.1", &faux(&pages, &vues)).is_err());
    }

    /// Un faux « java » (petit script) : il écrit des lignes, puis réussit ou échoue.
    #[cfg(unix)]
    #[test]
    fn suit_ce_que_dit_le_programme() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("bhost-suivi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let faux = d.join("java");
        std::fs::write(&faux, "#!/bin/sh\necho \"Attempting to build version\"\necho erreur >&2\necho \"Success! Everything completed\"\n[ \"$3\" = ok ]\n").unwrap();
        std::fs::set_permissions(&faux, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut lignes = Vec::new();
        let journal = d.join("journal.log");
        executer_suivi(&faux.to_string_lossy(), &d, "outil.jar", &["ok".into()], 1, &journal, &mut |l| lignes.push(l.to_string())).unwrap();
        assert!(lignes.contains(&"Attempting to build version".to_string()) && lignes.contains(&"erreur".to_string()));
        assert!(std::fs::read_to_string(&journal).unwrap().contains("Success!"));
        assert!(executer_suivi(&faux.to_string_lossy(), &d, "outil.jar", &["non".into()], 1, &journal, &mut |_| {}).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn trouve_quoi_lancer() {
        let d = std::env::temp_dir().join(format!("bhost-installeur-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let args = d.join("libraries/net/neoforged/neoforge/21.1.267");
        std::fs::create_dir_all(&args).unwrap();
        assert_eq!(lancement(&d, "NeoForge", "21.1.267"), None);
        std::fs::write(args.join(fichier_arguments()), "-cp x").unwrap();
        assert_eq!(lancement(&d, "NeoForge", "21.1.267").unwrap(), format!("@libraries/net/neoforged/neoforge/21.1.267/{}", fichier_arguments()));
        std::fs::write(d.join("forge-1.12.2-14.23.5.2860.jar"), "x").unwrap();
        assert_eq!(lancement(&d, "Forge", "1.12.2-14.23.5.2860").unwrap(), "forge-1.12.2-14.23.5.2860.jar");
        std::fs::write(d.join("fabric-server-launch.jar"), "x").unwrap();
        assert_eq!(lancement(&d, "Fabric", "0.20.1").unwrap(), "fabric-server-launch.jar");
        let _ = std::fs::remove_dir_all(&d);
    }
}
