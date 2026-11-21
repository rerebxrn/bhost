// Les « ticks par seconde » : 20 quand le serveur va bien, moins quand ça lag.
// Le serveur ne les donne que si on lui demande : BHOST lui envoie la commande « tps »
// et lit la réponse dans sa console.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use std::time::{Duration, Instant};

/// Temps entre deux demandes.
const PAUSE: Duration = Duration::from_secs(10);
/// Après autant de demandes sans réponse, on arrête de demander.
const ESSAIS: u8 = 3;

/// « 1.20.3 » ou plus récent (« 1.21.4 », « 26.1 ») : la commande « tick » de Minecraft existe.
fn a_tick_query(version: &str) -> bool {
    let n: Vec<u32> = version.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).take(3).filter_map(|p| p.parse().ok()).collect();
    match n.as_slice() {
        [a, ..] if *a >= 2 => true,
        [1, b, ..] if *b > 20 => true,
        [1, 20, c] => *c >= 3,
        _ => false,
    }
}

/// La commande à envoyer, si le logiciel en a une.
/// - Spigot et sa famille (et les hybrides) : « tps ».
/// - Vanilla, Fabric, Forge, NeoForge depuis la 1.20.3 : « tick query » (la commande de Minecraft).
/// - Forge et NeoForge plus anciens : « forge tps » / « neoforge tps ». Vanilla et Fabric plus anciens : rien.
pub fn commande(logiciel: &str, version: &str) -> Option<&'static str> {
    match logiciel {
        "Spigot" | "Paper" | "Purpur" | "Youer" | "Arclight" | "Mohist" => Some("tps"),
        _ if a_tick_query(version) && matches!(logiciel, "Vanilla" | "Fabric" | "Forge" | "NeoForge") => Some("tick query"),
        "Forge" => Some("forge tps"),
        "NeoForge" => Some("neoforge tps"),
        _ => None,
    }
}

/// Le texte d'une ligne de console, sans l'en-tête (« [18:04:02 INFO]: ») ni les couleurs.
fn texte(ligne: &str) -> String {
    sans_couleurs(ligne.split_once("]: ").map(|(_, t)| t).unwrap_or(ligne)).trim().to_string()
}

/// Les autres lignes de la réponse (« tick query », « forge tps ») : cachées avec elle.
pub fn accompagne(ligne: &str) -> bool {
    let t = texte(ligne);
    let forge = t.contains(" TPS (") && t.ends_with("ms/tick)");
    (forge || ["The game is ", "Target tick rate:", "Percentiles:", "Dim "].iter().any(|d| t.starts_with(d))) && !t.starts_with('<')
}

/// Un nombre au début d'un texte (« 20.000 TPS » → 20.0).
fn nombre(t: &str) -> Option<f32> {
    let n: String = t.trim_start().chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    n.parse().ok()
}

/// Lit la réponse du serveur : « TPS from last 1m, 5m, 15m: 20.0, 19.98, *20.0 ».
/// On garde le premier chiffre (la dernière minute). Un message du chat ne peut pas
/// tromper BHOST : il commence par « <pseudo> » ou « [Server] ».
pub fn lire(ligne: &str) -> Option<f32> {
    let texte = texte(ligne);
    let tps = if let Some(suite) = texte.strip_prefix("Average time per tick: ") {
        // « tick query » : « Average time per tick: 63.2ms (Target: 50.0ms) » → 1000 / 63,2.
        let moyenne = nombre(suite)?;
        let cible = suite.split_once("Target: ").and_then(|(_, c)| nombre(c)).unwrap_or(50.0);
        1000.0 / moyenne.max(cible).max(0.001)
    } else if let Some(suite) = texte.strip_prefix("Overall") {
        // Forge : « Overall: Mean tick time: 2.345 ms. Mean TPS: 20.000 » ; NeoForge : « Overall: 20.000 TPS (2.345 ms/tick) ».
        let suite = suite.trim_start().strip_prefix(':')?;
        match suite.split_once("Mean TPS:") {
            Some((_, v)) => nombre(v)?,
            None => nombre(suite).filter(|_| suite.contains("TPS"))?,
        }
    } else {
        return lire_tps(&texte);
    };
    tps.is_finite().then(|| tps.clamp(0.0, 20.0))
}

/// Spigot et sa famille : « TPS from last 1m, 5m, 15m: 20.0, 19.98, *20.0 ».
fn lire_tps(texte: &str) -> Option<f32> {
    let suite = texte.strip_prefix("TPS from last ")?;
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
    /// Quand la dernière réponse à BHOST est arrivée (ses lignes voisines sont cachées aussi).
    repondu: Option<Instant>,
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
        if pour_bhost {
            self.repondu = Some(Instant::now());
        }
        pour_bhost
    }

    /// Une ligne voisine de la réponse (« The game is running normally »…) : cachée si c'est BHOST qui a demandé.
    pub fn cacher_voisine(&self, maintenant: Instant) -> bool {
        let recent = |t: Option<Instant>| t.is_some_and(|t| maintenant.duration_since(t) < Duration::from_secs(3));
        (self.en_attente && recent(self.demande)) || recent(self.repondu)
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
    fn la_commande_selon_le_logiciel() {
        assert_eq!(commande("Spigot", "1.8.8"), Some("tps"));
        assert_eq!(commande("Purpur", "1.21.4"), Some("tps"));
        assert_eq!(commande("Mohist", "1.20.1"), Some("tps"));
        assert_eq!(commande("Vanilla", "1.21.4"), Some("tick query"));
        assert_eq!(commande("Vanilla", "26.1"), Some("tick query"));
        assert_eq!(commande("Fabric", "1.20.3"), Some("tick query"));
        assert_eq!(commande("Fabric", "1.20.2"), None);
        assert_eq!(commande("Vanilla", "1.8.9"), None);
        assert_eq!(commande("Forge", "1.12.2"), Some("forge tps"));
        assert_eq!(commande("Forge", "1.21.1"), Some("tick query"));
        assert_eq!(commande("NeoForge", "1.20.2"), Some("neoforge tps"));
    }

    #[test]
    fn lit_tick_query_et_forge() {
        // Vanilla, Fabric (tick query) : 1000 / temps moyen d'un tick, 20 au plus.
        assert_eq!(lire("[22:05:01] [Server thread/INFO]: Average time per tick: 12.3ms (Target: 50.0ms)"), Some(20.0));
        assert_eq!(lire("[22:05:01] [Server thread/INFO]: Average time per tick: 62.5ms (Target: 50.0ms)"), Some(16.0));
        assert!(accompagne("[22:05:01] [Server thread/INFO]: The game is running normally"));
        assert!(accompagne("[22:05:01] [Server thread/INFO]: Percentiles: P50: 0.2ms P95: 0.6ms P99: 1.3ms, sample: 100"));
        assert!(!accompagne("[22:05:01] [Server thread/INFO]: <Alex> The game is nul"));
        // Forge et NeoForge.
        assert_eq!(lire("[22:05:01] [Server thread/INFO]: Overall: Mean tick time: 2.345 ms. Mean TPS: 19.500"), Some(19.5));
        assert_eq!(lire("[22:05:01] [Server thread/INFO]: Overall : Mean tick time: 2.345 ms. Mean TPS: 20.000"), Some(20.0));
        assert_eq!(lire("[22:05:01] [Server thread/INFO]: Overall: 18.250 TPS (54.795 ms/tick)"), Some(18.25));
        assert!(accompagne("[22:05:01] [Server thread/INFO]: Dim minecraft:overworld (minecraft:overworld): Mean tick time: 2.1 ms. Mean TPS: 20.000"));
        assert_eq!(lire("[22:05:01] [Server thread/INFO]: <Alex> Overall: 3.0 TPS"), None);
        assert!(accompagne("[22:05:01] [Server thread/INFO]: minecraft:the_nether: 20.000 TPS (0.312 ms/tick)"));
    }

    #[test]
    fn cache_les_lignes_voisines() {
        let debut = Instant::now();
        let mut suivi = Suivi::default();
        assert!(!suivi.cacher_voisine(debut));
        assert!(suivi.demander(debut));
        assert!(suivi.cacher_voisine(debut + Duration::from_millis(100)));
        assert!(suivi.recevoir());
        assert!(suivi.cacher_voisine(Instant::now()));
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
