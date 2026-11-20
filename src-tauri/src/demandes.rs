// Onglet Joueurs › Demandes : les joueurs refusés par la liste blanche.
// Quand quelqu'un essaie d'entrer sans être sur la liste blanche, le serveur écrit une ligne dans sa console.
// BHOST la repère et garde une « demande » : Accepter l'ajoute à la liste blanche, Refuser l'oublie.
// Les demandes sont gardées dans %APPDATA%\BHOST\demandes.json (elles restent si on ferme BHOST).
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Au plus 30 demandes en attente par serveur (les plus vieilles partent).
const ATTENTE_MAX: usize = 30;
/// Au plus 200 joueurs refusés retenus par serveur.
const REFUSES_MAX: usize = 200;
/// Une même tentative écrit souvent deux lignes (« Disconnecting… » puis « … lost connection… ») : on n'en compte qu'une.
const MEME_ESSAI_MS: u64 = 10_000;

/// Un joueur qui veut entrer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Demande {
    pub nom: String,
    /// L'identifiant du compte (32 chiffres hexadécimaux, sans tirets), s'il est écrit dans la ligne.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// Dernier essai (millisecondes depuis 1970).
    pub quand: u64,
    /// Combien de fois il a essayé.
    pub fois: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DuServeur {
    #[serde(default)]
    attente: Vec<Demande>,
    /// Pseudos refusés (en minuscules) : leurs nouveaux essais ne refont pas de demande.
    #[serde(default)]
    refuses: Vec<String>,
}

/// Les demandes de tous les serveurs.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Registre {
    #[serde(default)]
    serveurs: BTreeMap<String, DuServeur>,
}

/// Ce que `noter` a fait.
#[derive(Debug, PartialEq)]
pub enum Note {
    /// Une nouvelle demande : BHOST la montre tout de suite.
    Nouvelle,
    /// Une demande déjà là, qui revient.
    Encore,
    /// Rien à faire (joueur refusé, ou même essai).
    Rien,
}

/// Un pseudo Minecraft : lettres, chiffres, tiret bas (et un point devant pour Bedrock).
fn pseudo(mot: &str) -> bool {
    let n = mot.strip_prefix('.').unwrap_or(mot);
    (1..=16).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Le mot qui suit `cle` dans `texte`, jusqu'à une virgule, un crochet ou un espace.
fn apres<'a>(texte: &'a str, cle: &str) -> Option<&'a str> {
    let reste = &texte[texte.find(cle)? + cle.len()..];
    let fin = reste.find([',', ']', '}', ' ', ')']).unwrap_or(reste.len());
    Some(&reste[..fin])
}

/// Repère un joueur refusé par la liste blanche. Renvoie (pseudo, identifiant sans tirets s'il est écrit).
/// Les formes connues :
///   1.8 à 1.20 : « Disconnecting com.mojang.authlib.GameProfile@1a2b[id=…,name=Hugo,…] (/1.2.3.4:5): You are not whitelisted… »
///   1.21 : « Disconnecting GameProfile[id=…, name=Hugo, …] (/1.2.3.4:5): You are not white-listed on this server! »
///   Paper : « Hugo (/1.2.3.4:5) lost connection: You are not whitelisted on this server! »
pub fn lire_refus(ligne: &str) -> Option<(String, Option<String>)> {
    let texte = ligne.split_once("]: ").map(|(_, t)| t).unwrap_or(ligne).trim();
    // Un message du chat (« <Alex> … ») ne compte pas.
    if texte.starts_with('<') {
        return None;
    }
    let bas = texte.to_ascii_lowercase();
    if !(bas.contains("whitelist") || bas.contains("white-list") || bas.contains("white list")) {
        return None;
    }
    if !(texte.starts_with("Disconnecting ") || texte.contains(" lost connection: ")) {
        return None;
    }
    let nom = match apres(texte, "name=") {
        Some(n) => n,
        None => {
            // « Disconnecting Hugo (/…) » ou « Hugo (/…) lost connection »
            let debut = texte.strip_prefix("Disconnecting ").unwrap_or(texte);
            debut.split([' ', '[']).next()?
        }
    };
    if !pseudo(nom) {
        return None;
    }
    let uuid = apres(texte, "id=")
        .map(|u| u.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase())
        .filter(|u| u.len() == 32 && u.chars().all(|c| c.is_ascii_hexdigit()));
    Some((nom.to_string(), uuid))
}

impl Registre {
    pub fn charger(fichier: &Path) -> Registre {
        std::fs::read_to_string(fichier).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn enregistrer(&self, fichier: &Path) {
        if let Ok(texte) = serde_json::to_string(self) {
            let _ = std::fs::write(fichier, texte);
        }
    }

    /// Les demandes en attente d'un serveur, la plus récente en premier.
    pub fn liste(&self, id: &str) -> Vec<Demande> {
        let mut l = self.serveurs.get(id).map(|s| s.attente.clone()).unwrap_or_default();
        l.sort_by(|a, b| b.quand.cmp(&a.quand));
        l
    }

    /// Un joueur vient d'être refusé.
    pub fn noter(&mut self, id: &str, nom: &str, uuid: Option<String>, maintenant: u64) -> Note {
        let s = self.serveurs.entry(id.to_string()).or_default();
        let bas = nom.to_ascii_lowercase();
        if s.refuses.contains(&bas) {
            return Note::Rien;
        }
        if let Some(d) = s.attente.iter_mut().find(|d| d.nom.eq_ignore_ascii_case(nom)) {
            if maintenant.saturating_sub(d.quand) < MEME_ESSAI_MS {
                return Note::Rien;
            }
            d.quand = maintenant;
            d.fois = d.fois.saturating_add(1);
            if uuid.is_some() {
                d.uuid = uuid;
            }
            return Note::Encore;
        }
        s.attente.push(Demande { nom: nom.to_string(), uuid, quand: maintenant, fois: 1 });
        if s.attente.len() > ATTENTE_MAX {
            // On enlève la plus vieille.
            if let Some(i) = s.attente.iter().enumerate().min_by_key(|(_, d)| d.quand).map(|(i, _)| i) {
                s.attente.remove(i);
            }
        }
        Note::Nouvelle
    }

    /// La demande d'un joueur (pour son identifiant), si elle existe.
    pub fn trouver(&self, id: &str, nom: &str) -> Option<&Demande> {
        self.serveurs.get(id)?.attente.iter().find(|d| d.nom.eq_ignore_ascii_case(nom))
    }

    /// Accepter (ou ajout à la liste blanche) : la demande disparaît, et le joueur n'est plus refusé.
    pub fn accepter(&mut self, id: &str, nom: &str) -> bool {
        let Some(s) = self.serveurs.get_mut(id) else { return false };
        let bas = nom.to_ascii_lowercase();
        let avant = s.attente.len() + s.refuses.len();
        s.attente.retain(|d| !d.nom.eq_ignore_ascii_case(nom));
        s.refuses.retain(|r| *r != bas);
        avant != s.attente.len() + s.refuses.len()
    }

    /// Refuser : la demande disparaît, et ses prochains essais ne reviennent pas.
    pub fn refuser(&mut self, id: &str, nom: &str) {
        let s = self.serveurs.entry(id.to_string()).or_default();
        s.attente.retain(|d| !d.nom.eq_ignore_ascii_case(nom));
        let bas = nom.to_ascii_lowercase();
        if !s.refuses.contains(&bas) {
            s.refuses.push(bas);
            if s.refuses.len() > REFUSES_MAX {
                s.refuses.remove(0);
            }
        }
    }

    /// Enlève les demandes des joueurs déjà sur la liste blanche (ajoutés ailleurs). Vrai si ça change.
    pub fn nettoyer(&mut self, id: &str, blancs: &[String]) -> bool {
        let Some(s) = self.serveurs.get_mut(id) else { return false };
        let avant = s.attente.len();
        s.attente.retain(|d| !blancs.iter().any(|b| b.eq_ignore_ascii_case(&d.nom)));
        avant != s.attente.len()
    }

    /// Un serveur retiré de BHOST : on oublie ses demandes.
    pub fn oublier(&mut self, id: &str) {
        self.serveurs.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_les_refus() {
        // Spigot 1.8.8
        let l = "[10:12:03 INFO]: Disconnecting com.mojang.authlib.GameProfile@5e2d1e4f[id=069a79f4-44e9-4726-a5be-fca90e38aaf5,name=Hugo,properties={textures=[]},legacy=false] (/192.168.1.20:52114): You are not whitelisted on this server!";
        assert_eq!(lire_refus(l), Some(("Hugo".into(), Some("069a79f444e94726a5befca90e38aaf5".into()))));
        // Vanilla 1.21
        let l = "[10:12:03] [Server thread/INFO]: Disconnecting GameProfile[id=069a79f4-44e9-4726-a5be-fca90e38aaf5, name=Ines_B, properties={}] (/127.0.0.1:61234): You are not white-listed on this server!";
        assert_eq!(lire_refus(l), Some(("Ines_B".into(), Some("069a79f444e94726a5befca90e38aaf5".into()))));
        // Paper
        let l = "[10:12:03 INFO]: Malo (/127.0.0.1:61234) lost connection: You are not whitelisted on this server!";
        assert_eq!(lire_refus(l), Some(("Malo".into(), None)));
        let l = "[10:12:03 INFO]: Disconnecting Sarah (/127.0.0.1:61234): You are not white-listed on this server!";
        assert_eq!(lire_refus(l), Some(("Sarah".into(), None)));
        // Joueur Bedrock (Floodgate)
        let l = "[10:12:03 INFO]: Disconnecting GameProfile[id=00000000-0000-0000-0009-01f2c3d4e5f6, name=.Yanis, properties={}] (/127.0.0.1:1): You are not white-listed on this server!";
        assert_eq!(lire_refus(l).map(|r| r.0), Some(".Yanis".into()));
    }

    #[test]
    fn ne_se_trompe_pas() {
        assert_eq!(lire_refus("[10:12:03 INFO]: <Alex> Disconnecting Bob: not whitelisted lol"), None);
        assert_eq!(lire_refus("[10:12:03 INFO]: KOTORO_YT lost connection: Disconnected"), None);
        assert_eq!(lire_refus("[10:12:03 INFO]: Added Hugo to the whitelist"), None);
        assert_eq!(lire_refus("[10:12:03 INFO]: Disconnecting /127.0.0.1:5 (/127.0.0.1:5): You are not whitelisted"), None);
        assert_eq!(lire_refus("[10:12:03 INFO]: Turned on the whitelist"), None);
    }

    #[test]
    fn tient_les_demandes() {
        let mut r = Registre::default();
        assert_eq!(r.noter("a", "Hugo", None, 1_000), Note::Nouvelle);
        // La 2e ligne du même essai ne compte pas.
        assert_eq!(r.noter("a", "hugo", Some("x".repeat(32)), 3_000), Note::Rien);
        assert_eq!(r.noter("a", "Hugo", Some("f".repeat(32)), 60_000), Note::Encore);
        let l = r.liste("a");
        assert_eq!((l.len(), l[0].fois, l[0].quand), (1, 2, 60_000));
        assert_eq!(r.trouver("a", "HUGO").and_then(|d| d.uuid.clone()), Some("f".repeat(32)));

        assert_eq!(r.noter("a", "Malo", None, 70_000), Note::Nouvelle);
        assert_eq!(r.liste("a")[0].nom, "Malo");

        // Refuser : ses essais suivants ne reviennent pas.
        r.refuser("a", "Malo");
        assert_eq!(r.noter("a", "Malo", None, 200_000), Note::Rien);
        assert!(r.liste("a").iter().all(|d| d.nom != "Malo"));
        // Accepté plus tard (ajouté à la main) : il n'est plus refusé.
        assert!(r.accepter("a", "malo"));
        assert_eq!(r.noter("a", "Malo", None, 300_000), Note::Nouvelle);

        // Déjà sur la liste blanche : la demande part.
        assert!(r.nettoyer("a", &["MALO".into()]));
        assert!(!r.nettoyer("a", &["MALO".into()]));
        assert!(r.accepter("a", "Hugo"));
        assert!(r.liste("a").is_empty());
        r.oublier("a");
        assert!(r.liste("a").is_empty());
    }

    #[test]
    fn garde_trente_demandes_au_plus() {
        let mut r = Registre::default();
        for i in 0..40u64 {
            r.noter("a", &format!("Joueur{i}"), None, 100_000 * (i + 1));
        }
        let l = r.liste("a");
        assert_eq!(l.len(), ATTENTE_MAX);
        assert_eq!(l.last().unwrap().nom, "Joueur10");
    }

    #[test]
    fn se_garde_et_se_relit() {
        let d = std::env::temp_dir().join("bhost-test-demandes");
        let _ = std::fs::create_dir_all(&d);
        let f = d.join("demandes.json");
        let mut r = Registre::default();
        r.noter("a", "Hugo", None, 5);
        r.refuser("a", "Malo");
        r.enregistrer(&f);
        let mut r2 = Registre::charger(&f);
        assert_eq!(r2.liste("a")[0].nom, "Hugo");
        assert_eq!(r2.noter("a", "Malo", None, 99_999), Note::Rien);
        assert!(Registre::charger(&d.join("absent.json")).liste("a").is_empty());
    }
}
