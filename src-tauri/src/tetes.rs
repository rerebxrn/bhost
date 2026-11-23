// Les têtes des joueurs, prises directement chez Mojang (aucun service tiers).
// Demandes : (pseudo → identifiant du compte) → adresse du skin → image du skin.
// L'identifiant est souvent déjà connu : le serveur l'écrit dans sa console à chaque connexion.
// Dans le skin, BHOST découpe le visage (8 × 8 pixels) et pose le casque par-dessus.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test), sans Internet.

use base64::Engine;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

/// Une tête gardée sur le disque est reprise telle quelle pendant un jour.
const FRAICHEUR: Duration = Duration::from_secs(24 * 3600);

/// De quoi télécharger une adresse : `None` si elle n'existe pas, une phrase si le réseau a échoué.
pub type Telecharger<'a> = &'a dyn Fn(&str) -> Result<Option<Vec<u8>>, String>;

/// Un pseudo de Minecraft Java : 1 à 16 lettres, chiffres ou tirets bas.
/// Les joueurs Bedrock (un point devant) n'ont pas de compte Java, donc pas de tête chez Mojang.
pub fn pseudo_java(nom: &str) -> bool {
    (1..=16).contains(&nom.len()) && nom.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// L'identifiant du compte, dans la réponse de api.mojang.com : {"id":"069a…","name":"Notch"}.
fn identifiant(reponse: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(reponse).ok()?;
    let id = json.get("id")?.as_str()?;
    (id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| id.to_string())
}

/// L'adresse du skin, dans la réponse de sessionserver.mojang.com.
/// Elle est rangée dans la propriété « textures », écrite en base64.
/// `None` si le joueur a gardé le skin par défaut.
fn adresse_skin(reponse: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(reponse).ok()?;
    let valeur = json
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))?
        .get("value")?
        .as_str()?;
    let textures: serde_json::Value = serde_json::from_slice(&base64::engine::general_purpose::STANDARD.decode(valeur).ok()?).ok()?;
    let adresse = textures.get("textures")?.get("SKIN")?.get("url")?.as_str()?;
    // On n'accepte que le serveur d'images de Mojang, et on passe toujours en HTTPS.
    let chemin = adresse.strip_prefix("http://textures.minecraft.net/texture/").or(adresse.strip_prefix("https://textures.minecraft.net/texture/"))?;
    (!chemin.is_empty() && chemin.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("https://textures.minecraft.net/texture/{chemin}"))
}

/// Ouvre une image PNG. Renvoie sa largeur, sa hauteur et ses pixels (4 octets chacun : rouge, vert, bleu, opacité).
fn ouvrir_png(octets: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let illisible = |_| "Skin illisible.".to_string();
    let mut decodeur = png::Decoder::new(Cursor::new(octets));
    decodeur.set_transformations(png::Transformations::normalize_to_color8());
    let mut lecteur = decodeur.read_info().map_err(illisible)?;
    let (largeur, hauteur) = (lecteur.info().width as usize, lecteur.info().height as usize);
    // Un skin fait 64 × 64 (ou 64 × 32 pour les anciens). On refuse le reste avant de réserver de la mémoire.
    if largeur != 64 || (hauteur != 64 && hauteur != 32) {
        return Err("Ce n’est pas un skin.".into());
    }
    let mut brut = vec![0; lecteur.output_buffer_size().ok_or("Skin illisible.")?];
    let image = lecteur.next_frame(&mut brut).map_err(illisible)?;
    let par_pixel = match image.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("Skin illisible.".into()),
    };
    let mut pixels = Vec::with_capacity(largeur * hauteur * 4);
    for ligne in brut[..image.buffer_size()].chunks_exact(image.line_size) {
        for p in ligne.chunks_exact(par_pixel) {
            pixels.extend_from_slice(&match par_pixel {
                1 => [p[0], p[0], p[0], 255],
                2 => [p[0], p[0], p[0], p[1]],
                3 => [p[0], p[1], p[2], 255],
                _ => [p[0], p[1], p[2], p[3]],
            });
        }
    }
    Ok((largeur, hauteur, pixels))
}

/// Découpe le visage dans un skin : une image PNG de 8 × 8 pixels, casque compris.
pub fn visage(skin: &[u8]) -> Result<Vec<u8>, String> {
    let (largeur, hauteur, pixels) = ouvrir_png(skin)?;
    let pixel = |x: usize, y: usize| -> [u8; 4] { pixels[(y * largeur + x) * 4..][..4].try_into().unwrap() };
    // Anciens skins (64 × 32) : la zone du casque est parfois pleine de couleur opaque sans rien dessiner.
    // Comme le jeu, on ignore alors le casque.
    let ancien = hauteur == 32;
    let casque_vide = ancien && (0..32).all(|y| (32..64).all(|x| pixel(x, y)[3] >= 128));
    let mut tete = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8 {
        for x in 0..8 {
            let peau = pixel(8 + x, 8 + y);
            let casque = if casque_vide { [0; 4] } else { pixel(40 + x, 8 + y) };
            // On pose le casque sur la peau, selon son opacité.
            let a = casque[3] as u32;
            let melange = |dessous: u8, dessus: u8| ((dessus as u32 * a + dessous as u32 * (255 - a) + 127) / 255) as u8;
            tete.extend_from_slice(&[melange(peau[0], casque[0]), melange(peau[1], casque[1]), melange(peau[2], casque[2]), 255]);
        }
    }
    let mut sortie = Vec::new();
    let mut encodeur = png::Encoder::new(&mut sortie, 8, 8);
    encodeur.set_color(png::ColorType::Rgba);
    encodeur.set_depth(png::BitDepth::Eight);
    let mut ecriture = encodeur.write_header().map_err(|e| e.to_string())?;
    ecriture.write_image_data(&tete).map_err(|e| e.to_string())?;
    ecriture.finish().map_err(|e| e.to_string())?;
    Ok(sortie)
}

/// Mojang renvoie parfois une erreur au hasard (souvent un 403) : on réessaie une fois.
fn avec_essai(telecharger: Telecharger, adresse: &str) -> Result<Option<Vec<u8>>, String> {
    telecharger(adresse).or_else(|_| telecharger(adresse))
}

/// Retrouve l'identifiant du compte à partir du pseudo. Mojang a deux adresses pour ça :
/// si la nouvelle échoue, on essaie l'ancienne.
pub fn identifiant_du_pseudo(telecharger: Telecharger, nom: &str) -> Result<Option<String>, String> {
    let reponse = telecharger(&format!("https://api.minecraftservices.com/minecraft/profile/lookup/name/{nom}"))
        .or_else(|_| avec_essai(telecharger, &format!("https://api.mojang.com/users/profiles/minecraft/{nom}")))?;
    Ok(reponse.and_then(|r| identifiant(&r)))
}

/// Un identifiant de compte Mojang est tiré au hasard (version 4).
/// Un serveur « hors ligne » (sans vérification des comptes) en fabrique un autre (version 3) : pas de skin chez Mojang.
fn compte_mojang(uuid: &str) -> bool {
    uuid.len() == 32 && uuid.chars().all(|c| c.is_ascii_hexdigit()) && uuid.as_bytes()[12] == b'4'
}

/// Demande à Mojang la tête d'un joueur. `None` : pas de compte à ce nom, ou skin par défaut.
/// `uuid` : l'identifiant lu dans la console du serveur, s'il est connu (sans tirets).
pub fn chercher(telecharger: Telecharger, nom: &str, uuid: Option<&str>) -> Result<Option<Vec<u8>>, String> {
    if !pseudo_java(nom) {
        return Ok(None);
    }
    let id = match uuid {
        Some(uuid) if compte_mojang(uuid) => uuid.to_string(),
        Some(_) => return Ok(None), // serveur hors ligne : Mojang ne connaît pas ce joueur
        None => match identifiant_du_pseudo(telecharger, nom)? {
            Some(id) => id,
            None => return Ok(None),
        },
    };
    let Some(profil) = avec_essai(telecharger, &format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}"))? else {
        return Ok(None);
    };
    let Some(adresse) = adresse_skin(&profil) else { return Ok(None) };
    let Some(skin) = avec_essai(telecharger, &adresse)? else { return Ok(None) };
    visage(&skin).map(Some)
}

/// La tête d'un joueur, en passant par le dossier `dossier` où BHOST garde celles qu'il connaît.
/// Mojang limite le nombre de demandes : on ne redemande une tête qu'une fois par jour.
pub fn avec_cache(dossier: &Path, nom: &str, chercher: impl FnOnce() -> Result<Option<Vec<u8>>, String>) -> Option<Vec<u8>> {
    if !pseudo_java(nom) {
        return None;
    }
    let fichier = dossier.join(format!("{}.png", nom.to_ascii_lowercase()));
    let gardee = fs::read(&fichier).ok();
    let fraiche = fs::metadata(&fichier).and_then(|m| m.modified()).ok().and_then(|date| date.elapsed().ok()).is_some_and(|age| age < FRAICHEUR);
    if gardee.is_some() && fraiche {
        return gardee;
    }
    match chercher() {
        Ok(Some(tete)) => {
            let _ = fs::create_dir_all(dossier);
            let _ = fs::write(&fichier, &tete);
            Some(tete)
        }
        Ok(None) => {
            // Le joueur n'a plus de skin (ou plus de compte) : on oublie l'ancienne tête.
            let _ = fs::remove_file(&fichier);
            None
        }
        // Pas d'Internet : on garde l'ancienne tête, même si elle date.
        Err(_) => gardee,
    }
}

/// L'image sous la forme d'une adresse « data: », que l'interface peut afficher directement.
pub fn en_adresse(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const ROUGE: [u8; 4] = [200, 30, 30, 255];
    const BLEU: [u8; 4] = [20, 40, 220, 255];
    const NOIR: [u8; 4] = [0, 0, 0, 255];
    const VIDE: [u8; 4] = [0, 0, 0, 0];

    /// Fabrique un skin : visage rouge, et la zone du casque remplie avec `casque`.
    fn skin(hauteur: u32, avec_opacite: bool, casque: [u8; 4]) -> Vec<u8> {
        let mut pixels = vec![VIDE; 64 * hauteur as usize];
        for y in 0..hauteur as usize {
            for x in 0..64 {
                pixels[y * 64 + x] = if (8..16).contains(&x) && (8..16).contains(&y) {
                    ROUGE
                } else if x >= 32 && y < 16 {
                    casque
                } else {
                    [90, 90, 90, 255]
                };
            }
        }
        let octets: Vec<u8> = pixels.iter().flat_map(|p| if avec_opacite { p.to_vec() } else { p[..3].to_vec() }).collect();
        let mut sortie = Vec::new();
        let mut encodeur = png::Encoder::new(&mut sortie, 64, hauteur);
        encodeur.set_color(if avec_opacite { png::ColorType::Rgba } else { png::ColorType::Rgb });
        encodeur.set_depth(png::BitDepth::Eight);
        let mut ecriture = encodeur.write_header().unwrap();
        ecriture.write_image_data(&octets).unwrap();
        ecriture.finish().unwrap();
        sortie
    }

    /// Les pixels d'une tête de 8 × 8.
    fn pixels(tete: &[u8]) -> Vec<[u8; 4]> {
        let mut decodeur = png::Decoder::new(Cursor::new(tete));
        decodeur.set_transformations(png::Transformations::normalize_to_color8());
        let mut lecteur = decodeur.read_info().unwrap();
        assert_eq!((lecteur.info().width, lecteur.info().height), (8, 8));
        let mut brut = vec![0; lecteur.output_buffer_size().unwrap()];
        let image = lecteur.next_frame(&mut brut).unwrap();
        assert_eq!(image.color_type, png::ColorType::Rgba);
        brut[..image.buffer_size()].chunks_exact(4).map(|p| p.try_into().unwrap()).collect()
    }

    #[test]
    fn decoupe_le_visage_et_pose_le_casque() {
        // Casque transparent : on voit la peau.
        assert!(pixels(&visage(&skin(64, true, VIDE)).unwrap()).iter().all(|p| *p == ROUGE));
        // Casque bleu opaque sur un skin récent : on voit le casque.
        assert!(pixels(&visage(&skin(64, true, BLEU)).unwrap()).iter().all(|p| *p == BLEU));
        // Casque à moitié transparent : un mélange des deux.
        let melange = pixels(&visage(&skin(64, true, [20, 40, 220, 128])).unwrap())[0];
        assert!(melange[0] > 100 && melange[0] < 120 && melange[2] > 120 && melange[3] == 255, "{melange:?}");
    }

    #[test]
    fn ignore_le_casque_plein_des_anciens_skins() {
        // Ancien skin sans transparence : la zone du casque est noire, mais ce n'est pas un casque.
        assert!(pixels(&visage(&skin(32, false, NOIR)).unwrap()).iter().all(|p| *p == ROUGE));
        assert!(pixels(&visage(&skin(32, true, NOIR)).unwrap()).iter().all(|p| *p == ROUGE));
    }

    #[test]
    fn refuse_ce_qui_n_est_pas_un_skin() {
        assert!(visage(b"pas une image").is_err());
        let mut petit = Vec::new();
        let mut encodeur = png::Encoder::new(&mut petit, 16, 16);
        encodeur.set_color(png::ColorType::Rgba);
        encodeur.write_header().unwrap().write_image_data(&[0; 16 * 16 * 4]).unwrap();
        assert!(visage(&petit).is_err());
    }

    #[test]
    fn reconnait_les_pseudos_java() {
        assert!(pseudo_java("KOTORO_YT"));
        assert!(pseudo_java("Notch"));
        assert!(!pseudo_java(".Lea")); // Bedrock
        assert!(!pseudo_java(""));
        assert!(!pseudo_java("un_pseudo_beaucoup_trop_long"));
        assert!(!pseudo_java("../secret"));
    }

    /// La réponse de sessionserver pour un skin à cette adresse.
    fn profil(adresse_du_skin: &str) -> Vec<u8> {
        let textures = format!(r#"{{"timestamp":1,"profileId":"069a79f444e94726a5befca90e38aaf5","profileName":"Notch","textures":{{"SKIN":{{"url":"{adresse_du_skin}"}}}}}}"#);
        let valeur = base64::engine::general_purpose::STANDARD.encode(textures);
        format!(r#"{{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{{"name":"textures","value":"{valeur}"}}]}}"#).into_bytes()
    }

    #[test]
    fn lit_les_reponses_de_mojang() {
        assert_eq!(identifiant(br#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#).as_deref(), Some("069a79f444e94726a5befca90e38aaf5"));
        assert_eq!(identifiant(br#"{"path":"/users/profiles/minecraft/x","errorMessage":"Couldn't find any profile with name x"}"#), None);
        assert_eq!(identifiant(br#"{"id":"../../etc"}"#), None);
        // L'adresse du skin passe toujours en HTTPS.
        assert_eq!(adresse_skin(&profil("http://textures.minecraft.net/texture/292009a4925b58f02c77dadc3ecef07ea4c7472f64e0fdc32ce5522489362680")).as_deref(), Some("https://textures.minecraft.net/texture/292009a4925b58f02c77dadc3ecef07ea4c7472f64e0fdc32ce5522489362680"));
        // Un autre serveur que celui de Mojang est refusé.
        assert_eq!(adresse_skin(&profil("https://exemple.com/texture/abc")), None);
        assert_eq!(adresse_skin(&profil("http://textures.minecraft.net/texture/../x")), None);
        // Skin par défaut : pas de « SKIN » dans la réponse.
        let sans = base64::engine::general_purpose::STANDARD.encode(r#"{"textures":{}}"#);
        assert_eq!(adresse_skin(format!(r#"{{"id":"x","properties":[{{"name":"textures","value":"{sans}"}}]}}"#).as_bytes()), None);
    }

    #[test]
    fn enchaine_les_trois_demandes() {
        let vues = RefCell::new(Vec::new());
        let faux_mojang = |adresse: &str| -> Result<Option<Vec<u8>>, String> {
            vues.borrow_mut().push(adresse.to_string());
            Ok(match adresse {
                "https://api.minecraftservices.com/minecraft/profile/lookup/name/Notch" => Some(br#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#.to_vec()),
                "https://sessionserver.mojang.com/session/minecraft/profile/069a79f444e94726a5befca90e38aaf5" => Some(profil("http://textures.minecraft.net/texture/abc123")),
                "https://textures.minecraft.net/texture/abc123" => Some(skin(64, true, VIDE)),
                _ => None,
            })
        };
        let tete = chercher(&faux_mojang, "Notch", None).unwrap().unwrap();
        assert!(pixels(&tete).iter().all(|p| *p == ROUGE));
        assert_eq!(vues.borrow().len(), 3);
        // Identifiant déjà lu dans la console : on saute la première demande.
        vues.borrow_mut().clear();
        assert!(chercher(&faux_mojang, "Notch", Some("069a79f444e94726a5befca90e38aaf5")).unwrap().is_some());
        assert_eq!(vues.borrow().len(), 2);
        // Serveur hors ligne (identifiant version 3) : aucune demande.
        vues.borrow_mut().clear();
        assert_eq!(chercher(&faux_mojang, "Notch", Some("069a79f444e93726a5befca90e38aaf5")).unwrap(), None);
        assert!(vues.borrow().is_empty());
        // Pseudo inconnu : une seule demande, pas de tête.
        vues.borrow_mut().clear();
        assert_eq!(chercher(&faux_mojang, "Inconnu_42", None).unwrap(), None);
        assert_eq!(vues.borrow().len(), 1);
        // Joueur Bedrock : aucune demande.
        vues.borrow_mut().clear();
        assert_eq!(chercher(&faux_mojang, ".Lea", None).unwrap(), None);
        assert!(vues.borrow().is_empty());
        // Réseau coupé : une erreur, pas un « inconnu ».
        assert!(chercher(&|_| Err("pas d’Internet".into()), "Notch", None).is_err());
    }

    #[test]
    fn reessaie_et_change_d_adresse_quand_mojang_refuse() {
        // La nouvelle adresse refuse, l'ancienne refuse une fois (le 403 au hasard), puis tout marche.
        let refus = RefCell::new(0);
        let capricieux = |adresse: &str| -> Result<Option<Vec<u8>>, String> {
            if adresse.starts_with("https://api.minecraftservices.com/") {
                return Err("http status: 403".into());
            }
            if adresse.starts_with("https://api.mojang.com/") && *refus.borrow() == 0 {
                *refus.borrow_mut() += 1;
                return Err("http status: 403".into());
            }
            Ok(match adresse {
                "https://api.mojang.com/users/profiles/minecraft/Notch" => Some(br#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#.to_vec()),
                "https://sessionserver.mojang.com/session/minecraft/profile/069a79f444e94726a5befca90e38aaf5" => Some(profil("http://textures.minecraft.net/texture/abc123")),
                "https://textures.minecraft.net/texture/abc123" => Some(skin(64, true, VIDE)),
                _ => None,
            })
        };
        assert!(chercher(&capricieux, "Notch", None).unwrap().is_some());
    }

    #[test]
    fn garde_les_tetes_sur_le_disque() {
        let dossier = std::env::temp_dir().join(format!("bhost-tetes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dossier);
        // Première fois : on demande à Mojang et on garde le résultat.
        assert_eq!(avec_cache(&dossier, "Notch", || Ok(Some(vec![1, 2, 3]))), Some(vec![1, 2, 3]));
        assert!(dossier.join("notch.png").exists());
        // Ensuite : on ne redemande pas (même avec des majuscules différentes).
        assert_eq!(avec_cache(&dossier, "NOTCH", || panic!("ne doit pas redemander")), Some(vec![1, 2, 3]));
        // Tête trop vieille et réseau coupé : on garde l'ancienne.
        let vieux = std::time::SystemTime::now() - Duration::from_secs(3 * 24 * 3600);
        fs::File::options().write(true).open(dossier.join("notch.png")).unwrap().set_modified(vieux).unwrap();
        assert_eq!(avec_cache(&dossier, "Notch", || Err("pas d’Internet".into())), Some(vec![1, 2, 3]));
        // Tête trop vieille et réseau présent : on la remplace.
        assert_eq!(avec_cache(&dossier, "Notch", || Ok(Some(vec![9]))), Some(vec![9]));
        // Le joueur n'a plus de skin : on oublie sa tête.
        fs::File::options().write(true).open(dossier.join("notch.png")).unwrap().set_modified(vieux).unwrap();
        assert_eq!(avec_cache(&dossier, "Notch", || Ok(None)), None);
        assert!(!dossier.join("notch.png").exists());
        // Un nom bizarre ne sort jamais du dossier.
        assert_eq!(avec_cache(&dossier, "../secret", || panic!("ne doit pas chercher")), None);
        let _ = fs::remove_dir_all(&dossier);
    }

    #[test]
    fn donne_une_adresse_affichable() {
        assert_eq!(en_adresse(&[1, 2, 3]), "data:image/png;base64,AQID");
    }
}
