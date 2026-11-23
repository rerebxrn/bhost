// L'onglet Fichiers : parcourir le dossier d'un serveur, lire et modifier les fichiers texte,
// importer des fichiers, changer l'icône du serveur (server-icon.png, 64 × 64).
// On ne sort jamais du dossier du serveur. Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use base64::Engine;
use serde::Serialize;
use std::fs;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Au-delà, un fichier texte n'est pas affiché (on propose l'Explorateur).
const TEXTE_MAX: u64 = 1024 * 1024;
/// Une image plus grosse n'est pas affichée.
const IMAGE_MAX: u64 = 4 * 1024 * 1024;

/// Les fichiers qu'on sait lire comme du texte.
const TEXTES: [&str; 22] = [
    "txt", "properties", "yml", "yaml", "json", "json5", "toml", "cfg", "conf", "ini", "log", "md", "csv", "sk", "mcmeta", "lang", "bat", "cmd", "sh", "xml", "snbt", "mcfunction",
];

/// Un chemin relatif sûr (« plugins/Essentials/config.yml ») → chemin complet dans le serveur. Refuse « .. » et les chemins absolus.
pub fn chemin_sur(racine: &Path, relatif: &str) -> Result<PathBuf, String> {
    let mut chemin = racine.to_path_buf();
    for morceau in relatif.split(['/', '\\']).filter(|m| !m.is_empty()) {
        let mut composants = Path::new(morceau).components();
        match (composants.next(), composants.next()) {
            (Some(Component::Normal(c)), None) if !morceau.contains(':') => chemin.push(c),
            _ => return Err("Chemin refusé.".into()),
        }
    }
    // Liens et jonctions résolus, le chemin doit rester dans le serveur.
    crate::garde::dans(racine, &chemin)?;
    Ok(chemin)
}

fn millisecondes(meta: &fs::Metadata) -> Option<u64> {
    meta.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Un élément d'un dossier.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Element {
    pub nom: String,
    pub dossier: bool,
    /// Poids (fichiers), ou nombre d'éléments (dossiers).
    pub taille: u64,
    /// Modifié à cette heure (millisecondes).
    pub modifie: Option<u64>,
}

/// Le contenu d'un dossier : les dossiers d'abord, puis les fichiers, par nom.
pub fn lister(racine: &Path, relatif: &str) -> Result<Vec<Element>, String> {
    let dossier = chemin_sur(racine, relatif)?;
    let entrees = fs::read_dir(&dossier).map_err(|_| "Ce dossier n’existe plus.".to_string())?;
    let mut liste: Vec<Element> = entrees
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let est_dossier = meta.is_dir();
            let taille = if est_dossier { fs::read_dir(e.path()).map(|d| d.count() as u64).unwrap_or(0) } else { meta.len() };
            Some(Element { nom: e.file_name().to_string_lossy().into_owned(), dossier: est_dossier, taille, modifie: millisecondes(&meta) })
        })
        .collect();
    liste.sort_by(|a, b| b.dossier.cmp(&a.dossier).then_with(|| a.nom.to_lowercase().cmp(&b.nom.to_lowercase())));
    Ok(liste)
}

/// Ce qu'on montre d'un fichier.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "genre", rename_all = "camelCase")]
pub enum Contenu {
    /// Du texte (UTF-8, ou Latin-1 pour les vieux fichiers de Java).
    Texte { texte: String, latin1: bool },
    /// Une image PNG, prête à afficher (data:…).
    Image { adresse: String },
    /// Trop gros pour être affiché ici.
    TropGros,
    /// Un fichier que BHOST n'ouvre pas (.jar, .dat, .mca…).
    Binaire,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fichier {
    #[serde(flatten)]
    pub contenu: Contenu,
    pub taille: u64,
    pub modifie: Option<u64>,
}

fn extension(chemin: &Path) -> String {
    chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Lit un fichier du serveur pour l'afficher.
pub fn lire(racine: &Path, relatif: &str) -> Result<Fichier, String> {
    let chemin = chemin_sur(racine, relatif)?;
    let meta = fs::metadata(&chemin).map_err(|_| "Ce fichier n’existe plus.".to_string())?;
    if meta.is_dir() {
        return Err("C’est un dossier.".into());
    }
    let (taille, modifie) = (meta.len(), millisecondes(&meta));
    let ext = extension(&chemin);
    let contenu = if ext == "png" {
        if taille > IMAGE_MAX {
            Contenu::TropGros
        } else {
            let octets = fs::read(&chemin).map_err(|e| e.to_string())?;
            Contenu::Image { adresse: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(octets)) }
        }
    } else if TEXTES.contains(&ext.as_str()) || (ext.is_empty() && taille <= 64 * 1024) {
        if taille > TEXTE_MAX {
            Contenu::TropGros
        } else {
            let octets = fs::read(&chemin).map_err(|e| e.to_string())?;
            match String::from_utf8(octets) {
                Ok(texte) => Contenu::Texte { texte, latin1: false },
                // Un fichier sans extension qui n'est pas du texte : binaire.
                Err(_) if ext.is_empty() => Contenu::Binaire,
                Err(e) => Contenu::Texte { texte: e.into_bytes().iter().map(|&o| o as char).collect(), latin1: true },
            }
        }
    } else {
        Contenu::Binaire
    };
    Ok(Fichier { contenu, taille, modifie })
}

/// Enregistre un fichier texte modifié. Garde son encodage (Latin-1) et ses fins de ligne (Windows).
pub fn ecrire(racine: &Path, relatif: &str, texte: &str) -> Result<(), String> {
    let chemin = chemin_sur(racine, relatif)?;
    if !TEXTES.contains(&extension(&chemin).as_str()) && !extension(&chemin).is_empty() {
        return Err("Ce fichier ne se modifie pas ici.".into());
    }
    let avant = fs::read(&chemin).unwrap_or_default();
    let latin1 = String::from_utf8(avant.clone()).is_err();
    let crlf = avant.windows(2).any(|w| w == b"\r\n");
    let texte = texte.replace("\r\n", "\n");
    let texte = if crlf { texte.replace('\n', "\r\n") } else { texte };
    let octets: Vec<u8> = if latin1 { texte.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect() } else { texte.into_bytes() };
    // On écrit à côté, puis on remplace : un fichier n'est jamais à moitié écrit.
    let provisoire = chemin.with_extension("bhost-tmp");
    fs::write(&provisoire, octets).map_err(|e| format!("Impossible d’enregistrer ({e})."))?;
    fs::rename(&provisoire, &chemin).map_err(|e| {
        let _ = fs::remove_file(&provisoire);
        format!("Impossible d’enregistrer ({e}).")
    })
}

/// Un nom libre dans `dossier` : « config.yml », sinon « config (2).yml »…
fn nom_libre(dossier: &Path, nom: &str) -> PathBuf {
    let chemin = dossier.join(nom);
    if !chemin.exists() {
        return chemin;
    }
    let (base, ext) = match nom.rsplit_once('.') {
        Some((b, e)) if !b.is_empty() => (b.to_string(), format!(".{e}")),
        _ => (nom.to_string(), String::new()),
    };
    (2..1000).map(|i| dossier.join(format!("{base} ({i}){ext}"))).find(|p| !p.exists()).unwrap_or(chemin)
}

/// Copie des fichiers (ou des dossiers) du PC dans un dossier du serveur. Ne remplace rien : « nom (2) » si besoin.
/// Renvoie le nombre d'éléments copiés.
pub fn importer(racine: &Path, relatif: &str, sources: &[PathBuf]) -> Result<usize, String> {
    let dossier = chemin_sur(racine, relatif)?;
    if !dossier.is_dir() {
        return Err("Ce dossier n’existe plus.".into());
    }
    let racine_vraie = racine.canonicalize().unwrap_or_else(|_| racine.to_path_buf());
    let mut copies = 0;
    for source in sources {
        let Some(nom) = source.file_name() else { continue };
        // Pas de copie du serveur dans lui-même.
        if source.canonicalize().is_ok_and(|s| racine_vraie.starts_with(&s)) {
            return Err("Impossible de copier le serveur dans lui-même.".into());
        }
        let cible = nom_libre(&dossier, &nom.to_string_lossy());
        if source.is_dir() {
            fs::create_dir(&cible).map_err(|e| format!("Copie impossible ({e})."))?;
            crate::copie::copier(source, &cible, &mut |_| {})?;
        } else if source.is_file() {
            fs::copy(source, &cible).map_err(|e| format!("Copie impossible ({e})."))?;
        } else {
            continue;
        }
        copies += 1;
    }
    Ok(copies)
}

/// Fait l'icône du serveur (server-icon.png, 64 × 64) à partir d'une image PNG, recadrée au centre.
pub fn icone(source: &Path, racine: &Path) -> Result<(), String> {
    let illisible = |_| "Image illisible (il faut un fichier PNG).".to_string();
    let octets = fs::read(source).map_err(|_| "Image introuvable.".to_string())?;
    let mut decodeur = png::Decoder::new(Cursor::new(octets));
    decodeur.set_transformations(png::Transformations::normalize_to_color8());
    let mut lecteur = decodeur.read_info().map_err(illisible)?;
    let mut brut = vec![0; lecteur.output_buffer_size().ok_or("Image trop grande.")?];
    let image = lecteur.next_frame(&mut brut).map_err(illisible)?;
    let (largeur, hauteur) = (image.width as usize, image.height as usize);
    let par_pixel = match image.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("Image illisible.".into()),
    };
    let ligne = image.line_size;
    let pixel = |x: usize, y: usize| -> [u32; 4] {
        let p = &brut[y * ligne + x * par_pixel..][..par_pixel];
        match par_pixel {
            1 => [p[0].into(), p[0].into(), p[0].into(), 255],
            2 => [p[0].into(), p[0].into(), p[0].into(), p[1].into()],
            3 => [p[0].into(), p[1].into(), p[2].into(), 255],
            _ => [p[0].into(), p[1].into(), p[2].into(), p[3].into()],
        }
    };
    // Le plus grand carré au centre, réduit en 64 × 64 (moyenne des pixels de chaque case).
    let cote = largeur.min(hauteur);
    if cote == 0 {
        return Err("Image vide.".into());
    }
    let (x0, y0) = ((largeur - cote) / 2, (hauteur - cote) / 2);
    let mut sortie = Vec::with_capacity(64 * 64 * 4);
    for ty in 0..64 {
        for tx in 0..64 {
            let (xa, xb) = (x0 + tx * cote / 64, (x0 + (tx + 1) * cote / 64).max(x0 + tx * cote / 64 + 1));
            let (ya, yb) = (y0 + ty * cote / 64, (y0 + (ty + 1) * cote / 64).max(y0 + ty * cote / 64 + 1));
            let mut somme = [0u32; 4];
            let mut n = 0;
            for y in ya..yb.min(hauteur) {
                for x in xa..xb.min(largeur) {
                    let p = pixel(x, y);
                    (0..4).for_each(|i| somme[i] += p[i]);
                    n += 1;
                }
            }
            sortie.extend(somme.iter().map(|s| (s / n.max(1)) as u8));
        }
    }
    let mut png_sortie = Vec::new();
    let mut encodeur = png::Encoder::new(&mut png_sortie, 64, 64);
    encodeur.set_color(png::ColorType::Rgba);
    encodeur.set_depth(png::BitDepth::Eight);
    let mut ecriture = encodeur.write_header().map_err(|e| e.to_string())?;
    ecriture.write_image_data(&sortie).map_err(|e| e.to_string())?;
    ecriture.finish().map_err(|e| e.to_string())?;
    crate::garde::ecrire_atomique(&racine.join("server-icon.png"), &png_sortie).map_err(|e| format!("Impossible d’écrire l’icône. {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bhost-fichiers-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn refuse_de_sortir_du_serveur() {
        let r = Path::new("/serveur");
        assert_eq!(chemin_sur(r, "plugins/Essentials/config.yml").unwrap(), r.join("plugins").join("Essentials").join("config.yml"));
        assert_eq!(chemin_sur(r, "").unwrap(), r);
        assert!(chemin_sur(r, "../x").is_err());
        assert!(chemin_sur(r, "plugins/../../x").is_err());
        assert!(chemin_sur(r, "C:/Windows").is_err());
        assert!(chemin_sur(r, "plugins/./x").is_err());
    }

    #[test]
    fn liste_lit_et_ecrit() {
        let d = dossier("lire");
        fs::create_dir_all(d.join("plugins")).unwrap();
        fs::write(d.join("plugins").join("a.jar"), b"PK").unwrap();
        fs::write(d.join("server.properties"), "motd=Caf\u{e9}\r\nmax-players=20\r\n".chars().map(|c| c as u8).collect::<Vec<u8>>()).unwrap();
        fs::write(d.join("eula.txt"), "eula=true\n").unwrap();
        fs::write(d.join("server.jar"), b"PK\x03\x04").unwrap();
        let l = lister(&d, "").unwrap();
        assert_eq!(l.iter().map(|e| e.nom.as_str()).collect::<Vec<_>>(), ["plugins", "eula.txt", "server.jar", "server.properties"]);
        assert!(l[0].dossier && l[0].taille == 1);
        // Latin-1 : relu, modifié, réécrit pareil (accents et fins de ligne Windows).
        let f = lire(&d, "server.properties").unwrap();
        assert_eq!(f.contenu, Contenu::Texte { texte: "motd=Café\r\nmax-players=20\r\n".into(), latin1: true });
        ecrire(&d, "server.properties", "motd=Café !\nmax-players=10\n").unwrap();
        assert_eq!(fs::read(d.join("server.properties")).unwrap(), b"motd=Caf\xe9 !\r\nmax-players=10\r\n");
        assert_eq!(lire(&d, "server.jar").unwrap().contenu, Contenu::Binaire);
        assert!(ecrire(&d, "server.jar", "x").is_err());
        assert!(lire(&d, "plugins").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn importe_sans_rien_ecraser() {
        let d = dossier("importer");
        let serveur = d.join("serveur");
        fs::create_dir_all(serveur.join("plugins")).unwrap();
        fs::write(serveur.join("plugins").join("a.jar"), b"ancien").unwrap();
        let pc = d.join("pc");
        fs::create_dir_all(pc.join("Config")).unwrap();
        fs::write(pc.join("a.jar"), b"neuf").unwrap();
        fs::write(pc.join("Config").join("x.yml"), b"x: 1").unwrap();
        assert_eq!(importer(&serveur, "plugins", &[pc.join("a.jar"), pc.join("Config")]).unwrap(), 2);
        assert_eq!(fs::read(serveur.join("plugins").join("a.jar")).unwrap(), b"ancien");
        assert_eq!(fs::read(serveur.join("plugins").join("a (2).jar")).unwrap(), b"neuf");
        assert!(serveur.join("plugins").join("Config").join("x.yml").is_file());
        assert!(importer(&serveur, "plugins", &[d.clone()]).is_err()); // le serveur dans lui-même
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn fabrique_une_icone() {
        let d = dossier("icone");
        // Une image 100 × 50 (RGB) : moitié gauche rouge, moitié droite bleue.
        let mut brut = Vec::new();
        for _ in 0..50 {
            for x in 0..100 {
                brut.extend(if x < 50 { [255u8, 0, 0] } else { [0, 0, 255] });
            }
        }
        let mut png_octets = Vec::new();
        let mut e = png::Encoder::new(&mut png_octets, 100, 50);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&brut).unwrap();
        w.finish().unwrap();
        fs::write(d.join("image.png"), &png_octets).unwrap();
        icone(&d.join("image.png"), &d).unwrap();
        let mut lecteur = png::Decoder::new(Cursor::new(fs::read(d.join("server-icon.png")).unwrap())).read_info().unwrap();
        let mut sortie = vec![0; lecteur.output_buffer_size().unwrap()];
        let info = lecteur.next_frame(&mut sortie).unwrap();
        assert_eq!((info.width, info.height), (64, 64));
        assert_eq!(&sortie[..4], &[255, 0, 0, 255]); // en haut à gauche : rouge
        assert_eq!(&sortie[63 * 4..64 * 4], &[0, 0, 255, 255]); // en haut à droite : bleu
        assert!(icone(&d.join("rien.png"), &d).is_err());
        fs::write(d.join("faux.png"), b"pas une image").unwrap();
        assert!(icone(&d.join("faux.png"), &d).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
