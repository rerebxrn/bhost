// Spigot ne se télécharge pas : il se fabrique sur le PC avec BuildTools, l'outil officiel de SpigotMC.
// BuildTools télécharge le serveur de Mojang, les sources de Spigot (avec Git ; sur Windows il prend lui-même
// une copie portable de Git si besoin), applique les modifications de Spigot et compile. Compter 5 à 15 minutes.
// Son dossier de travail (%APPDATA%\BHOST\buildtools) est gardé : les fois suivantes vont plus vite.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test), sans Internet.

use crate::creation::VersionMc;

/// L'outil officiel, toujours la dernière version réussie (HTTPS, site de SpigotMC).
pub const ADRESSE_BUILDTOOLS: &str = "https://hub.spigotmc.org/jenkins/job/BuildTools/lastSuccessfulBuild/artifact/target/BuildTools.jar";
/// La liste des versions que BuildTools sait fabriquer (une page de fichiers « 1.21.4.json »…).
pub const ADRESSE_VERSIONS: &str = "https://hub.spigotmc.org/versions/";
/// Une fabrication prend du temps : 40 minutes au plus.
pub const DUREE_MAX_MINUTES: u64 = 40;

/// « 1.21.4 » → [1, 21, 4]. `None` si ce ne sont pas que des chiffres (latest, numéros de build…).
fn nombres(version: &str) -> Option<Vec<u32>> {
    let n: Vec<u32> = version.split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    (n.len() >= 2).then_some(n)
}

/// Lit la page des versions : on garde les fichiers « 1.21.4.json », à partir de la 1.8, du plus récent au plus ancien.
pub fn lire_versions(page: &[u8]) -> Result<Vec<VersionMc>, String> {
    let texte = String::from_utf8_lossy(page);
    let mut versions: Vec<(Vec<u32>, String)> = texte
        .split("href=\"")
        .skip(1)
        .filter_map(|morceau| morceau.split_once('"').map(|(lien, _)| lien))
        .filter_map(|lien| lien.rsplit('/').next()?.strip_suffix(".json"))
        .filter_map(|v| Some((nombres(v)?, v.to_string())))
        .filter(|(n, _)| n.as_slice() >= [1, 8].as_slice())
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.dedup_by(|a, b| a.0 == b.0);
    if versions.is_empty() {
        return Err("Liste des versions illisible.".into());
    }
    Ok(versions.into_iter().map(|(_, id)| VersionMc { java: crate::java::requise(&id), id, instable: false }).collect())
}

/// Si la page de SpigotMC ne répond pas : les versions de Mojang à partir de la 1.8 (BuildTools dira si l'une manque).
pub fn depuis_mojang(versions: Vec<VersionMc>) -> Vec<VersionMc> {
    versions.into_iter().filter(|v| !v.instable && nombres(&v.id).is_some_and(|n| n.as_slice() >= [1, 8].as_slice())).collect()
}

/// Ce qu'on donne à BuildTools : la version, le dossier du serveur, le nom du fichier final.
pub fn arguments(version: &str, dossier_serveur: &str) -> Vec<String> {
    ["--nogui", "--rev", version, "--output-dir", dossier_serveur, "--final-name", "server.jar"].map(String::from).to_vec()
}

/// L'avancement d'après ce que BuildTools écrit (à peu près : il ne donne pas de pourcentage).
pub fn avancement(ligne: &str) -> Option<u8> {
    let l = ligne.to_ascii_lowercase();
    Some(if l.contains("success! everything completed") {
        100
    } else if l.contains("compiling spigot") || l.contains("building spigot") {
        85
    } else if l.contains("compiling craftbukkit") || l.contains("building craftbukkit") {
        75
    } else if l.contains("compiling bukkit") || l.contains("building bukkit") {
        65
    } else if l.contains("applying patches") || l.contains("rebuilding") {
        50
    } else if l.contains("applying craftbukkit") || l.contains("patching with") {
        40
    } else if l.contains("decompil") || l.contains("final mapped jar") {
        25
    } else if l.contains("cloning") || l.contains("pulling updates") || l.contains("portablegit") {
        12
    } else if l.contains("attempting to build version") || l.contains("loading buildtools") {
        5
    } else {
        return None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_la_page_des_versions() {
        let page = br#"<html><body><pre><a href="../">../</a>
<a href="1.10.2.json">1.10.2.json</a>  01-Jan-2017
<a href="1.21.11.json">1.21.11.json</a>
<a href="1.21.4.json">1.21.4.json</a>
<a href="1.7.10.json">1.7.10.json</a>
<a href="1.8.8.json">1.8.8.json</a>
<a href="26.1.json">26.1.json</a>
<a href="4241.json">4241.json</a>
<a href="latest.json">latest.json</a>
<a href="1.21.4-pre1.json">1.21.4-pre1.json</a>
</pre></body></html>"#;
        let v = lire_versions(page).unwrap();
        assert_eq!(v.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["26.1", "1.21.11", "1.21.4", "1.10.2", "1.8.8"]);
        assert_eq!(v[0].java, Some(25));
        assert_eq!(v[4].java, Some(8));
        assert!(lire_versions(b"<html>Cloudflare</html>").is_err());
    }

    #[test]
    fn prend_les_versions_de_mojang_sinon() {
        let mojang = vec![
            VersionMc { id: "26.3".into(), java: Some(25), instable: false },
            VersionMc { id: "26.4-snapshot-1".into(), java: Some(25), instable: true },
            VersionMc { id: "1.7.10".into(), java: Some(8), instable: false },
        ];
        assert_eq!(depuis_mojang(mojang).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["26.3"]);
    }

    #[test]
    fn construit_la_commande_et_suit_l_avancement() {
        assert_eq!(arguments("1.21.4", r"C:\serveurs\survie"), ["--nogui", "--rev", "1.21.4", "--output-dir", r"C:\serveurs\survie", "--final-name", "server.jar"]);
        assert_eq!(avancement("Attempting to build version: '1.21.4' use --rev <version> to override"), Some(5));
        assert_eq!(avancement("Success! Everything completed successfully. Copying final .jar files now."), Some(100));
        assert_eq!(avancement("[INFO] Downloading from central"), None);
    }
}
