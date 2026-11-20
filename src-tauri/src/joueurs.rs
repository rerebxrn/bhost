// Savoir qui est connecté, en lisant la console du serveur.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

#[derive(Debug, PartialEq)]
pub enum Passage {
    Arrivee(String),
    Depart(String),
}

/// Un pseudo Minecraft : lettres, chiffres, tiret bas (et un point devant pour Bedrock).
fn pseudo(mot: &str) -> bool {
    (1..=17).contains(&mot.len()) && mot.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Reconnaît une arrivée ou un départ dans une ligne de console.
/// On se fie d'abord aux lignes techniques du serveur, toujours écrites :
///   « Alex[/127.0.0.1:59576] logged in with entity id 415 at (…) » et « Alex lost connection: … ».
/// Les messages « joined the game » / « left the game » servent aussi, mais un plugin peut les cacher.
/// Un message du chat ne peut pas tromper BHOST : il commence par « <pseudo> ».
pub fn lire(ligne: &str) -> Option<Passage> {
    // On saute l'en-tête « [18:04:02 INFO]: ».
    let texte = ligne.split_once("]: ").map(|(_, t)| t).unwrap_or(ligne).trim();
    let arrivee = |nom: &str| pseudo(nom).then(|| Passage::Arrivee(nom.to_string()));
    let depart = |nom: &str| pseudo(nom).then(|| Passage::Depart(nom.to_string()));

    if texte.contains("] logged in with entity id ") {
        return texte.split_once("[/").and_then(|(nom, _)| arrivee(nom));
    }
    if let Some((nom, _)) = texte.split_once(" lost connection: ") {
        return depart(nom);
    }
    if let Some(nom) = texte.strip_suffix(" joined the game") {
        return arrivee(nom);
    }
    if let Some(nom) = texte.strip_suffix(" left the game").or(texte.strip_suffix(" left the game.")) {
        return depart(nom);
    }
    None
}

/// Lit l'identifiant du compte, que le serveur écrit à chaque connexion :
///   « UUID of player Alex is 069a79f4-44e9-4726-a5be-fca90e38aaf5 ».
/// Renvoie (pseudo, identifiant sans tirets, en minuscules).
pub fn lire_identifiant(ligne: &str) -> Option<(String, String)> {
    let texte = ligne.split_once("]: ").map(|(_, t)| t).unwrap_or(ligne).trim();
    let (nom, uuid) = texte.strip_prefix("UUID of player ")?.split_once(" is ")?;
    let uuid: String = uuid.trim().chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
    (pseudo(nom) && uuid.len() == 32 && uuid.chars().all(|c| c.is_ascii_hexdigit())).then(|| (nom.to_string(), uuid))
}

/// Met la liste des connectés à jour. Renvoie vrai si elle a changé.
pub fn noter(liste: &mut Vec<String>, passage: Passage) -> bool {
    match passage {
        Passage::Arrivee(nom) if !liste.contains(&nom) => {
            liste.push(nom);
            true
        }
        Passage::Depart(nom) => {
            let avant = liste.len();
            liste.retain(|j| *j != nom);
            liste.len() != avant
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_les_arrivees_et_les_departs() {
        assert_eq!(lire("[18:04:02 INFO]: Alex joined the game"), Some(Passage::Arrivee("Alex".into())));
        assert_eq!(lire("[18:04:02] [Server thread/INFO]: Noe_59 left the game"), Some(Passage::Depart("Noe_59".into())));
        assert_eq!(lire("[18:04:02 INFO]: .Lea joined the game"), Some(Passage::Arrivee(".Lea".into())));
    }

    #[test]
    fn reconnait_les_lignes_techniques_quand_un_plugin_cache_les_messages() {
        // Les vraies lignes du serveur Spigot 1.8.8 de Thomas.
        assert_eq!(
            lire("[12:22:03 INFO]: KOTORO_YT[/127.0.0.1:59576] logged in with entity id 415 at ([world]64.5, 64.0, 252.5)"),
            Some(Passage::Arrivee("KOTORO_YT".into()))
        );
        assert_eq!(lire("[12:22:03 INFO]: UUID of player KOTORO_YT is b6c08110-bff1-4f6b-bbdc-55d90b74f47d"), None);
        assert_eq!(
            lire("[12:30:10 INFO]: KOTORO_YT lost connection: TextComponent{text='Disconnected', siblings=[], style=Style{}}"),
            Some(Passage::Depart("KOTORO_YT".into()))
        );
        assert_eq!(lire("[12:30:10] [Server thread/INFO]: KOTORO_YT lost connection: Disconnected"), Some(Passage::Depart("KOTORO_YT".into())));
        // Une connexion ratée avant l'entrée en jeu : pas un joueur.
        assert_eq!(lire("[12:30:10 INFO]: com.mojang.authlib.GameProfile@1a2b[id=<null>,name=X] (/127.0.0.1:5) lost connection: Disconnected"), None);
        assert_eq!(lire("[12:30:10 INFO]: /127.0.0.1:59580 lost connection: Timed out"), None);
    }

    #[test]
    fn ne_se_laisse_pas_tromper_par_le_chat() {
        assert_eq!(lire("[18:05:17 INFO]: <Alex> Bob[/1.2.3.4:5] logged in with entity id 1 at (x)"), None);
        assert_eq!(lire("[18:05:17 INFO]: <Alex> Bob lost connection: lol"), None);
        assert_eq!(lire("[18:05:17 INFO]: <Alex> ]: Bob joined the game"), None);
        assert_eq!(lire("[18:05:17 INFO]: <Alex> Camille joined the game"), None);
        assert_eq!(lire("[18:05:17 INFO]: [Server] tout le monde left the game"), None);
        assert_eq!(lire("[18:05:17 INFO]: Done (2,1s)! For help, type \"help\""), None);
    }

    #[test]
    fn lit_l_identifiant_du_compte() {
        assert_eq!(
            lire_identifiant("[12:22:03 INFO]: UUID of player KOTORO_YT is b6c08110-bff1-4f6b-bbdc-55d90b74f47d"),
            Some(("KOTORO_YT".into(), "b6c08110bff14f6bbbdc55d90b74f47d".into()))
        );
        assert_eq!(
            lire_identifiant("[12:22:03] [User Authenticator #1/INFO]: UUID of player Notch is 069A79F4-44E9-4726-A5BE-FCA90E38AAF5"),
            Some(("Notch".into(), "069a79f444e94726a5befca90e38aaf5".into()))
        );
        assert_eq!(lire_identifiant("[12:22:03 INFO]: <Alex> UUID of player Bob is 069a79f444e94726a5befca90e38aaf5"), None);
        assert_eq!(lire_identifiant("[12:22:03 INFO]: UUID of player Bob is pas-un-uuid"), None);
    }

    #[test]
    fn tient_la_liste_a_jour() {
        let mut liste = Vec::new();
        assert!(noter(&mut liste, Passage::Arrivee("Alex".into())));
        assert!(!noter(&mut liste, Passage::Arrivee("Alex".into())));
        assert!(noter(&mut liste, Passage::Arrivee("Lea".into())));
        assert!(noter(&mut liste, Passage::Depart("Alex".into())));
        assert!(!noter(&mut liste, Passage::Depart("Alex".into())));
        assert_eq!(liste, vec!["Lea".to_string()]);
    }
}
