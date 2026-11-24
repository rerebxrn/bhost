// BHOST+ côté serveur (plus.bhost.app, derrière Caddy qui fait le HTTPS).
//
// L'appli vérifie sa clé directement chez Polar (adresses publiques). Ce petit service sert pour ce que Polar
// ne dit qu'avec la clé secrète de l'organisation :
//   - les PC où la clé est activée (pour en libérer un depuis l'appli) ;
//   - l'abonnement en retard de paiement ;
//   - le jeton du relais prioritaire, signé ici et vérifié par le relais (relais/src/lib.rs).
//
// Une seule adresse : POST /etat  {"cle": "...", "activation": "...", "agent": "<clé publique du PC, hexa>"}
// Réglages (variables d'environnement, fichier /opt/bhost-relais/plus.env, jamais dans Git) :
//   POLAR_JETON         la clé d'accès de l'organisation Polar (lecture des clés de licence et des abonnements)
//   POLAR_ORGANISATION  l'identifiant de l'organisation
//   POLAR_API           (essais) https://sandbox-api.polar.sh
//   BHOST_PLUS_ECOUTE   127.0.0.1:8130
//   BHOST_PLUS_CLE      /opt/bhost-relais/plus.cle (créée au premier lancement ; plus.cle.pub pour le relais)

mod polar;
mod service;

use std::io::Read;

fn variable(nom: &str, defaut: &str) -> String {
    std::env::var(nom).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| defaut.to_string())
}

fn main() {
    let ecoute = variable("BHOST_PLUS_ECOUTE", "127.0.0.1:8130");
    let fichier_cle = variable("BHOST_PLUS_CLE", "plus.cle");
    let polar = polar::Polar {
        api: variable("POLAR_API", "https://api.polar.sh"),
        jeton: variable("POLAR_JETON", ""),
        organisation: variable("POLAR_ORGANISATION", ""),
        agent: polar::agent(),
    };
    if polar.jeton.is_empty() || polar.organisation.is_empty() {
        eprintln!("POLAR_JETON et POLAR_ORGANISATION manquent (fichier plus.env) : BHOST+ répondra « indisponible ».");
    }
    let signature = service::charger_cle(std::path::Path::new(&fichier_cle)).expect("impossible de lire ou créer la clé de signature");
    let service = service::Service::nouveau(polar, signature);
    let serveur = tiny_http::Server::http(&ecoute).expect("impossible d'ouvrir l'adresse d'écoute");
    println!("BHOST+ sur {ecoute}.");
    let serveur = std::sync::Arc::new(serveur);
    let service = std::sync::Arc::new(service);
    // Quelques fils : une demande à Polar prend une fraction de seconde.
    let fils: Vec<_> = (0..4)
        .map(|_| {
            let (serveur, service) = (serveur.clone(), service.clone());
            std::thread::spawn(move || {
                for demande in serveur.incoming_requests() {
                    repondre(&service, demande);
                }
            })
        })
        .collect();
    for f in fils {
        let _ = f.join();
    }
}

fn repondre(service: &service::Service, mut demande: tiny_http::Request) {
    // Derrière Caddy : l'adresse du visiteur est dans X-Forwarded-For (on n'écoute que 127.0.0.1).
    let ip = demande
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Forwarded-For"))
        .and_then(|h| h.value.as_str().split(',').next().map(|s| s.trim().to_string()))
        .or_else(|| demande.remote_addr().map(|a| a.ip().to_string()))
        .unwrap_or_default();
    let (code, corps) = if demande.method() != &tiny_http::Method::Post || demande.url() != "/etat" {
        (404, r#"{"erreur":"introuvable"}"#.to_string())
    } else {
        let mut texte = String::new();
        let lu = demande.as_reader().take(4096).read_to_string(&mut texte);
        match lu {
            Ok(_) => service.etat(&ip, &texte),
            Err(_) => (400, r#"{"erreur":"demande"}"#.to_string()),
        }
    };
    let entete = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("entête");
    let reponse = tiny_http::Response::from_string(corps).with_status_code(code).with_header(entete);
    let _ = demande.respond(reponse);
}
