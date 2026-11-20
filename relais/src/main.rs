// Le programme du relais : `bhost-relais --domaine bhost.app --joueurs 0.0.0.0:25565 --agents 0.0.0.0:4443 --registre noms.json`
use bhost_relais::{Reglages, demarrer};
use std::path::PathBuf;

fn valeur(args: &[String], nom: &str, defaut: &str) -> String {
    args.iter().position(|a| a == nom).and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| defaut.to_string())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--aide" || a == "--help") {
        println!("bhost-relais --domaine bhost.app --joueurs 0.0.0.0:25565 --agents 0.0.0.0:4443 --registre noms.json");
        return;
    }
    let lire = |nom: &str, defaut: &str| valeur(&args, nom, defaut).parse().unwrap_or_else(|_| panic!("{nom} : adresse invalide"));
    let reglages = Reglages {
        domaine: valeur(&args, "--domaine", "bhost.app"),
        joueurs: lire("--joueurs", "0.0.0.0:25565"),
        agents: lire("--agents", "0.0.0.0:4443"),
        registre: Some(PathBuf::from(valeur(&args, "--registre", "noms.json"))),
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
