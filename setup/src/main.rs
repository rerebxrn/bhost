// BHOST Setup : installe BHOST avec une fenêtre aux couleurs de l'appli (src/installeur).
// Le vrai travail est fait par l'installeur Windows de BHOST, rangé dans ce programme et lancé sans fenêtre :
// même dossier dans le registre, mêmes raccourcis, même désinstallation, mêmes mises à jour.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use std::{env, fs};
use tauri::{AppHandle, Emitter};

/// L'installeur Windows de BHOST (NSIS), rangé ici par build.rs.
const INSTALLEUR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bhost-nsis.exe"));
/// La taille de bhost.exe une fois installé : la barre de progression suit sa copie.
const TAILLE_BHOST: &str = env!("TAILLE_BHOST");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Infos {
    dossier: String,
    lecteur: String,
    libre_go: u64,
}

/// Le dossier par défaut : celui de l'installeur Windows de BHOST (sans droits administrateur).
fn dossier_par_defaut() -> PathBuf {
    let base = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    base.join("BHOST")
}

/// Un dossier choisi à la main : on installe dans « BHOST » à l'intérieur, sauf s'il s'appelle déjà ainsi.
fn normaliser(choisi: &str) -> PathBuf {
    let p = PathBuf::from(choisi.trim());
    let deja = p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("BHOST"));
    if deja { p } else { p.join("BHOST") }
}

/// Le disque qui contient ce dossier, et sa place libre.
fn disque(dossier: &Path) -> (String, u64) {
    let Ok(disques) = sysinfo::Disks::new_with_refreshed_list() else {
        return ("?".into(), 0);
    };
    let trouve = disques
        .iter()
        .filter(|d| dossier.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len());
    match trouve {
        Some(d) => {
            let nom = d.mount_point().to_string_lossy().trim_end_matches(['\\', '/']).to_string();
            (if nom.is_empty() { "/".into() } else { nom }, d.available_space() / 1_000_000_000)
        }
        None => ("?".into(), 0),
    }
}

#[tauri::command]
fn infos(dossier: Option<String>) -> Infos {
    let dossier = dossier.map(|d| normaliser(&d)).unwrap_or_else(dossier_par_defaut);
    let (lecteur, libre_go) = disque(&dossier);
    Infos { dossier: dossier.to_string_lossy().into_owned(), lecteur, libre_go }
}

/// Lance l'installeur Windows sans fenêtre. /D= doit être le dernier argument, sans guillemets.
fn lancer_installeur(installeur: &Path, dossier: Option<&Path>) -> std::io::Result<std::process::Child> {
    let mut c = Command::new(installeur);
    if let Some(dossier) = dossier {
        c.arg("/S");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.raw_arg(format!("/D={}", dossier.display()));
        }
        #[cfg(not(windows))]
        c.arg(format!("/D={}", dossier.display()));
    }
    c.spawn()
}

fn copier_installeur() -> Result<PathBuf, String> {
    let chemin = env::temp_dir().join(format!("bhost-installeur-{}.exe", std::process::id()));
    fs::write(&chemin, INSTALLEUR).map_err(|_| "Pas assez de place sur le disque.".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&chemin, fs::Permissions::from_mode(0o700));
    }
    Ok(chemin)
}

fn installer_bloquant(app: &AppHandle, dossier: &Path) -> Result<(), String> {
    // On vérifie qu'on peut écrire ici (Program Files demande des droits que BHOST ne prend pas).
    let essai = dossier.join(".bhost-essai");
    fs::create_dir_all(dossier)
        .and_then(|_| fs::write(&essai, b""))
        .and_then(|_| fs::remove_file(&essai))
        .map_err(|_| "Dossier protégé. Choisis-en un autre.".to_string())?;

    let installeur = copier_installeur()?;
    let resultat = (|| {
        let mut enfant = lancer_installeur(&installeur, Some(dossier)).map_err(|_| "L’installation n’a pas pu démarrer.".to_string())?;
        let total = TAILLE_BHOST.parse::<u64>().unwrap_or(1).max(1);
        let exe = dossier.join("bhost.exe");
        let debut = Instant::now();
        let mut dernier = u8::MAX;
        loop {
            if let Some(etat) = enfant.try_wait().map_err(|_| "L’installation s’est arrêtée.".to_string())? {
                return if etat.success() { Ok(()) } else { Err("L’installation a échoué. Réessaie.".to_string()) };
            }
            // Avance : la copie de bhost.exe, ou à défaut le temps écoulé (environ 3 s), sans jamais finir avant l'installeur.
            let copie = fs::metadata(&exe).map(|m| m.len() * 100 / total).unwrap_or(0);
            let temps = (90.0 * (1.0 - (-debut.elapsed().as_secs_f64() / 3.0).exp())) as u64;
            let pct = copie.max(temps).min(95) as u8;
            if pct != dernier {
                dernier = pct;
                let _ = app.emit("avancement", pct);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })();
    let _ = fs::remove_file(&installeur);
    resultat
}

#[tauri::command]
async fn installer(app: AppHandle, dossier: String) -> Result<(), String> {
    let dossier = normaliser(&dossier);
    tauri::async_runtime::spawn_blocking(move || installer_bloquant(&app, &dossier))
        .await
        .map_err(|_| "L’installation s’est arrêtée.".to_string())?
}

/// Ouvre BHOST, puis BHOST Setup se ferme.
#[tauri::command]
fn ouvrir(app: AppHandle, dossier: String) -> Result<(), String> {
    let dossier = normaliser(&dossier);
    Command::new(dossier.join("bhost.exe"))
        .current_dir(&dossier)
        .spawn()
        .map_err(|_| "BHOST n’a pas pu s’ouvrir. Cherche-le dans le menu Démarrer.".to_string())?;
    app.exit(0);
    Ok(())
}

/// Sans WebView2 (vieux Windows 10), pas de fenêtre possible : l'installeur Windows classique prend le relais
/// (lui sait installer WebView2).
fn secours() {
    if let Ok(installeur) = copier_installeur() {
        if let Ok(mut enfant) = lancer_installeur(&installeur, None) {
            let _ = enfant.wait();
        }
        let _ = fs::remove_file(installeur);
    }
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![infos, installer, ouvrir])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(_) => secours(),
    }
}
