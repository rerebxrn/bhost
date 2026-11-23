// Onglet Automatisations : ce que BHOST fait tout seul pour un serveur.
// « Quand ? » (une heure, un crash, un joueur qui arrive…) puis « Alors… » (démarrer, annoncer…).
// Ici : les règles, le calcul des heures, les seuils (lag, mémoire), l'historique.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use chrono::{Datelike, Duration as Jours, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Au plus 30 automatisations par serveur.
pub const MAX: usize = 30;
/// On garde les 30 dernières actions faites, par serveur.
const HISTORIQUE_MAX: usize = 30;

/// Une automatisation. Les champs qui ne servent pas à son déclencheur gardent leur valeur par défaut.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Automatisation {
    pub id: String,
    /// Active ou en pause.
    pub on: bool,
    /// Quand : time, crash, empty, join, boot, lag, mem, wake, upd (une mise à jour des plugins ou mods sort).
    pub trig: String,
    /// Alors : start, stop, restart, backup, say, cmd, notify, update (mettre à jour les plugins et mods).
    pub act: String,
    /// À une heure : once (une fois), daily (chaque jour), days (certains jours), every (toutes les N heures).
    pub when: String,
    /// « 05:00 ».
    pub time: String,
    /// Certains jours : 0 = lundi … 6 = dimanche.
    pub days: Vec<u8>,
    /// Une fois : le jour, « 2026-11-18 ».
    pub date: String,
    /// Régulièrement : toutes les N heures (1 à 24), à partir de minuit.
    pub hours: u32,
    /// Arrêter ou redémarrer à une heure : prévenir les joueurs avant (1, 5 ou 15 minutes).
    pub warn: bool,
    pub warn_min: u32,
    /// Annonce : le message ({joueur} = son pseudo).
    pub msg: String,
    /// Commande, sans « / » ({joueur} = son pseudo).
    pub cmd: String,
    /// Il est vide : personne depuis N minutes (5 à 240).
    pub mins: u32,
    /// Un joueur arrive : seulement la première fois.
    pub first: bool,
    /// Le message de la liste des serveurs pendant que le serveur dort (réveil).
    pub motd: String,
}

impl Default for Automatisation {
    fn default() -> Self {
        Automatisation {
            id: String::new(),
            on: true,
            trig: "time".into(),
            act: "restart".into(),
            when: "daily".into(),
            time: "05:00".into(),
            days: vec![4],
            date: String::new(),
            hours: 2,
            warn: true,
            warn_min: 5,
            msg: String::new(),
            cmd: String::new(),
            mins: 30,
            first: true,
            motd: String::new(),
        }
    }
}

/// Les actions possibles pour chaque déclencheur.
pub fn actions_permises(trig: &str) -> &'static [&'static str] {
    match trig {
        "time" => &["start", "stop", "restart", "backup", "say", "cmd", "notify"],
        "crash" => &["restart", "notify"],
        "empty" => &["stop", "restart", "backup"],
        "join" => &["say", "cmd", "notify"],
        "boot" => &["cmd", "say", "notify"],
        "lag" => &["notify", "say", "restart", "cmd"],
        "mem" => &["restart", "notify"],
        "wake" => &["start"],
        "upd" => &["update", "notify"],
        _ => &[],
    }
}

/// « 05:00 » → 5 h 00.
pub fn heure(texte: &str) -> Option<NaiveTime> {
    let (h, m) = texte.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)
}

/// Vérifie une automatisation avant de l'enregistrer, et la met au propre.
/// `tps_possible` : le logiciel du serveur donne ses ticks par seconde (pour « Ça lag »).
/// `maj_possible` : le serveur a des plugins ou des mods (pour « Une mise à jour sort »).
pub fn verifier(a: &mut Automatisation, tps_possible: bool, maj_possible: bool) -> Result<(), String> {
    if !actions_permises(&a.trig).contains(&a.act.as_str()) {
        return Err("Cette action ne va pas avec ce déclencheur.".into());
    }
    a.msg = a.msg.replace(['\r', '\n'], " ").trim().to_string();
    a.cmd = a.cmd.replace(['\r', '\n'], " ").trim().trim_start_matches('/').trim().to_string();
    a.motd = a.motd.replace(['\r', '\n'], " ").trim().to_string();
    if a.act == "say" && a.msg.is_empty() {
        return Err("Écris le message.".into());
    }
    if a.act == "cmd" && a.cmd.is_empty() {
        return Err("Écris la commande.".into());
    }
    if a.msg.chars().count() > 200 || a.cmd.chars().count() > 250 || a.motd.chars().count() > 120 {
        return Err("Texte trop long.".into());
    }
    if a.trig == "time" {
        heure(&a.time).ok_or("Heure invalide.")?;
        match a.when.as_str() {
            "daily" => {}
            "days" => {
                a.days.retain(|d| *d < 7);
                a.days.sort_unstable();
                a.days.dedup();
                if a.days.is_empty() {
                    return Err("Choisis au moins un jour.".into());
                }
            }
            "once" => {
                NaiveDate::parse_from_str(&a.date, "%Y-%m-%d").map_err(|_| "Date invalide.")?;
            }
            "every" => {
                if !(1..=24).contains(&a.hours) {
                    return Err("Entre 1 et 24 heures.".into());
                }
            }
            _ => return Err("Choisis quand.".into()),
        }
    }
    if a.trig == "empty" && !(5..=240).contains(&a.mins) {
        return Err("Entre 5 et 240 minutes.".into());
    }
    if a.trig == "lag" && !tps_possible {
        return Err("Pas encore pour ce logiciel.".into());
    }
    if a.trig == "upd" && !maj_possible {
        return Err("Ce serveur n’a ni plugins ni mods.".into());
    }
    if !matches!(a.warn_min, 1 | 5 | 15) {
        a.warn_min = 5;
    }
    Ok(())
}

/// Prévient-on les joueurs avant cette action ?
pub fn previent(a: &Automatisation) -> bool {
    a.trig == "time" && a.warn && matches!(a.act.as_str(), "stop" | "restart")
}

/// La prochaine heure (strictement après `apres`) où une automatisation « à une heure » se déclenche.
/// Les heures sont celles du PC (heure locale).
pub fn prochaine_apres(a: &Automatisation, apres: NaiveDateTime) -> Option<NaiveDateTime> {
    if a.trig != "time" {
        return None;
    }
    let jour = apres.date();
    if a.when == "every" {
        let pas = a.hours.clamp(1, 24);
        for k in 0..=1 {
            let date = jour + Jours::days(k);
            for h in (0..24).step_by(pas as usize) {
                let t = date.and_hms_opt(h, 0, 0)?;
                if t > apres {
                    return Some(t);
                }
            }
        }
        return None;
    }
    let h = heure(&a.time)?;
    match a.when.as_str() {
        "once" => {
            let t = NaiveDate::parse_from_str(&a.date, "%Y-%m-%d").ok()?.and_time(h);
            (t > apres).then_some(t)
        }
        "daily" | "days" => (0..=8).map(|k| (jour + Jours::days(k)).and_time(h)).find(|t| {
            *t > apres && (a.when == "daily" || a.days.contains(&(t.weekday().num_days_from_monday() as u8)))
        }),
        _ => None,
    }
}

/// Les moments où l'on prévient avant l'action (en secondes avant), puis l'action (0).
pub fn avances(a: &Automatisation) -> Vec<i64> {
    let mut v = Vec::new();
    if previent(a) {
        let m = a.warn_min as i64;
        v.push(m * 60);
        if m > 1 {
            v.push(60);
        }
        v.push(10);
    }
    v.push(0);
    v
}

/// Ce qui tombe entre `avant` (exclu) et `maintenant` (inclus) : pour chaque moment,
/// le nombre de secondes avant l'action (0 = l'action elle-même).
pub fn echeances(a: &Automatisation, avant: NaiveDateTime, maintenant: NaiveDateTime) -> Vec<i64> {
    if !a.on || maintenant <= avant {
        return Vec::new();
    }
    avances(a)
        .into_iter()
        .filter(|o| {
            let o2 = Jours::seconds(*o);
            prochaine_apres(a, avant + o2).is_some_and(|t| t - o2 <= maintenant)
        })
        .collect()
}

/// Le message envoyé aux joueurs avant un arrêt ou un redémarrage.
pub fn message_avant(act: &str, secondes: i64) -> String {
    let quand = if secondes >= 60 { format!("{} min", secondes / 60) } else { format!("{secondes} s") };
    if act == "stop" { format!("Le serveur s’arrête dans {quand}.") } else { format!("Le serveur redémarre dans {quand}.") }
}

/// Remplace {joueur} par le pseudo.
pub fn remplir(texte: &str, joueur: &str) -> String {
    texte.replace("{joueur}", joueur)
}

/// Le nom court d'une action, pour l'historique.
pub fn nom_action(act: &str) -> &'static str {
    match act {
        "start" => "Démarrage",
        "stop" => "Arrêt",
        "restart" => "Redémarrage",
        "backup" => "Sauvegarde",
        "say" => "Annonce",
        "cmd" => "Commande",
        "notify" => "Notification",
        "update" => "Mise à jour",
        _ => "Action",
    }
}

/// « EssentialsX », « EssentialsX et Chunky », « EssentialsX, Chunky et 3 autres ».
pub fn resume_noms(noms: &[String]) -> String {
    match noms {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} et {b}"),
        [a, b, c] => format!("{a}, {b} et {c}"),
        [a, b, reste @ ..] => format!("{a}, {b} et {} autres", reste.len()),
    }
}

/// Une condition qui doit durer (lag, mémoire pleine) avant de déclencher, puis un temps de repos.
#[derive(Default, Debug)]
pub struct Seuil {
    depuis: Option<Instant>,
    dernier: Option<Instant>,
}

impl Seuil {
    /// Une nouvelle mesure. Renvoie vrai quand il faut agir : `depasse` dure depuis `duree`,
    /// et la dernière fois date d'au moins `repos`.
    pub fn mesurer(&mut self, depasse: bool, maintenant: Instant, duree: Duration, repos: Duration) -> bool {
        if !depasse {
            self.depuis = None;
            return false;
        }
        let depuis = *self.depuis.get_or_insert(maintenant);
        if maintenant.duration_since(depuis) < duree || self.dernier.is_some_and(|d| maintenant.duration_since(d) < repos) {
            return false;
        }
        self.dernier = Some(maintenant);
        self.depuis = None;
        true
    }
}

/// Les relances après un crash : `max` essais en 15 minutes, pas plus.
#[derive(Default, Debug)]
pub struct Relances {
    essais: Vec<Instant>,
}

impl Relances {
    /// Peut-on relancer encore ? Si oui, l'essai est noté et son numéro renvoyé (1, 2, 3…).
    pub fn essayer(&mut self, max: usize, maintenant: Instant) -> Option<usize> {
        self.essais.retain(|t| maintenant.duration_since(*t) < Duration::from_secs(15 * 60));
        if self.essais.len() >= max {
            return None;
        }
        self.essais.push(maintenant);
        Some(self.essais.len())
    }
}

/// Une action faite, pour « Dernières actions ».
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fait {
    pub quoi: String,
    pub act: String,
    /// Millisecondes depuis 1970.
    pub quand: u64,
}

/// Ce que BHOST garde entre deux lancements : l'historique et les joueurs déjà venus.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Memoire {
    pub historique: BTreeMap<String, Vec<Fait>>,
    /// Pseudos (en minuscules) déjà venus sur chaque serveur.
    pub connus: BTreeMap<String, Vec<String>>,
}

impl Memoire {
    pub fn charger(fichier: &Path) -> Memoire {
        std::fs::read_to_string(fichier).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn enregistrer(&self, fichier: &Path) {
        if let Ok(texte) = serde_json::to_string(self) {
            let _ = std::fs::write(fichier, texte);
        }
    }

    /// Note une action faite (la plus récente en premier).
    pub fn noter(&mut self, id: &str, fait: Fait) {
        let liste = self.historique.entry(id.to_string()).or_default();
        liste.insert(0, fait);
        liste.truncate(HISTORIQUE_MAX);
    }

    /// Les joueurs déjà venus : remplis la première fois avec ceux du serveur (usercache.json).
    pub fn preparer_connus(&mut self, id: &str, dossier: &Path) {
        if self.connus.contains_key(id) {
            return;
        }
        let noms = std::fs::read_to_string(dossier.join("usercache.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok())
            .unwrap_or_default()
            .iter()
            .filter_map(|e| e.get("name")?.as_str().map(str::to_ascii_lowercase))
            .collect();
        self.connus.insert(id.to_string(), noms);
    }

    /// Un joueur arrive : vrai si c'est sa première venue (il est alors noté).
    pub fn premiere_venue(&mut self, id: &str, nom: &str) -> bool {
        let liste = self.connus.entry(id.to_string()).or_default();
        let nom = nom.to_ascii_lowercase();
        if liste.contains(&nom) {
            return false;
        }
        // Au plus 10 000 pseudos par serveur : on oublie les plus anciens.
        if liste.len() >= 10_000 {
            liste.remove(0);
        }
        liste.push(nom);
        true
    }

    /// Un serveur retiré de BHOST : on oublie tout de lui.
    pub fn oublier(&mut self, id: &str) {
        self.historique.remove(id);
        self.connus.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(texte: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(texte, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn a(when: &str) -> Automatisation {
        Automatisation { id: "a1".into(), when: when.into(), ..Default::default() }
    }

    #[test]
    fn calcule_la_prochaine_heure() {
        // Mardi 17 novembre 2026.
        let chaque_jour = a("daily");
        assert_eq!(prochaine_apres(&chaque_jour, t("2026-11-17 04:00:00")), Some(t("2026-11-17 05:00:00")));
        assert_eq!(prochaine_apres(&chaque_jour, t("2026-11-17 05:00:00")), Some(t("2026-11-18 05:00:00")));
        // Certains jours : vendredi (4) et dimanche (6).
        let jours = Automatisation { days: vec![4, 6], ..a("days") };
        assert_eq!(prochaine_apres(&jours, t("2026-11-17 12:00:00")), Some(t("2026-11-20 05:00:00")));
        assert_eq!(prochaine_apres(&jours, t("2026-11-20 06:00:00")), Some(t("2026-11-22 05:00:00")));
        // Une fois.
        let fois = Automatisation { date: "2026-11-19".into(), time: "20:30".into(), ..a("once") };
        assert_eq!(prochaine_apres(&fois, t("2026-11-17 12:00:00")), Some(t("2026-11-19 20:30:00")));
        assert_eq!(prochaine_apres(&fois, t("2026-11-19 20:30:00")), None);
        // Toutes les 6 h, à partir de minuit.
        let six = Automatisation { hours: 6, ..a("every") };
        assert_eq!(prochaine_apres(&six, t("2026-11-17 13:10:00")), Some(t("2026-11-17 18:00:00")));
        assert_eq!(prochaine_apres(&six, t("2026-11-17 23:00:00")), Some(t("2026-11-18 00:00:00")));
        // Autre déclencheur : pas d'heure.
        assert_eq!(prochaine_apres(&Automatisation { trig: "crash".into(), ..a("daily") }, t("2026-11-17 04:00:00")), None);
    }

    #[test]
    fn previent_avant_puis_agit() {
        let r = a("daily"); // redémarrer à 05:00, prévenir 5 min avant
        assert_eq!(avances(&r), [300, 60, 10, 0]);
        assert_eq!(echeances(&r, t("2026-11-17 04:54:58"), t("2026-11-17 04:55:01")), [300]);
        assert_eq!(echeances(&r, t("2026-11-17 04:59:49"), t("2026-11-17 04:59:51")), [10]);
        assert_eq!(echeances(&r, t("2026-11-17 04:59:59"), t("2026-11-17 05:00:01")), [0]);
        assert!(echeances(&r, t("2026-11-17 05:00:01"), t("2026-11-17 05:00:03")).is_empty());
        // En pause : rien.
        assert!(echeances(&Automatisation { on: false, ..r.clone() }, t("2026-11-17 04:59:59"), t("2026-11-17 05:00:01")).is_empty());
        // Une annonce ne prévient pas.
        assert_eq!(avances(&Automatisation { act: "say".into(), ..r }), [0]);
        assert_eq!(message_avant("restart", 300), "Le serveur redémarre dans 5 min.");
        assert_eq!(message_avant("stop", 10), "Le serveur s’arrête dans 10 s.");
    }

    #[test]
    fn resume_des_noms() {
        let n = |l: &[&str]| resume_noms(&l.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(n(&[]), "");
        assert_eq!(n(&["EssentialsX"]), "EssentialsX");
        assert_eq!(n(&["EssentialsX", "Chunky"]), "EssentialsX et Chunky");
        assert_eq!(n(&["A", "B", "C"]), "A, B et C");
        assert_eq!(n(&["A", "B", "C", "D", "E"]), "A, B et 3 autres");
    }

    #[test]
    fn verifie_et_nettoie() {
        let mut b = Automatisation { trig: "join".into(), act: "cmd".into(), cmd: " /give {joueur} bread\n".into(), ..Default::default() };
        verifier(&mut b, true, true).unwrap();
        assert_eq!(b.cmd, "give {joueur} bread");
        assert_eq!(remplir(&b.cmd, "Hugo"), "give Hugo bread");
        assert!(verifier(&mut Automatisation { trig: "join".into(), act: "stop".into(), ..Default::default() }, true, true).is_err());
        assert!(verifier(&mut Automatisation { act: "say".into(), ..Default::default() }, true, true).is_err());
        assert!(verifier(&mut Automatisation { when: "days".into(), days: vec![], ..Default::default() }, true, true).is_err());
        assert!(verifier(&mut Automatisation { time: "25:00".into(), ..Default::default() }, true, true).is_err());
        assert!(verifier(&mut Automatisation { trig: "lag".into(), act: "notify".into(), ..Default::default() }, false, true).is_err());
        assert!(verifier(&mut Automatisation { trig: "empty".into(), act: "stop".into(), mins: 2, ..Default::default() }, true, true).is_err());
        assert!(verifier(&mut Automatisation { trig: "wake".into(), act: "start".into(), ..Default::default() }, true, true).is_ok());
        // Une mise à jour sort : seulement s'il y a des plugins ou des mods, et pour mettre à jour ou prévenir.
        assert!(verifier(&mut Automatisation { trig: "upd".into(), act: "update".into(), ..Default::default() }, true, true).is_ok());
        assert!(verifier(&mut Automatisation { trig: "upd".into(), act: "notify".into(), ..Default::default() }, true, true).is_ok());
        assert!(verifier(&mut Automatisation { trig: "upd".into(), act: "update".into(), ..Default::default() }, true, false).is_err());
        assert!(verifier(&mut Automatisation { trig: "upd".into(), act: "restart".into(), ..Default::default() }, true, true).is_err());
        assert!(verifier(&mut Automatisation { trig: "time".into(), act: "update".into(), ..Default::default() }, true, true).is_err());
        assert_eq!(nom_action("update"), "Mise à jour");
    }

    #[test]
    fn seuil_qui_dure_puis_repos() {
        let debut = Instant::now();
        let s = |n: u64| debut + Duration::from_secs(n);
        let (duree, repos) = (Duration::from_secs(60), Duration::from_secs(600));
        let mut seuil = Seuil::default();
        assert!(!seuil.mesurer(true, s(0), duree, repos));
        assert!(!seuil.mesurer(true, s(30), duree, repos));
        assert!(!seuil.mesurer(false, s(40), duree, repos)); // ça repart
        assert!(!seuil.mesurer(true, s(50), duree, repos));
        assert!(seuil.mesurer(true, s(110), duree, repos));
        assert!(!seuil.mesurer(true, s(300), duree, repos)); // au repos
        assert!(seuil.mesurer(true, s(780), duree, repos));
    }

    #[test]
    fn trois_relances_au_plus() {
        let debut = Instant::now();
        let mut r = Relances::default();
        assert_eq!(r.essayer(3, debut), Some(1));
        assert_eq!(r.essayer(3, debut + Duration::from_secs(60)), Some(2));
        assert_eq!(r.essayer(3, debut + Duration::from_secs(120)), Some(3));
        assert_eq!(r.essayer(3, debut + Duration::from_secs(180)), None);
        assert_eq!(r.essayer(3, debut + Duration::from_secs(16 * 60)), Some(2));
    }

    #[test]
    fn historique_et_joueurs_connus() {
        let dossier = std::env::temp_dir().join(format!("bhost-auto-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).unwrap();
        std::fs::write(dossier.join("usercache.json"), r#"[{"name":"KOTORO_YT","uuid":"x","expiresOn":"y"}]"#).unwrap();
        let mut m = Memoire::default();
        m.preparer_connus("s1", &dossier);
        assert!(!m.premiere_venue("s1", "kotoro_yt"));
        assert!(m.premiere_venue("s1", "Hugo"));
        assert!(!m.premiere_venue("s1", "hugo"));
        for i in 0..40 {
            m.noter("s1", Fait { quoi: format!("n{i}"), act: "say".into(), quand: i });
        }
        assert_eq!(m.historique["s1"].len(), 30);
        assert_eq!(m.historique["s1"][0].quoi, "n39");
        let fichier = dossier.join("automatisations.json");
        m.enregistrer(&fichier);
        assert_eq!(Memoire::charger(&fichier).historique["s1"].len(), 30);
        m.oublier("s1");
        assert!(m.historique.is_empty() && m.connus.is_empty());
        let _ = std::fs::remove_dir_all(&dossier);
    }
}
