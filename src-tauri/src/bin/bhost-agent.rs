//! bhost-agent : le cœur de BHOST sur BHOST OS (sans fenêtre).
//! Les données vont dans /var/lib/bhost (ou dans le dossier de la variable BHOST_DOSSIER).
//!
//! Par défaut, la machine se connecte au relais et attend qu'un PC la pilote avec son code (src/pilotage.rs).
//! Avec `--stdio` (essais), les commandes arrivent sur l'entrée et les réponses partent sur la sortie,
//! une ligne JSON à la fois (voir src/agent.rs).

use bhost_lib::{agent, pilotage};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn main() {
    let dossier = std::env::var_os("BHOST_DOSSIER").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/var/lib/bhost"));
    let stdio = std::env::args().any(|a| a == "--stdio");
    // BHOST OS : chaque serveur aura son groupe (mémoire max, part du processeur). Avant tout lancement.
    bhost_lib::processus::cgroupes::preparer();
    let runtime = tokio::runtime::Runtime::new().expect("tokio");
    let _dedans = runtime.enter();

    // Une seule écriture à la fois sur la sortie (réponses et nouvelles se mélangent sinon).
    let sortie = Arc::new(Mutex::new(std::io::stdout()));
    let ecrire = move |ligne: String| {
        let mut s = sortie.lock().unwrap();
        let _ = writeln!(s, "{ligne}");
        let _ = s.flush();
    };

    // Les nouvelles (statut, console…) partent vers tous les PC qui pilotent la machine.
    let sessions = Arc::new(pilotage::Sessions::default());
    let diffusion = {
        let (sessions, ecrire) = (sessions.clone(), ecrire.clone());
        move |evt: &str, data: serde_json::Value| {
            let ligne = agent::ligne_nouvelle(evt, data);
            if stdio {
                ecrire(ligne);
            } else {
                sessions.diffuser(&ligne);
            }
        }
    };
    let app = match agent::demarrer(dossier.clone(), Box::new(diffusion)) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("[bhost-agent] {e}");
            std::process::exit(1);
        }
    };
    agent::nettoyer_envois(&dossier);
    match (pilotage::code(&dossier), pilotage::id(&dossier)) {
        (Ok(code), Ok(id)) => {
            agent::noter_machine(id.clone());
            eprintln!("[bhost-agent] prêt · machine {id} · code {}-{}", &code[..4], &code[4..]);
        }
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("[bhost-agent] code ou clé de la machine : {e}");
            std::process::exit(1);
        }
    }

    // Extinction de la machine (systemd envoie SIGTERM) ou Ctrl+C : les mondes sont enregistrés avant de quitter.
    {
        let app = app.clone();
        runtime.spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{SignalKind, signal};
                let mut fin = signal(SignalKind::terminate()).expect("signal");
                tokio::select! { _ = fin.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("[bhost-agent] arrêt des serveurs…");
            tokio::task::spawn_blocking(move || agent::tout_arreter(&app)).await.ok();
            std::process::exit(0);
        });
    }

    if !stdio {
        // En direct (réseau de la maison, VPS) et par le relais : les deux à la fois.
        pilotage::ecouter_direct(dossier.clone(), app.clone(), sessions.clone(), runtime.handle().clone());
        pilotage::servir(dossier, app, sessions, runtime.handle().clone(), Arc::new(pilotage::EtatRelais::default()));
        // Le travail se fait dans les autres fils : on attend l'arrêt (signal ci-dessus).
        loop {
            std::thread::park();
        }
    }

    for ligne in std::io::stdin().lock().lines() {
        let Ok(ligne) = ligne else { break };
        if ligne.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<agent::Demande>(&ligne) {
            Ok(demande) => {
                let (app, ecrire) = (app.clone(), ecrire.clone());
                // Chaque commande à part : une création de serveur ne bloque pas « lister_serveurs ».
                runtime.spawn(async move { ecrire(agent::repondre(&app, demande).await) });
            }
            Err(e) => ecrire(serde_json::json!({ "err": format!("Ligne illisible : {e}") }).to_string()),
        }
    }
}
