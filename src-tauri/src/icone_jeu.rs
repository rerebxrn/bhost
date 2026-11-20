// L'icône du serveur dans Minecraft (server-icon.png, 64 × 64) : par défaut, la même que dans BHOST.
// Les 34 icônes sont dessinées en PNG par docs/logo/icones.py (sortie/jeu) et rangées dans le programme.
// Si l'utilisateur a mis sa propre image (Fichiers › Icône du serveur), BHOST n'y touche plus.

use std::fs;
use std::path::Path;

/// Les 34 icônes, en PNG 64 × 64, dans l'ordre de `serveurs::ICONES`.
const PNGS: [(&str, &[u8]); 34] = [
    ("graphite", include_bytes!("../icones-jeu/graphite.png")),
    ("bleu", include_bytes!("../icones-jeu/bleu.png")),
    ("indigo", include_bytes!("../icones-jeu/indigo.png")),
    ("violet", include_bytes!("../icones-jeu/violet.png")),
    ("rose", include_bytes!("../icones-jeu/rose.png")),
    ("rouge", include_bytes!("../icones-jeu/rouge.png")),
    ("orange", include_bytes!("../icones-jeu/orange.png")),
    ("jaune", include_bytes!("../icones-jeu/jaune.png")),
    ("vert", include_bytes!("../icones-jeu/vert.png")),
    ("menthe", include_bytes!("../icones-jeu/menthe.png")),
    ("serveur-acier", include_bytes!("../icones-jeu/serveur-acier.png")),
    ("serveur-dore", include_bytes!("../icones-jeu/serveur-dore.png")),
    ("serveur-titane", include_bytes!("../icones-jeu/serveur-titane.png")),
    ("serveur-noir", include_bytes!("../icones-jeu/serveur-noir.png")),
    ("serveur-orrose", include_bytes!("../icones-jeu/serveur-orrose.png")),
    ("cadeau-rouge", include_bytes!("../icones-jeu/cadeau-rouge.png")),
    ("cadeau-bleu", include_bytes!("../icones-jeu/cadeau-bleu.png")),
    ("cadeau-vert", include_bytes!("../icones-jeu/cadeau-vert.png")),
    ("gateau", include_bytes!("../icones-jeu/gateau.png")),
    ("gateau-chocolat", include_bytes!("../icones-jeu/gateau-chocolat.png")),
    ("fromage", include_bytes!("../icones-jeu/fromage.png")),
    ("pain", include_bytes!("../icones-jeu/pain.png")),
    ("guimauve", include_bytes!("../icones-jeu/guimauve.png")),
    ("bento", include_bytes!("../icones-jeu/bento.png")),
    ("carton", include_bytes!("../icones-jeu/carton.png")),
    ("de", include_bytes!("../icones-jeu/de.png")),
    ("camera", include_bytes!("../icones-jeu/camera.png")),
    ("aquarium", include_bytes!("../icones-jeu/aquarium.png")),
    ("terminal", include_bytes!("../icones-jeu/terminal.png")),
    ("damier", include_bytes!("../icones-jeu/damier.png")),
    ("caisse", include_bytes!("../icones-jeu/caisse.png")),
    ("herbe", include_bytes!("../icones-jeu/herbe.png")),
    ("hiver", include_bytes!("../icones-jeu/hiver.png")),
    ("grenouille", include_bytes!("../icones-jeu/grenouille.png")),
];

/// L'icône par défaut selon le logiciel, quand l'utilisateur n'en a pas choisi (comme `IconeServeur.tsx`).
pub fn par_defaut(logiciel: &str) -> &'static str {
    match logiciel {
        "Vanilla" => "vert",
        "Paper" => "bleu",
        "Purpur" => "violet",
        "Spigot" | "NeoForge" => "orange",
        "Fabric" => "jaune",
        "Youer" | "Arclight" | "Mohist" => "indigo",
        _ => "graphite",
    }
}

/// Les icônes posées par les versions d'avant de BHOST (empreintes SHA-1) : à remplacer aussi.
/// Les 34 premières : rendus du 18/11 au matin ; les 34 suivantes : rendus SVG « sans joints » (qui en avaient encore) ;
/// les 34 dernières : les PNG du 18/11 avec des métadonnées en plus (même dessin).
const ANCIENNES: [&str; 102] = [
    "84692cafb2853ebede0999a14cf65c6022c4b26d",
    "501c1fb48ef79516fca73489d11af5da06891a93",
    "a2855654029d82ac9a857e9119ab8d0b6dfbe26e",
    "737e1ba43c22dae01ed5817c6f36d9450bc08daa",
    "e356d6c818b364032c905bf9b48a5a08593ee555",
    "3ddf79e2e3eac9fd41b61f3e47206e94da588803",
    "04b1cf645268e699bdddf9b9d8554379a8580ea8",
    "4f8efbbc7a22d00e5c136967dce63eea25c8b939",
    "4126c6f4202159a0b4f806d0ae744eb0a96d5526",
    "4b73e868000a065e68e81455eb5ac7ff57719519",
    "17a626b1698e609a1a1ccbf45a6386ac000ff2cd",
    "04b13ec3f92e084b2aba613c6c711f7232a9fc6a",
    "237ece3d86e277162a726d5cb4e1ea710cbd165f",
    "aceb22a25ed6ee503234e0f7cd0b285dc0f4bbd4",
    "afda68c382d01f9749c72799148a03de1a5bcc9e",
    "528753f2717b6411ae8ecddf565036bac014e420",
    "cb982a83cc073bf1d0f7ec07857010d77295eb76",
    "1e9d8d552eae36cd37017feee4e4271681f24fd7",
    "2c37e1d64c1d2cb58ad33ddaf8ab252138070ef9",
    "ad66b09f9ab1135c07770d14dd411919a693c48d",
    "08458988b895df7f1ec2a84512d53ee6c3effc01",
    "69b52c35b1450c9e68f624fe4f9dbf1844ca6813",
    "d5962562d9170093dee3afe3a9fbaaace0115ae5",
    "1484812664bfcd11144bba825ec04f10110d1a2b",
    "e3bb9d899216d05ad637e965d044c335f5acfaa9",
    "a9ecbe0e5fa0ddb0e307354af3a8bb5a27f784d2",
    "139088546b9d616271bd87585bfa348bafae6296",
    "ae60d5f8b0fc37fc8bd65dc070e81c109184888d",
    "a5e8f003b3a7f8832dbde74ced4e09a947ad851d",
    "8b9751bbf04de8a9aced9ac91749cc025b239402",
    "8df01c31a6b7705f03c881927e679cb3fa5ffe34",
    "0a6665e268ffd937dd3e1257f427fc31d4b314a4",
    "ab025e9b72ac9b148d4f0945bbcab2396e62c168",
    "1b0bb338fc7e6cce499b6ff939c77b7e331d1138",
    "1c238a988e38192907a8b3ed2620543db2455d1b",
    "09b7e83318bbe7f68176c40fb07bb49c29b246ba",
    "2be5d037acd492afedc690c88486294c296e4738",
    "793b848db27161a43ba59501912040d5b684f71d",
    "53938b234bc02d8026e564bbe93b45302e731d87",
    "3f6b719727414dcb1173fcef34a9f28a18c92815",
    "37eb0671db18ac62a6c2101dba8cec3da507baeb",
    "7e94ad45f3952d8cc53170875d5139dafce61b9b",
    "ad68697d1b9d5e0144a9285512d3d63d4650ede3",
    "fc8efb7cd6f26198ec69197ec0d5033aa480b878",
    "0cdb3a06085117ece09777ea5a8bad6be9889029",
    "fe1162fe36b8f59e3999caa09c7b0ca11c33cb68",
    "bdb8ca49246347cc4d753acf367d8a66f257d9fa",
    "cee8509db3f14e9ac5830dae418084bad5282f6c",
    "fd2500d9a070a387b5c99f849e6f6e744ab1babb",
    "c971172004eb7edb01eedec66aa3648d44ef4b72",
    "a5c1fb79ad6adf9ae91044e680e141163f45f814",
    "08eae3709a00275298456f704941e93197e0d82c",
    "dcecb2dc43eeb17d6fe6cc9748c4fecea0fba7a7",
    "cb60f4473893c646c570505fe4a7d11861bb71bf",
    "1726a5875db15b6b94aa38f073b9d6163db7a03d",
    "f3732fe121b4f005632da7641c5eb5afe74b8ac5",
    "eb82d2729a2000154272ff4f3c4e215b2d457727",
    "8f68a33f3d6e79a2b46afcf19c4c1a73562ef5e1",
    "e4a27005c2f1bdd4d0c3ac48a420cd558bf2cf64",
    "af90b85f312b72c9b6784959738759bb3051f2f7",
    "eadbca5666acff7ac9efda4c168ca50988a5fd75",
    "4d26614543436ba0e7171b4858d63deb844a4a3c",
    "276937a662332af08a15656c63a1eeb446778f3a",
    "6d54e44c053561d8ea5edd2557ed8ee1346478d6",
    "5355c01b8552ce6d54ba6b37e7dfc641be0cf523",
    "5539d73a95a1dcba99be66fbb6fd48ac70d5ab12",
    "645ded30ae7cc99ec354edc990cfafcbcb8743f5",
    "10dd6d2ec0ad5a0f4ecb2001be4041365c8f635c",
    "641e23d8b7937b47376a721c460838689de7b9e4",
    "437f2bb50969267b902eebbe52d217fda20495a8",
    "e922ec65681da5cdd5571d091c0a774994660f6e",
    "2ba437f1aded7d4a5d49bd6e1d67152d5c128011",
    "1539f3d5a533ab4e6bbe8790307dafb9f0aab6aa",
    "1f5d53842e23f4a1849a171b89be4ff08477d16e",
    "f5c1bbe7f37005c03147b575c39aed6bef98d37f",
    "1ba695f2206ea72e07c0f862eee5c7e138b03548",
    "4a27d485a428ee6e23ae3a471d5edf85bf6002e2",
    "82215b93b78f717b53698b6a1e2143981fdc5b8a",
    "a97a0f64bb635a5616148e81b564998d9ecf7a18",
    "5d2f7c2371816c161e268e1d205f0bd9246d688f",
    "e51e7025a4ecbc8ee1e5bd575a1026312b28153c",
    "4ecf933fcedbf28b6ffdcc8ba01740d6c387051b",
    "75ee2e5948a1f35ac45243e31a10167e6604138e",
    "2dc7727c646f5e7317e361c2fdeb284e42509e59",
    "2745ae78984417a24f7e7a33fd38f5b478657645",
    "1823dab395821dd51db3c5216a3015d46fdbaeb1",
    "a2e9873b879558ec3728a8e86e2c5a8c4734ac76",
    "53b961c29609c5afa59b01ed68e40e09824e8514",
    "94c0ac74cc0aa9d4d160a683cab22c1e54cf2342",
    "fde936576690e881c4e7e50b0dd78c437b486b67",
    "bc00c5e5bd5444d10b79b40e73efd5a31e4049c4",
    "1752080bf2facf9e3ee524963f77e862bb49db73",
    "43133a30879085776bb2baf565679f1b27575bc7",
    "57b8c648716c813b05e2d65746cafce95fa5363d",
    "9e382aa389bf3541dfa92d5b05f33a6dc290ddba",
    "0c426607dacb08791eb716e3e33e3a93ace20b6d",
    "5e8e2fc9c419636165271027460aba2b72a0ff63",
    "23c0e35403b304f9c799b4d7ad6d1802b95a20af",
    "0b9bad21e1a2b525ad2d114ab4321f7aedddcd21",
    "4dc52e0eef3f071db514b0aec9c86a313a24ae81",
    "c2d73bbbe1cfdc2231033705532103ae2b2c1c82",
    "b06e229eb0fa089726ab362b7b8ca0585c7bac49",
];

fn png(cle: &str) -> Option<&'static [u8]> {
    PNGS.iter().find(|(c, _)| *c == cle).map(|(_, o)| *o)
}

/// Cette image a été posée par BHOST (c'est une de ses 34 icônes) : on peut la remplacer.
fn de_bhost(octets: &[u8]) -> bool {
    use sha1::{Digest, Sha1};
    if PNGS.iter().any(|(_, o)| *o == octets) {
        return true;
    }
    let empreinte: String = Sha1::digest(octets).iter().map(|o| format!("{o:02x}")).collect();
    ANCIENNES.contains(&empreinte.as_str())
}

/// L'utilisateur a mis sa propre image comme icône du serveur.
pub fn perso(dossier: &Path) -> bool {
    fs::read(dossier.join("server-icon.png")).is_ok_and(|o| !de_bhost(&o))
}

/// Met l'icône BHOST `cle` comme icône du serveur dans Minecraft, sauf si l'utilisateur a mis la sienne
/// (`forcer` : la remplacer quand même). Renvoie vrai si le fichier a été écrit.
pub fn poser(dossier: &Path, cle: &str, forcer: bool) -> Result<bool, String> {
    let image = png(cle).ok_or("Icône inconnue.")?;
    let chemin = dossier.join("server-icon.png");
    match fs::read(&chemin) {
        Ok(o) if o == image => return Ok(false),
        Ok(o) if !forcer && !de_bhost(&o) => return Ok(false),
        _ => {}
    }
    crate::garde::ecrire_atomique(&chemin, image).map_err(|e| format!("Impossible d'écrire l'icône. {e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_icones_sont_toutes_la_et_en_64() {
        let cles: Vec<&str> = PNGS.iter().map(|(c, _)| *c).collect();
        assert_eq!(cles, crate::serveurs::ICONES);
        for (c, o) in PNGS {
            let lecteur = png::Decoder::new(std::io::Cursor::new(o)).read_info().unwrap();
            let i = lecteur.info();
            assert_eq!((i.width, i.height), (64, 64), "{c}");
        }
        for l in ["Vanilla", "Paper", "Purpur", "Spigot", "Fabric", "Forge", "NeoForge", "Mohist", "?"] {
            assert!(png(par_defaut(l)).is_some());
        }
    }

    #[test]
    fn garde_l_image_de_l_utilisateur() {
        let d = std::env::temp_dir().join(format!("bhost-icone-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        // Pas d'icône : BHOST pose la sienne, puis la change quand on change d'icône.
        assert!(poser(&d, "herbe", false).unwrap());
        assert!(!poser(&d, "herbe", false).unwrap());
        assert!(poser(&d, "gateau", false).unwrap());
        assert_eq!(fs::read(d.join("server-icon.png")).unwrap(), png("gateau").unwrap());
        assert!(!perso(&d));
        // Une image à soi : BHOST n'y touche plus, sauf si on le demande.
        fs::write(d.join("server-icon.png"), b"mon image").unwrap();
        assert!(perso(&d));
        assert!(!poser(&d, "herbe", false).unwrap());
        assert_eq!(fs::read(d.join("server-icon.png")).unwrap(), b"mon image");
        assert!(poser(&d, "herbe", true).unwrap());
        assert!(!perso(&d));
        let _ = fs::remove_dir_all(&d);
    }
}
