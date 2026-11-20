// Les « ticks par seconde » : 20 quand le serveur va bien, moins quand ça lag.
// Le serveur ne les donne que si on lui demande : BHOST lui envoie la commande « tps »
// et lit la réponse dans sa console.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use std::time::{Duration, Instant};

/// Temps entre deux demandes.
const PAUSE: Duration = Duration::from_secs(10);
/// Après autant de demandes sans réponse, on arrête de demander.
const ESSAIS: u8 = 3;

/// La commande à envoyer, si le logiciel en a une.
/// Spigot et sa famille répondent à « tps ». Vanilla, Fabric et Forge : pas encore.
pub fn commande(logiciel: &str) -> Option<&'static str> {
    matches!(logiciel, "Spigot" | "Paper" | "Purpur" | "Youer" | "Arclight" | "Mohist").then_some("tps")
}

/// Lit la réponse du serveur : « TPS from last 1m, 5m, 15m: 20.0, 19.98, *20.0 ».
/// On garde le premier chiffre (la dernière minute). Un message du chat ne peut pas
/// tromper BHOST : il commence par « <pseudo> » ou « [Server] ».
pub fn lire(ligne: &str) -> Option<f32> {
    // On saute l'en-tête « [18:04:02 INFO]: ».
    let texte = ligne.split_once("]: ").map(|(_, t)| t).unwrap_or(ligne);
    let texte = sans_couleurs(texte);
    let suite = texte.trim_start().strip_prefix("TPS from last ")?;
    let (_, valeurs) = suite.split_once(':')?;
    let premiere = valeurs.split(',').next()?.trim().trim_start_matches('*');
    let tps: f32 = premiere.parse().ok()?;
    tps.is_finite().then(|| tps.clamp(0.0, 20.0))
}

/// Enlève les codes de couleur de Minecraft (« §a », « §6 »…).
fn sans_couleurs(texte: &str) -> String {
    let mut propre = String::with_capacity(texte.len());
    let mut lettres = texte.chars();
    while let Some(c) = lettres.next() {
        if c == '§' {
            lettres.next();
        } else {
            propre.push(c);
        }
    }
    propre
}

/// Où en est BHOST avec un serveur : a-t-il posé la question, attend-il la réponse ?
#[derive(Default)]
pub struct Suivi {
    /// Heure de la dernière demande.
    demande: Option<Instant>,
    /// Vrai tant que la réponse à notre demande n'est pas arrivée.
    en_attente: bool,
    /// Demandes restées sans réponse, à la suite.
    rates: u8,
}

impl Suivi {
    /// Faut-il envoyer la commande maintenant ? Si oui, la demande est notée.
    pub fn demander(&mut self, maintenant: Instant) -> bool {
        if self.rates >= ESSAIS {
            return false; // ce serveur ne répond pas à « tps » : on n'insiste pas
        }
        if self.demande.is_some_and(|avant| maintenant.duration_since(avant) < PAUSE) {
            return false;
        }
        if self.en_attente {
            self.rates += 1;
            if self.rates >= ESSAIS {
                return false;
            }
        }
        self.demande = Some(maintenant);
        self.en_attente = true;
        true
    }

    /// Une réponse vient d'arriver. Renvoie vrai si c'est BHOST qui l'avait demandée :
    /// il faut alors la cacher de la console. Sinon, c'est l'utilisateur qui a tapé « tps ».
    pub fn recevoir(&mut self) -> bool {
        let pour_bhost = self.en_attente;
        self.en_attente = false;
        self.rates = 0;
        pour_bhost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_la_reponse_de_spigot_paper_et_purpur() {
        // Spigot 1.8.8 (le serveur de Thomas) et Paper.
        assert_eq!(lire("[12:22:03 INFO]: TPS from last 1m, 5m, 15m: 20.0, 19.98, 19.95"), Some(20.0));
        assert_eq!(lire("[12:22:03] [Server thread/INFO]: TPS from last 1m, 5m, 15m: 18.4, 19.5, 19.9"), Some(18.4));
        // Une étoile quand le serveur rattrape son retard, et les couleurs si elles restent.
        assert_eq!(lire("[12:22:03 INFO]: TPS from last 1m, 5m, 15m: *20.0, *20.0, 19.95"), Some(20.0));
        assert_eq!(lire("[12:22:03 INFO]: §6TPS from last 1m, 5m, 15m: §a19.7, §a20.0, §a20.0"), Some(19.7));
        // Purpur ajoute les 5 dernières secondes.
        assert_eq!(lire("[12:22:03 INFO]: TPS from last 5s, 1m, 5m, 15m: 12.31, 19.0, 20.0, 20.0"), Some(12.31));
    }

    #[test]
    fn ne_se_laisse_pas_tromper() {
        assert_eq!(lire("[18:05:17 INFO]: <Alex> TPS from last 1m, 5m, 15m: 3.0, 3.0, 3.0"), None);
        assert_eq!(lire("[18:05:17 INFO]: [Server] TPS from last 1m, 5m, 15m: 3.0, 3.0, 3.0"), None);
        assert_eq!(lire("[18:05:17 INFO]: <Alex> ]: TPS from last 1m: 3.0"), None);
        assert_eq!(lire("[18:05:17 INFO]: TPS from last 1m, 5m, 15m: beaucoup"), None);
        assert_eq!(lire("[18:05:17 INFO]: Unknown command. Type \"/help\" for help."), None);
        assert_eq!(lire("[18:05:17 INFO]: Done (2,1s)! For help, type \"help\""), None);
    }

    #[test]
    fn seule_la_famille_spigot_a_la_commande() {
        assert_eq!(commande("Spigot"), Some("tps"));
        assert_eq!(commande("Purpur"), Some("tps"));
        assert_eq!(commande("Mohist"), Some("tps"));
        assert_eq!(commande("Vanilla"), None);
        assert_eq!(commande("Fabric"), None);
        assert_eq!(commande("NeoForge"), None);
    }

    #[test]
    fn demande_toutes_les_dix_secondes_et_cache_sa_reponse() {
        let debut = Instant::now();
        let mut suivi = Suivi::default();
        assert!(suivi.demander(debut));
        assert!(!suivi.demander(debut + Duration::from_secs(4))); // trop tôt
        assert!(suivi.recevoir()); // la réponse à BHOST : cachée
        assert!(!suivi.recevoir()); // l'utilisateur a tapé « tps » lui-même : affichée
        assert!(suivi.demander(debut + Duration::from_secs(10)));
        assert!(suivi.recevoir());
    }

    #[test]
    fn n_insiste_pas_si_le_serveur_ne_repond_pas() {
        let debut = Instant::now();
        let dans = |s| debut + Duration::from_secs(s);
        let mut suivi = Suivi::default();
        assert!(suivi.demander(dans(0)));
        assert!(suivi.demander(dans(10)));
        assert!(suivi.demander(dans(20)));
        assert!(!suivi.demander(dans(30))); // trois demandes sans réponse : on arrête
        assert!(!suivi.demander(dans(300)));

        // Une réponse entre-temps remet le compteur à zéro.
        let mut suivi = Suivi::default();
        assert!(suivi.demander(dans(0)));
        assert!(suivi.demander(dans(10)));
        assert!(suivi.recevoir());
        assert!(suivi.demander(dans(20)));
        assert!(suivi.demander(dans(30)));
        assert!(suivi.demander(dans(40)));
        assert!(!suivi.demander(dans(50)));
    }
}
