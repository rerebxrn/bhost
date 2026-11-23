// Créer un serveur neuf : trouver les versions, télécharger le bon fichier .jar, préparer le dossier.
// Logiciels pris en charge ici : Vanilla (Mojang), Paper (PaperMC, API « Fill » v3), Purpur,
// et les hybrides mods + plugins : Mohist et Youer (api.mohistmc.com), Arclight (versions publiées sur GitHub).
// Les hybrides se téléchargent d'un bloc, puis installent eux-mêmes leurs bibliothèques au premier démarrage.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test), sans Internet.

use md5::Md5;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Taille maximale d'un serveur téléchargé : 300 Mo (un serveur fait 50 à 70 Mo).
const TAILLE_MAX: u64 = 300 * 1024 * 1024;

/// Les logiciels que BHOST sait installer.
pub const LOGICIELS: [&str; 6] = ["Vanilla", "Paper", "Purpur", "Mohist", "Youer", "Arclight"];

/// Les versions d'Arclight, publiées sur GitHub (une « release » par version d'Arclight).
const ARCLIGHT: &str = "https://api.github.com/repos/IzzelAliz/Arclight/releases?per_page=100";
/// Le début des adresses de téléchargement d'Arclight.
const ARCLIGHT_FICHIERS: &str = "https://github.com/IzzelAliz/Arclight/releases/download/";

/// Une version de Minecraft qu'on peut choisir.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VersionMc {
    pub id: String,
    /// Le Java qu'il lui faut (voir java.rs).
    pub java: Option<u32>,
    /// Snapshot : version de test, risquée pour un monde.
    pub instable: bool,
}

/// L'adresse de la liste des versions d'un logiciel.
pub fn adresse_liste(logiciel: &str) -> Option<&'static str> {
    match logiciel {
        "Vanilla" => Some("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"),
        "Paper" => Some("https://fill.papermc.io/v3/projects/paper"),
        "Purpur" => Some("https://api.purpurmc.org/v2/purpur"),
        "Mohist" => Some("https://api.mohistmc.com/project/mohist/versions"),
        "Youer" => Some("https://api.mohistmc.com/project/youer/versions"),
        "Arclight" => Some(ARCLIGHT),
        _ => None,
    }
}

/// Un fichier d'Arclight : « arclight-neoforge-1.21.1-1.0.1-8ec9529.jar » → (« neoforge », « 1.21.1 »).
fn lire_arclight(nom: &str) -> Option<(&'static str, &str)> {
    let reste = nom.strip_prefix("arclight-")?.strip_suffix(".jar")?;
    let (moteur, reste) = reste.split_once('-')?;
    let moteur = ["neoforge", "forge", "fabric"].into_iter().find(|m| *m == moteur)?;
    let (mc, _) = reste.split_once('-')?;
    nombres(mc).map(|_| (moteur, mc))
}

/// Pour une version de Minecraft, Arclight existe parfois pour NeoForge, Forge et Fabric : on garde celui
/// d'avant (`prefere`, quand on change de version), sinon on préfère NeoForge, puis Forge.
fn rang_moteur(moteur: &str, prefere: Option<&str>) -> u8 {
    if prefere == Some(moteur) {
        return 0;
    }
    match moteur {
        "neoforge" => 1,
        "forge" => 2,
        _ => 3,
    }
}

/// « 1.21.11 » → [1, 21, 11]. `None` pour une snapshot ou une préversion (« 1.21.9-pre1 », « 24w14a »).
fn nombres(version: &str) -> Option<Vec<u32>> {
    version.split('.').map(|n| n.parse().ok()).collect()
}

/// La plus ancienne version pour laquelle Mojang donne un serveur (avant : Alpha, Bêta et 1.0 à 1.2.4, sans serveur officiel).
const PREMIER_SERVEUR: [u32; 3] = [1, 2, 5];

/// Toutes les versions numérotées, de la plus récente à la plus ancienne.
fn trier(versions: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<(Vec<u32>, String)> = versions.into_iter().filter_map(|id| Some((nombres(&id)?, id))).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.dedup_by(|a, b| a.0 == b.0);
    v.into_iter().map(|(_, id)| id).collect()
}

/// Lit la liste des versions renvoyée par l'adresse de `adresse_liste`.
/// Vanilla : toutes les versions depuis la 1.2.5, et toutes les snapshots si on les demande.
/// Paper et Purpur : toutes les versions qu'ils proposent.
pub fn lire_versions(logiciel: &str, reponse: &[u8], snapshots: bool) -> Result<Vec<VersionMc>, String> {
    let illisible = || "Liste des versions illisible.".to_string();
    let json: serde_json::Value = serde_json::from_slice(reponse).map_err(|_| illisible())?;
    let textes = |v: &serde_json::Value| -> Vec<String> { v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() };
    let stable = |id: String| VersionMc { java: crate::java::requise(&id), id, instable: false };
    let liste: Vec<VersionMc> = match logiciel {
        "Vanilla" => {
            let entrees = json.get("versions").and_then(|v| v.as_array()).ok_or_else(illisible)?;
            let champ = |v: &serde_json::Value, nom: &str| v.get(nom).and_then(|x| x.as_str()).unwrap_or("").to_string();
            // Les snapshots plus anciennes que la 1.2.5 n'ont pas de serveur non plus.
            let debut = entrees.iter().find(|v| champ(v, "id") == "1.2.5").map(|v| champ(v, "releaseTime")).unwrap_or_default();
            // La liste de Mojang va de la plus récente à la plus ancienne : une snapshot prend le Java
            // de la version sortie juste après elle (la dernière « release » vue).
            let mut java_suivante: Option<u32> = None;
            let mut liste = Vec::new();
            for v in entrees {
                let id = champ(v, "id");
                match champ(v, "type").as_str() {
                    "release" => {
                        let Some(n) = nombres(&id) else { continue };
                        if n < PREMIER_SERVEUR.to_vec() {
                            continue;
                        }
                        java_suivante = crate::java::requise(&id);
                        liste.push(stable(id));
                    }
                    "snapshot" if snapshots && !debut.is_empty() && champ(v, "releaseTime") >= debut => {
                        // « 26.4-snapshot-8 » : on lit le numéro devant. Sinon, le Java de la version suivante.
                        let java = crate::java::requise(id.split('-').next().unwrap_or("")).or(java_suivante).or(Some(25));
                        liste.push(VersionMc { id, java, instable: true });
                    }
                    _ => {}
                }
            }
            liste
        }
        // Paper range ses versions par groupe : { "1.21": ["1.21.11", "1.21.10", …], "26.1": [...] }.
        "Paper" => trier(json.get("versions").and_then(|v| v.as_object()).ok_or_else(illisible)?.values().flat_map(textes)).into_iter().map(stable).collect(),
        "Purpur" => trier(textes(json.get("versions").ok_or_else(illisible)?)).into_iter().map(stable).collect(),
        // [{ "name": "1.20.1" }, { "name": "1.7.10" }, …]
        "Mohist" | "Youer" => {
            let noms = json.as_array().ok_or_else(illisible)?.iter().filter_map(|v| v.get("name")?.as_str().map(String::from));
            trier(noms).into_iter().map(stable).collect()
        }
        // Les « releases » de GitHub : une version est stable si une release qui n'est pas une préversion la contient.
        "Arclight" => {
            let mut vues: Vec<(String, bool)> = Vec::new(); // (Minecraft, stable)
            for release in json.as_array().ok_or_else(illisible)? {
                let stable_ici = release.get("prerelease").and_then(|p| p.as_bool()) == Some(false) && release.get("draft").and_then(|d| d.as_bool()) != Some(true);
                for asset in release.get("assets").and_then(|a| a.as_array()).into_iter().flatten() {
                    let Some((_, mc)) = asset.get("name").and_then(|n| n.as_str()).and_then(lire_arclight) else { continue };
                    match vues.iter_mut().find(|(m, _)| m == mc) {
                        Some(v) => v.1 |= stable_ici,
                        None => vues.push((mc.to_string(), stable_ici)),
                    }
                }
            }
            trier(vues.iter().filter(|(_, ok)| *ok || snapshots).map(|(m, _)| m.clone()))
                .into_iter()
                .map(|id| {
                    let instable = !vues.iter().any(|(m, ok)| *m == id && *ok);
                    VersionMc { java: crate::java::requise(&id), id, instable }
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

/// Le Java demandé par Mojang (8, 16, 17, 21…), arrondi à une version que BHOST sait installer.
fn java_installable(majeure: u64) -> u32 {
    [8, 17, 21, 25].into_iter().find(|&j| u64::from(j) >= majeure).unwrap_or(25)
}

/// Comment vérifier un fichier téléchargé.
#[derive(Debug, Clone, PartialEq)]
pub enum Empreinte {
    Sha1(String),
    Sha256(String),
    Md5(String),
    /// Pas d'empreinte publiée (vieilles versions d'Arclight sur GitHub) : seulement pour github.com, en HTTPS.
    Aucune,
}

/// Le fichier .jar à télécharger.
#[derive(Debug, Clone, PartialEq)]
pub struct Fichier {
    pub adresse: String,
    pub empreinte: Empreinte,
    /// En octets (0 si inconnue).
    pub taille: u64,
    /// Le Java demandé par Mojang pour cette version (Vanilla seulement).
    pub java: Option<u32>,
}

fn hexa(texte: &str, longueur: usize) -> Option<String> {
    let t = texte.trim().to_ascii_lowercase();
    (t.len() == longueur && t.chars().all(|c| c.is_ascii_hexdigit())).then_some(t)
}

fn https(adresse: &str) -> Option<String> {
    adresse.starts_with("https://").then(|| adresse.to_string())
}

/// Trouve le fichier du serveur pour un logiciel et une version.
/// `lire` donne le contenu d'une adresse (petites réponses JSON).
#[cfg(test)]
pub fn trouver_fichier(logiciel: &str, version: &str, lire: &dyn Fn(&str) -> Result<Vec<u8>, String>) -> Result<Fichier, String> {
    trouver_fichier_pour(logiciel, version, None, lire)
}

/// Comme `trouver_fichier`. `moteur` : pour Arclight, le moteur de mods à garder (« forge », « neoforge », « fabric »).
pub fn trouver_fichier_pour(logiciel: &str, version: &str, moteur: Option<&str>, lire: &dyn Fn(&str) -> Result<Vec<u8>, String>) -> Result<Fichier, String> {
    let introuvable = || format!("{logiciel} {version} introuvable.");
    let json = |octets: Vec<u8>| serde_json::from_slice::<serde_json::Value>(&octets).map_err(|_| introuvable());
    // La version ne sert que dans des adresses : on refuse tout ce qui n'est pas « chiffres, points, tirets ».
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(introuvable());
    }
    match logiciel {
        "Vanilla" => {
            let liste = json(lire(adresse_liste("Vanilla").unwrap())?)?;
            let entree = liste
                .get("versions")
                .and_then(|v| v.as_array())
                .and_then(|v| v.iter().find(|x| x.get("id").and_then(|i| i.as_str()) == Some(version)))
                .ok_or_else(introuvable)?;
            let fiche = entree.get("url").and_then(|u| u.as_str()).and_then(https).ok_or_else(introuvable)?;
            let details = json(lire(&fiche)?)?;
            let serveur = details.get("downloads").and_then(|d| d.get("server")).ok_or_else(|| format!("Pas de serveur pour Minecraft {version}."))?;
            Ok(Fichier {
                adresse: serveur.get("url").and_then(|u| u.as_str()).and_then(https).ok_or_else(introuvable)?,
                empreinte: Empreinte::Sha1(serveur.get("sha1").and_then(|s| s.as_str()).and_then(|s| hexa(s, 40)).ok_or_else(introuvable)?),
                taille: serveur.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                java: details.get("javaVersion").and_then(|j| j.get("majorVersion")).and_then(|m| m.as_u64()).map(java_installable),
            })
        }
        "Paper" => {
            // Toutes les constructions de cette version, de la plus récente à la plus ancienne.
            // On prend la plus récente marquée « STABLE » ; s'il n'y en a pas, la plus récente tout court.
            let builds = json(lire(&format!("https://fill.papermc.io/v3/projects/paper/versions/{version}/builds"))?)?;
            let builds = builds.as_array().ok_or_else(introuvable)?;
            let build = builds
                .iter()
                .find(|b| b.get("channel").and_then(|c| c.as_str()) == Some("STABLE"))
                .or_else(|| builds.first())
                .ok_or_else(introuvable)?;
            let jar = build.get("downloads").and_then(|d| d.get("server:default")).ok_or_else(introuvable)?;
            Ok(Fichier {
                adresse: jar.get("url").and_then(|u| u.as_str()).and_then(https).ok_or_else(introuvable)?,
                empreinte: Empreinte::Sha256(
                    jar.get("checksums").and_then(|c| c.get("sha256")).and_then(|s| s.as_str()).and_then(|s| hexa(s, 64)).ok_or_else(introuvable)?,
                ),
                taille: jar.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                java: None,
            })
        }
        "Purpur" => {
            let derniere = json(lire(&format!("https://api.purpurmc.org/v2/purpur/{version}/latest"))?)?;
            let build = derniere.get("build").and_then(|b| b.as_str()).filter(|b| !b.is_empty() && b.chars().all(|c| c.is_ascii_digit())).ok_or_else(introuvable)?;
            Ok(Fichier {
                adresse: format!("https://api.purpurmc.org/v2/purpur/{version}/{build}/download"),
                empreinte: Empreinte::Md5(derniere.get("md5").and_then(|s| s.as_str()).and_then(|s| hexa(s, 32)).ok_or_else(introuvable)?),
                taille: 0,
                java: None,
            })
        }
        "Mohist" | "Youer" => {
            let projet = logiciel.to_ascii_lowercase();
            let builds = json(lire(&format!("https://api.mohistmc.com/project/{projet}/{version}/builds"))?)?;
            // La construction la plus récente (le plus grand numéro), avec son empreinte SHA-256.
            let (id, sha) = builds
                .as_array()
                .ok_or_else(introuvable)?
                .iter()
                .filter_map(|b| Some((b.get("id")?.as_u64()?, hexa(b.get("file_sha256")?.as_str()?, 64)?)))
                .max_by_key(|(id, _)| *id)
                .ok_or_else(introuvable)?;
            Ok(Fichier { adresse: format!("https://api.mohistmc.com/project/{projet}/{version}/builds/{id}/download"), empreinte: Empreinte::Sha256(sha), taille: 0, java: None })
        }
        "Arclight" => {
            let releases = json(lire(ARCLIGHT)?)?;
            // (stable, date, rang du moteur, asset) : la plus récente stable, sinon la plus récente tout court.
            let mut meilleur: Option<(bool, String, u8, &serde_json::Value)> = None;
            for release in releases.as_array().ok_or_else(introuvable)? {
                if release.get("draft").and_then(|d| d.as_bool()) == Some(true) {
                    continue;
                }
                let stable = release.get("prerelease").and_then(|p| p.as_bool()) == Some(false);
                let date = release.get("published_at").and_then(|d| d.as_str()).unwrap_or("").to_string();
                for asset in release.get("assets").and_then(|a| a.as_array()).into_iter().flatten() {
                    let Some((m, mc)) = asset.get("name").and_then(|n| n.as_str()).and_then(lire_arclight) else { continue };
                    if mc != version {
                        continue;
                    }
                    let candidat = (stable, date.clone(), rang_moteur(m, moteur), asset);
                    let mieux = match &meilleur {
                        None => true,
                        Some((s, d, r, _)) => (stable, &date) > (*s, d) || ((stable, &date) == (*s, d) && candidat.2 < *r),
                    };
                    if mieux {
                        meilleur = Some(candidat);
                    }
                }
            }
            let (_, _, _, asset) = meilleur.ok_or_else(introuvable)?;
            let adresse = asset.get("browser_download_url").and_then(|u| u.as_str()).filter(|u| u.starts_with(ARCLIGHT_FICHIERS)).ok_or_else(introuvable)?.to_string();
            let empreinte = match asset.get("digest").and_then(|d| d.as_str()).and_then(|d| d.strip_prefix("sha256:")).and_then(|d| hexa(d, 64)) {
                Some(sha) => Empreinte::Sha256(sha),
                None => Empreinte::Aucune,
            };
            Ok(Fichier { adresse, empreinte, taille: asset.get("size").and_then(|s| s.as_u64()).unwrap_or(0), java: None })
        }
        _ => Err("Logiciel pas encore pris en charge.".into()),
    }
}

/// Le nom du fichier à lancer dans le dossier du serveur. Arclight garde son vrai nom
/// (« arclight-neoforge-1.21.1-1.0.1.jar ») : il dit pour quel moteur de mods il est fait.
pub fn nom_du_jar(logiciel: &str, fichier: &Fichier) -> String {
    let propre = |n: &&str| lire_arclight(n).is_some() && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    match fichier.adresse.rsplit('/').next().filter(propre) {
        Some(nom) if logiciel == "Arclight" => nom.to_string(),
        _ => "server.jar".into(),
    }
}

/// Recopie le téléchargement dans `destination` en vérifiant son empreinte, et prévient de l'avancement (0 à 100).
/// On écrit d'abord dans un fichier « .part », renommé seulement si tout est bon.
pub fn telecharger(flux: impl Read, fichier: &Fichier, destination: &Path, avancement: &mut dyn FnMut(u8)) -> Result<(), String> {
    let partiel = destination.with_extension("jar.part");
    let resultat = (|| {
        let ecrire_err = |e: std::io::Error| format!("Impossible d’écrire le fichier ({e}).");
        let mut sortie = fs::File::create(&partiel).map_err(ecrire_err)?;
        let (mut sha1, mut sha256, mut md5) = (Sha1::new(), Sha256::new(), Md5::new());
        let mut flux = flux;
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
            let morceau = &tampon[..n];
            match fichier.empreinte {
                Empreinte::Sha1(_) => sha1.update(morceau),
                Empreinte::Sha256(_) => sha256.update(morceau),
                Empreinte::Md5(_) => md5.update(morceau),
                Empreinte::Aucune => {}
            }
            sortie.write_all(morceau).map_err(ecrire_err)?;
            let pourcent = if fichier.taille > 0 { (lu * 100 / fichier.taille).min(99) as u8 } else { 0 };
            if pourcent != dernier {
                dernier = pourcent;
                avancement(pourcent);
            }
        }
        sortie.flush().map_err(ecrire_err)?;
        let hex = |o: &[u8]| o.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let (obtenue, attendue) = match &fichier.empreinte {
            Empreinte::Sha1(a) => (hex(&sha1.finalize()), a.clone()),
            Empreinte::Sha256(a) => (hex(&sha256.finalize()), a.clone()),
            Empreinte::Md5(a) => (hex(&md5.finalize()), a.clone()),
            // Sans empreinte : seulement un fichier d'Arclight, venu de GitHub en HTTPS.
            Empreinte::Aucune if fichier.adresse.starts_with(ARCLIGHT_FICHIERS) => (String::new(), String::new()),
            Empreinte::Aucune => return Err("Fichier sans empreinte : refusé.".into()),
        };
        if obtenue != attendue {
            return Err("Le fichier téléchargé est abîmé. Réessaie.".into());
        }
        drop(sortie);
        fs::rename(&partiel, destination).map_err(ecrire_err)?;
        avancement(100);
        Ok(())
    })();
    if resultat.is_err() {
        let _ = fs::remove_file(&partiel);
    }
    resultat
}

/// Le nom d'un dossier à partir du nom du serveur : « Survie du week-end » → « survie-du-week-end ».
pub fn nom_de_dossier(nom: &str) -> String {
    let mut sortie = String::new();
    for c in nom.trim().to_lowercase().chars() {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ÿ' => 'y',
            'œ' => 'o',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        };
        if c != '-' || !sortie.ends_with('-') {
            sortie.push(c);
        }
    }
    let sortie: String = sortie.trim_matches('-').chars().take(40).collect();
    let sortie = sortie.trim_end_matches('-').to_string();
    // Windows interdit certains noms de dossier (CON, NUL, COM1…).
    let reserve = matches!(sortie.as_str(), "con" | "prn" | "aux" | "nul")
        || ((sortie.starts_with("com") || sortie.starts_with("lpt")) && sortie.len() == 4 && sortie.ends_with(|c: char| c.is_ascii_digit()));
    if sortie.is_empty() {
        "serveur".into()
    } else if reserve {
        format!("{sortie}-serveur")
    } else {
        sortie
    }
}

/// Crée un dossier neuf pour le serveur dans `parent` (« survie », sinon « survie-2 », « survie-3 »…).
pub fn creer_dossier(parent: &Path, nom: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(parent).map_err(|e| format!("Impossible de créer le dossier des serveurs ({e})."))?;
    let base = nom_de_dossier(nom);
    for n in 1..1000 {
        let dossier = parent.join(if n == 1 { base.clone() } else { format!("{base}-{n}") });
        // create_dir échoue si le dossier existe : on ne risque pas d'écrire dans un dossier existant.
        match fs::create_dir(&dossier) {
            Ok(()) => return Ok(dossier),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Impossible de créer le dossier ({e}).")),
        }
    }
    Err("Trop de dossiers portent ce nom.".into())
}

/// Les réglages du monde choisis dans l'assistant.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Monde {
    /// survie, creatif, aventure ou hardcore.
    pub mode: String,
    /// paisible, facile, normale ou difficile.
    pub difficulte: String,
    pub joueurs_max: u32,
}

/// server.properties est lu en Latin-1 par les vieux serveurs : on écrit les autres lettres en « \uXXXX ».
pub fn echapper(texte: &str) -> String {
    texte
        .chars()
        .filter(|c| !c.is_control())
        // Au-delà de U+FFFF (emoji…), Java veut deux \uXXXX (UTF-16).
        .map(|c| if c.is_ascii() && c != '\\' { c.to_string() } else { c.encode_utf16(&mut [0; 2]).iter().map(|u| format!("\\u{u:04x}")).collect() })
        .collect()
}

/// Écrit eula.txt et server.properties dans le dossier du serveur.
/// Les vieux serveurs (avant 1.13) veulent des chiffres pour le mode de jeu et la difficulté.
pub fn ecrire_fichiers(dossier: &Path, nom: &str, version: &str, monde: &Monde, port: u16) -> Result<(), String> {
    let mut proprietes = vec!["# Fichier prepare par BHOST".to_string(), format!("motd={}", echapper(nom)), format!("server-port={port}")];
    proprietes.extend(valeurs_partie(version, monde).into_iter().map(|(k, v)| format!("{k}={v}")));
    let ecrire = |fichier: &str, texte: String| fs::write(dossier.join(fichier), texte).map_err(|e| format!("Impossible d’écrire {fichier} ({e})."));
    ecrire("server.properties", proprietes.join("\n") + "\n")?;
    // Des listes vides : sans elles, les vieilles versions (1.8…) écrivent de longues alertes au premier lancement.
    for liste in ["ops.json", "whitelist.json", "banned-players.json", "banned-ips.json"] {
        ecrire(liste, "[]\n".into())?;
    }
    // L'utilisateur a coché « J'accepte le contrat de licence de Minecraft » dans l'assistant.
    ecrire("eula.txt", "# Accepté dans BHOST (https://aka.ms/MinecraftEULA)\neula=true\n".into())
}

/// Les lignes de server.properties pour le mode, la difficulté et le nombre de joueurs.
/// Avant la 1.13, le mode et la difficulté s'écrivent en chiffres.
pub fn valeurs_partie(version: &str, monde: &Monde) -> Vec<(&'static str, String)> {
    let ancien = nombres(version).is_some_and(|n| n.len() >= 2 && n[0] == 1 && n[1] < 13);
    let hardcore = monde.mode == "hardcore";
    let (mode_nom, mode_num) = match monde.mode.as_str() {
        "creatif" => ("creative", 1),
        "aventure" => ("adventure", 2),
        _ => ("survival", 0),
    };
    let (diff_nom, diff_num) = match (hardcore, monde.difficulte.as_str()) {
        (true, _) | (_, "difficile") => ("hard", 3),
        (_, "paisible") => ("peaceful", 0),
        (_, "facile") => ("easy", 1),
        _ => ("normal", 2),
    };
    vec![
        ("gamemode", if ancien { mode_num.to_string() } else { mode_nom.to_string() }),
        ("difficulty", if ancien { diff_num.to_string() } else { diff_nom.to_string() }),
        ("hardcore", hardcore.to_string()),
        ("max-players", monde.joueurs_max.clamp(1, 100).to_string()),
    ]
}

/// Relit le mode, la difficulté et le nombre de joueurs dans server.properties (chiffres ou mots).
pub fn lire_partie(proprietes: &[(String, String)]) -> Monde {
    let valeur = |cle: &str| proprietes.iter().find(|(k, _)| k == cle).map(|(_, v)| v.to_ascii_lowercase()).unwrap_or_default();
    let mode = match valeur("gamemode").as_str() {
        _ if valeur("hardcore") == "true" => "hardcore",
        "1" | "creative" => "creatif",
        "2" | "adventure" => "aventure",
        _ => "survie",
    };
    let difficulte = match valeur("difficulty").as_str() {
        "0" | "peaceful" => "paisible",
        "1" | "easy" => "facile",
        "3" | "hard" => "difficile",
        _ => "normale",
    };
    Monde { mode: mode.into(), difficulte: difficulte.into(), joueurs_max: valeur("max-players").parse().unwrap_or(20) }
}

/// Le premier port libre à partir de 25565, en évitant ceux des autres serveurs de BHOST.
pub fn port_libre(pris: &[u16]) -> u16 {
    (25565..25665).find(|p| !pris.contains(p)).unwrap_or(25565)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::Cursor;

    #[test]
    fn trie_toutes_les_versions() {
        let v = trier(["1.21.10", "1.21.11", "26.1", "26.1.2", "1.8.8", "1.7.10", "1.21.9-pre1", "1.16.5", "1.16"].map(String::from));
        assert_eq!(v, ["26.1.2", "26.1", "1.21.11", "1.21.10", "1.16.5", "1.16", "1.8.8", "1.7.10"]);
    }

    #[test]
    fn lit_la_liste_de_mojang() {
        let reponse = br#"{"latest":{"release":"26.3","snapshot":"26.4-snapshot-8"},"versions":[
            {"id":"26.4-snapshot-8","type":"snapshot","releaseTime":"2026-11-10T10:00:00+00:00"},
            {"id":"26.3","type":"release","releaseTime":"2026-10-01T10:00:00+00:00"},
            {"id":"1.21.11","type":"release","releaseTime":"2025-12-01T10:00:00+00:00"},
            {"id":"24w14a","type":"snapshot","releaseTime":"2024-04-03T10:00:00+00:00"},
            {"id":"1.20.5","type":"release","releaseTime":"2024-04-23T10:00:00+00:00"},
            {"id":"1.8.9","type":"release","releaseTime":"2015-12-03T10:00:00+00:00"},
            {"id":"1.8.8","type":"release","releaseTime":"2015-07-28T10:00:00+00:00"},
            {"id":"12w15a","type":"snapshot","releaseTime":"2012-04-12T10:00:00+00:00"},
            {"id":"1.2.5","type":"release","releaseTime":"2012-03-29T22:00:00+00:00"},
            {"id":"12w08a","type":"snapshot","releaseTime":"2012-02-23T10:00:00+00:00"},
            {"id":"1.2.4","type":"release","releaseTime":"2012-03-22T10:00:00+00:00"},
            {"id":"b1.7.3","type":"old_beta","releaseTime":"2011-07-08T10:00:00+00:00"},
            {"id":"a1.0.16","type":"old_alpha","releaseTime":"2010-08-12T10:00:00+00:00"}]}"#;
        let ids = |l: &[VersionMc]| l.iter().map(|v| v.id.clone()).collect::<Vec<_>>();
        let sans = lire_versions("Vanilla", reponse, false).unwrap();
        assert_eq!(ids(&sans), ["26.3", "1.21.11", "1.20.5", "1.8.9", "1.8.8", "1.2.5"]);
        assert_eq!(sans[0].java, Some(25));
        assert_eq!(sans[3].java, Some(8));
        let avec = lire_versions("Vanilla", reponse, true).unwrap();
        assert_eq!(ids(&avec), ["26.4-snapshot-8", "26.3", "1.21.11", "24w14a", "1.20.5", "1.8.9", "1.8.8", "12w15a", "1.2.5"]);
        assert_eq!(avec[0], VersionMc { id: "26.4-snapshot-8".into(), java: Some(25), instable: true });
        // 24w14a vient avant la 1.20.5 : il lui faut déjà Java 21. 12w15a vient après la 1.2.5 (Java 8).
        assert_eq!(avec[3].java, Some(21));
        assert_eq!(avec[7].java, Some(8));
    }

    #[test]
    fn relit_la_partie() {
        let monde = Monde { mode: "creatif".into(), difficulte: "facile".into(), joueurs_max: 12 };
        for version in ["1.8.8", "1.21.11"] {
            let lignes: Vec<(String, String)> = valeurs_partie(version, &monde).into_iter().map(|(k, v)| (k.to_string(), v)).collect();
            assert_eq!(lire_partie(&lignes), monde);
        }
        assert_eq!(valeurs_partie("1.8.8", &monde)[0].1, "1");
        let dur = Monde { mode: "hardcore".into(), difficulte: "facile".into(), joueurs_max: 5 };
        let lignes: Vec<(String, String)> = valeurs_partie("1.21", &dur).into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        assert_eq!(lire_partie(&lignes), Monde { difficulte: "difficile".into(), ..dur });
    }

    #[test]
    fn arrondit_le_java_de_mojang() {
        assert_eq!(java_installable(8), 8);
        assert_eq!(java_installable(16), 17);
        assert_eq!(java_installable(21), 21);
        assert_eq!(java_installable(25), 25);
        assert_eq!(java_installable(30), 25);
    }

    #[test]
    fn lit_les_listes_de_paper_et_purpur() {
        let paper = br#"{"project":{"id":"paper"},"versions":{"26.2":["26.2"],"26.1":["26.1.2","26.1.1","26.1"],"1.21":["1.21.11","1.21.10"],"1.8":["1.8.8"]}}"#;
        let v = lire_versions("Paper", paper, true).unwrap();
        assert_eq!(v.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["26.2", "26.1.2", "26.1.1", "26.1", "1.21.11", "1.21.10", "1.8.8"]);
        assert!(v.iter().all(|v| !v.instable));
        let purpur = br#"{"project":"purpur","metadata":{"current":"26.3"},"versions":["1.14.1","1.21.10","1.21.11","26.1.2","26.2","26.3"]}"#;
        let v = lire_versions("Purpur", purpur, false).unwrap();
        assert_eq!(v.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["26.3", "26.2", "26.1.2", "1.21.11", "1.21.10", "1.14.1"]);
        assert!(lire_versions("Paper", b"pas du json", false).is_err());
        assert!(lire_versions("Forge", paper, false).is_err());
    }

    const RELEASES_ARCLIGHT: &str = r#"[
        {"tag_name":"Dipper/1.1.0-SNAPSHOT","prerelease":true,"draft":false,"published_at":"2026-11-05T02:16:58Z","assets":[
            {"name":"arclight-fabric-26.1.2-1.1.0-SNAPSHOT-9e3febe.jar","size":10,"digest":"sha256:957df80077e3ef9732d118e5126326c1cb6568d0fb7e4c8eb3392bdda9abfa78","browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/Dipper/1.1.0-SNAPSHOT/arclight-fabric-26.1.2-1.1.0-SNAPSHOT-9e3febe.jar"},
            {"name":"arclight-neoforge-26.1.2-1.1.0-SNAPSHOT-9e3febe.jar","size":10,"digest":"sha256:ad0e8dd04acfa58f799b1809e1010a4ca464e55164a44938a3d29f39b1bec2ee","browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/Dipper/1.1.0-SNAPSHOT/arclight-neoforge-26.1.2-1.1.0-SNAPSHOT-9e3febe.jar"}]},
        {"tag_name":"FeudalKings/1.0.1","prerelease":false,"published_at":"2025-07-16T16:29:46Z","assets":[
            {"name":"arclight-fabric-1.21.1-1.0.1-8ec9529.jar","size":20,"digest":"sha256:98088835a2671d476895d8cb282f328fec9f0c96d0dffd82236a181348dd3f82","browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/FeudalKings/1.0.1/arclight-fabric-1.21.1-1.0.1-8ec9529.jar"},
            {"name":"arclight-forge-1.21.1-1.0.1-8ec9529.jar","size":20,"digest":"sha256:9556a10ca71898e107bd3032616e2ad7ed26998016d080dd19cec5e84d7906eb","browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/FeudalKings/1.0.1/arclight-forge-1.21.1-1.0.1-8ec9529.jar"},
            {"name":"arclight-neoforge-1.21.1-1.0.1-8ec9529.jar","size":20,"digest":"sha256:a7ac57eee67c288df3c344e4f35a66d8513d6d0e1300bb58c81667836c2aebe3","browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/FeudalKings/1.0.1/arclight-neoforge-1.21.1-1.0.1-8ec9529.jar"}]},
        {"tag_name":"FeudalKings/1.0.0","prerelease":false,"published_at":"2025-02-23T14:07:52Z","assets":[
            {"name":"arclight-neoforge-1.21.1-1.0.0-3457560.jar","size":20,"digest":null,"browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/FeudalKings/1.0.0/arclight-neoforge-1.21.1-1.0.0-3457560.jar"}]},
        {"tag_name":"Trials/1.0.6","prerelease":false,"published_at":"2025-01-21T08:20:41Z","assets":[
            {"name":"arclight-forge-1.20.1-1.0.6.jar","size":30,"digest":null,"browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/Trials/1.0.6/arclight-forge-1.20.1-1.0.6.jar"},
            {"name":"notes.txt","size":1,"browser_download_url":"https://github.com/IzzelAliz/Arclight/releases/download/Trials/1.0.6/notes.txt"}]}
    ]"#;

    #[test]
    fn lit_les_listes_des_hybrides() {
        let ids = |l: &[VersionMc]| l.iter().map(|v| v.id.clone()).collect::<Vec<_>>();
        let mohist = br#"[{"name":"1.20.1"},{"name":"1.21.1"},{"name":"1.7.10"},{"name":"1.12.2"},{"name":"1.16.5"}]"#;
        let v = lire_versions("Mohist", mohist, false).unwrap();
        assert_eq!(ids(&v), ["1.21.1", "1.20.1", "1.16.5", "1.12.2", "1.7.10"]);
        assert_eq!(v[0].java, Some(21));
        assert_eq!(v[4].java, Some(8));
        assert_eq!(ids(&lire_versions("Youer", br#"[{"name":"1.21.1"},{"name":"26.2"}]"#, false).unwrap()), ["26.2", "1.21.1"]);
        // Arclight : 26.1.2 n'existe qu'en préversion (avec « snapshots » seulement).
        let sans = lire_versions("Arclight", RELEASES_ARCLIGHT.as_bytes(), false).unwrap();
        assert_eq!(ids(&sans), ["1.21.1", "1.20.1"]);
        let avec = lire_versions("Arclight", RELEASES_ARCLIGHT.as_bytes(), true).unwrap();
        assert_eq!(ids(&avec), ["26.1.2", "1.21.1", "1.20.1"]);
        assert!(avec[0].instable && !avec[1].instable);
        assert_eq!(lire_arclight("arclight-forge-1.20.1-1.0.6.jar"), Some(("forge", "1.20.1")));
        assert_eq!(lire_arclight("arclight-quilt-1.20.1-1.0.6.jar"), None);
        assert_eq!(lire_arclight("notes.txt"), None);
    }

    #[test]
    fn trouve_les_hybrides() {
        let builds = r#"[
            {"id":997,"file_sha256":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","build_date":"2026-11-15"},
            {"id":998,"file_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","build_date":"2026-11-16"},
            {"id":999,"file_sha256":"pas une empreinte"}]"#;
        let pages = [("https://api.mohistmc.com/project/youer/1.21.1/builds", builds), ("https://api.github.com/repos/IzzelAliz/Arclight/releases?per_page=100", RELEASES_ARCLIGHT)];
        let vues = RefCell::new(Vec::new());
        let f = trouver_fichier("Youer", "1.21.1", &faux(&pages, &vues)).unwrap();
        assert_eq!(f.adresse, "https://api.mohistmc.com/project/youer/1.21.1/builds/998/download");
        assert_eq!(f.empreinte, Empreinte::Sha256("b".repeat(64)));
        assert_eq!(nom_du_jar("Youer", &f), "server.jar");
        // Arclight 1.21.1 : la release stable la plus récente, et NeoForge plutôt que Forge ou Fabric.
        let f = trouver_fichier("Arclight", "1.21.1", &faux(&pages, &vues)).unwrap();
        assert_eq!(f.adresse, "https://github.com/IzzelAliz/Arclight/releases/download/FeudalKings/1.0.1/arclight-neoforge-1.21.1-1.0.1-8ec9529.jar");
        assert_eq!(f.empreinte, Empreinte::Sha256("a7ac57eee67c288df3c344e4f35a66d8513d6d0e1300bb58c81667836c2aebe3".into()));
        assert_eq!(nom_du_jar("Arclight", &f), "arclight-neoforge-1.21.1-1.0.1-8ec9529.jar");
        // En changeant de version, on garde le moteur d'avant (Fabric ici).
        let f = trouver_fichier_pour("Arclight", "1.21.1", Some("fabric"), &faux(&pages, &vues)).unwrap();
        assert_eq!(nom_du_jar("Arclight", &f), "arclight-fabric-1.21.1-1.0.1-8ec9529.jar");
        // 1.20.1 : seulement Forge, sans empreinte publiée.
        let f = trouver_fichier("Arclight", "1.20.1", &faux(&pages, &vues)).unwrap();
        assert_eq!(f.empreinte, Empreinte::Aucune);
        assert_eq!(nom_du_jar("Arclight", &f), "arclight-forge-1.20.1-1.0.6.jar");
        // 26.1.2 : seulement en préversion, mais on la prend si on la demande.
        assert!(trouver_fichier("Arclight", "26.1.2", &faux(&pages, &vues)).unwrap().adresse.contains("arclight-neoforge-26.1.2"));
        assert!(trouver_fichier("Arclight", "1.19.2", &faux(&pages, &vues)).is_err());
        assert!(trouver_fichier("Mohist", "1.20.1", &faux(&pages, &vues)).is_err());
    }

    #[test]
    fn refuse_un_fichier_sans_empreinte_hors_de_github() {
        let dossier = std::env::temp_dir().join("bhost-aucune");
        let _ = fs::create_dir_all(&dossier);
        let ailleurs = Fichier { adresse: "https://exemple.fr/x.jar".into(), empreinte: Empreinte::Aucune, taille: 0, java: None };
        assert!(telecharger(Cursor::new(b"abc".to_vec()), &ailleurs, &dossier.join("a.jar"), &mut |_| {}).is_err());
        let github = Fichier { adresse: format!("{ARCLIGHT_FICHIERS}Trials/1.0.6/arclight-forge-1.20.1-1.0.6.jar"), ..ailleurs };
        assert!(telecharger(Cursor::new(b"abc".to_vec()), &github, &dossier.join("b.jar"), &mut |_| {}).is_ok());
        let _ = fs::remove_dir_all(&dossier);
    }

    /// Un faux Internet : chaque adresse connue renvoie son texte, et on note les adresses demandées.
    fn faux<'a>(pages: &'a [(&'a str, &'a str)], vues: &'a RefCell<Vec<String>>) -> impl Fn(&str) -> Result<Vec<u8>, String> + 'a {
        move |adresse| {
            vues.borrow_mut().push(adresse.to_string());
            pages.iter().find(|(a, _)| *a == adresse).map(|(_, t)| t.as_bytes().to_vec()).ok_or_else(|| format!("404 {adresse}"))
        }
    }

    #[test]
    fn trouve_le_serveur_vanilla() {
        let pages = [
            ("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json", r#"{"versions":[{"id":"26.3","type":"release","url":"https://piston-meta.mojang.com/v1/packages/b/26.3.json"}]}"#),
            (
                "https://piston-meta.mojang.com/v1/packages/b/26.3.json",
                r#"{"downloads":{"server":{"sha1":"33680F5F2AC32864D6D7CF5E56A705FDB3E05F4C","size":62294556,"url":"https://piston-data.mojang.com/v1/objects/33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c/server.jar"}},"javaVersion":{"majorVersion":25}}"#,
            ),
        ];
        let vues = RefCell::new(Vec::new());
        let f = trouver_fichier("Vanilla", "26.3", &faux(&pages, &vues)).unwrap();
        assert_eq!(f.adresse, "https://piston-data.mojang.com/v1/objects/33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c/server.jar");
        assert_eq!(f.empreinte, Empreinte::Sha1("33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c".into()));
        assert_eq!(f.taille, 62294556);
        assert_eq!(f.java, Some(25));
        assert!(trouver_fichier("Vanilla", "1.0.0", &faux(&pages, &vues)).is_err());
    }

    #[test]
    fn trouve_le_serveur_paper_stable() {
        let builds = r#"[
            {"id":61,"channel":"BETA","downloads":{"server:default":{"name":"paper-1.21.8-61.jar","checksums":{"sha256":"aaaa"},"size":1,"url":"https://fill-data.papermc.io/v1/objects/aaaa/paper-1.21.8-61.jar"}}},
            {"id":60,"channel":"STABLE","downloads":{"server:default":{"name":"paper-1.21.8-60.jar","checksums":{"sha256":"8de7c52c3b02403503d16fac58003f1efef7dd7a0256786843927fa92ee57f1e"},"size":52811717,"url":"https://fill-data.papermc.io/v1/objects/8de7c52c3b02403503d16fac58003f1efef7dd7a0256786843927fa92ee57f1e/paper-1.21.8-60.jar"}}}]"#;
        let pages = [("https://fill.papermc.io/v3/projects/paper/versions/1.21.8/builds", builds)];
        let vues = RefCell::new(Vec::new());
        let f = trouver_fichier("Paper", "1.21.8", &faux(&pages, &vues)).unwrap();
        assert!(f.adresse.ends_with("/paper-1.21.8-60.jar"));
        assert_eq!(f.empreinte, Empreinte::Sha256("8de7c52c3b02403503d16fac58003f1efef7dd7a0256786843927fa92ee57f1e".into()));
        assert_eq!(f.taille, 52811717);
    }

    #[test]
    fn trouve_le_serveur_purpur() {
        let pages = [("https://api.purpurmc.org/v2/purpur/1.21.11/latest", r#"{"project":"purpur","version":"1.21.11","build":"2568","result":"SUCCESS","md5":"b8d5402ef8e38bf60cabc6eeddb3fa18"}"#)];
        let vues = RefCell::new(Vec::new());
        let f = trouver_fichier("Purpur", "1.21.11", &faux(&pages, &vues)).unwrap();
        assert_eq!(f.adresse, "https://api.purpurmc.org/v2/purpur/1.21.11/2568/download");
        assert_eq!(f.empreinte, Empreinte::Md5("b8d5402ef8e38bf60cabc6eeddb3fa18".into()));
    }

    #[test]
    fn refuse_une_version_bizarre() {
        let vues = RefCell::new(Vec::new());
        assert!(trouver_fichier("Paper", "../../x", &faux(&[], &vues)).is_err());
        assert!(trouver_fichier("Paper", "1.21?x=1", &faux(&[], &vues)).is_err());
        assert!(vues.borrow().is_empty());
    }

    fn dossier_test(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-creation-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn telecharge_et_verifie() {
        let d = dossier_test("telecharge");
        let contenu = b"un faux serveur".to_vec();
        let sha1: String = Sha1::digest(&contenu).iter().map(|b| format!("{b:02x}")).collect();
        let md5: String = Md5::digest(&contenu).iter().map(|b| format!("{b:02x}")).collect();
        let jar = d.join("server.jar");
        let mut etapes = Vec::new();
        let fichier = Fichier { adresse: String::new(), empreinte: Empreinte::Sha1(sha1), taille: contenu.len() as u64, java: None };
        telecharger(Cursor::new(contenu.clone()), &fichier, &jar, &mut |p| etapes.push(p)).unwrap();
        assert_eq!(fs::read(&jar).unwrap(), contenu);
        assert_eq!(etapes.last(), Some(&100));
        // MD5 (Purpur), taille inconnue.
        let fichier = Fichier { adresse: String::new(), empreinte: Empreinte::Md5(md5), taille: 0, java: None };
        telecharger(Cursor::new(contenu.clone()), &fichier, &d.join("b.jar"), &mut |_| {}).unwrap();
        // Mauvaise empreinte : rien n'est gardé.
        let fichier = Fichier { adresse: String::new(), empreinte: Empreinte::Sha256("0".repeat(64)), taille: 0, java: None };
        let erreur = telecharger(Cursor::new(contenu), &fichier, &d.join("c.jar"), &mut |_| {}).unwrap_err();
        assert!(erreur.contains("abîmé"));
        assert!(!d.join("c.jar").exists() && !d.join("c.jar.part").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn fabrique_des_noms_de_dossier_propres() {
        assert_eq!(nom_de_dossier("Survie du week-end"), "survie-du-week-end");
        assert_eq!(nom_de_dossier("  Créatif   à 2 !! "), "creatif-a-2");
        assert_eq!(nom_de_dossier("../../Windows"), "windows");
        assert_eq!(nom_de_dossier("???"), "serveur");
        assert_eq!(nom_de_dossier("CON"), "con-serveur");
        assert_eq!(nom_de_dossier("com1"), "com1-serveur");
        assert_eq!(nom_de_dossier("Console"), "console");
    }

    #[test]
    fn cree_un_dossier_neuf_sans_ecraser() {
        let d = dossier_test("dossiers");
        assert_eq!(creer_dossier(&d, "Survie").unwrap(), d.join("survie"));
        assert_eq!(creer_dossier(&d, "survie").unwrap(), d.join("survie-2"));
        assert_eq!(creer_dossier(&d, "Survie!").unwrap(), d.join("survie-3"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn ecrit_les_fichiers_du_serveur() {
        let d = dossier_test("fichiers");
        let monde = Monde { mode: "creatif".into(), difficulte: "facile".into(), joueurs_max: 12 };
        ecrire_fichiers(&d, "Créa des potes", "26.3", &monde, 25566).unwrap();
        let p = fs::read_to_string(d.join("server.properties")).unwrap();
        assert!(p.contains("motd=Cr\\u00e9a des potes\n"));
        assert!(p.contains("server-port=25566\n") && p.contains("gamemode=creative\n") && p.contains("difficulty=easy\n"));
        assert!(p.contains("hardcore=false\n") && p.contains("max-players=12\n"));
        assert!(p.is_ascii());
        assert!(fs::read_to_string(d.join("eula.txt")).unwrap().contains("eula=true"));
        // Hardcore : difficile d'office. Vieille version : des chiffres.
        let monde = Monde { mode: "hardcore".into(), difficulte: "paisible".into(), joueurs_max: 500 };
        ecrire_fichiers(&d, "Old", "1.8.9", &monde, 25565).unwrap();
        assert_eq!(fs::read_to_string(d.join("ops.json")).unwrap(), "[]\n");
        let p = fs::read_to_string(d.join("server.properties")).unwrap();
        assert!(p.contains("gamemode=0\n") && p.contains("difficulty=3\n") && p.contains("hardcore=true\n") && p.contains("max-players=100\n"));
        // Le nom ne peut pas ajouter de ligne au fichier.
        let monde = Monde { mode: "survie".into(), difficulte: "normale".into(), joueurs_max: 10 };
        ecrire_fichiers(&d, "Nom\nonline-mode=false", "26.3", &monde, 25565).unwrap();
        assert!(!fs::read_to_string(d.join("server.properties")).unwrap().contains("\nonline-mode"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn choisit_un_port_libre() {
        assert_eq!(port_libre(&[]), 25565);
        assert_eq!(port_libre(&[25565, 25566]), 25567);
    }
}
