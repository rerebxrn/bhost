// Le programme du relais : `bhost-relais --domaine bhost.app --joueurs 0.0.0.0:25565 --agents 0.0.0.0:4443 --registre noms.json`
// BHOST+ (relais prioritaire) : `--plus-cle /opt/bhost-relais/plus.cle.pub`, et `--joueurs-max 400`.
use bhost_relais::{Reglages, demarrer};
use std::path::PathBuf;

fn valeur(args: &[String], nom: &str, defaut: &str) -> String {
    args.iter().position(|a| a == nom).and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| defaut.to_string())
}

/// Chaque joueur relayé tient deux connexions : la limite du système (souvent 1024) arrive vite.
#[cfg(unix)]
fn plus_de_fichiers() {
    let mut l = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY : getrlimit et setrlimit ne lisent et n'écrivent que cette structure.
    unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut l) == 0 && l.rlim_cur < l.rlim_max {
            l.rlim_cur = l.rlim_max.min(65_536);
            libc::setrlimit(libc::RLIMIT_NOFILE, &l);
        }
    }
}

#[tokio::main]
async fn main() {
    #[cfg(unix)]
    plus_de_fichiers();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--aide" || a == "--help") {
        println!("bhost-relais --domaine bhost.app --joueurs 0.0.0.0:25565 --agents 0.0.0.0:4443 --registre noms.json [--joueurs-max 400] [--plus-cle plus.cle.pub]");
        return;
    }
    let lire = |nom: &str, defaut: &str| valeur(&args, nom, defaut).parse().unwrap_or_else(|_| panic!("{nom} : adresse invalide"));
    let reglages = Reglages {
        domaine: valeur(&args, "--domaine", "bhost.app"),
        joueurs: lire("--joueurs", "0.0.0.0:25565"),
        agents: lire("--agents", "0.0.0.0:4443"),
        registre: Some(PathBuf::from(valeur(&args, "--registre", "noms.json"))),
        joueurs_max: valeur(&args, "--joueurs-max", "400").parse().expect("--joueurs-max : un nombre"),
        // BHOST+ : la clé publique du service (fichier plus.cle.pub, écrit par bhost-plus). Absente : pas de priorité.
        plus_cle: args
            .iter()
            .position(|a| a == "--plus-cle")
            .and_then(|i| args.get(i + 1))
            .map(|f| std::fs::read_to_string(f).unwrap_or_else(|_| panic!("{f} : clé BHOST+ illisible"))),
    };
    let domaine = reglages.domaine.clone();
    let (joueurs, agents, relais) = demarrer(reglages).await.expect("impossible d'ouvrir les ports");
    println!("Relais BHOST pour *.{domaine} : joueurs sur {joueurs}, BHOST sur {agents}.");
    // Ctrl+C, ou « systemctl stop » (SIGTERM) : on enregistre avant de partir.
    #[cfg(unix)]
    {
        let mut terme = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("signaux");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terme.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
    println!("Arrêt ({} serveurs en ligne).", relais.en_ligne());
    relais.enregistrer();
}
