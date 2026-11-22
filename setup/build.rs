// Range dans BHOST Setup l'installeur Windows de BHOST, fabriqué avant par « npm run tauri build ».
use std::{env, fs, path::PathBuf};

fn main() {
    let ici = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let app = ici.join("../src-tauri");
    let conf: serde_json::Value = serde_json::from_str(&fs::read_to_string(app.join("tauri.conf.json")).unwrap()).unwrap();
    let version = conf["version"].as_str().expect("version dans src-tauri/tauri.conf.json");

    let release = app.join("target/release");
    let nsis = release.join(format!("bundle/nsis/BHOST_{version}_x64-setup.exe"));
    let bhost = release.join("bhost.exe");
    for f in [&nsis, &bhost] {
        if !f.exists() {
            panic!("{} introuvable : lance d'abord « npm run tauri build ».", f.display());
        }
        println!("cargo:rerun-if-changed={}", f.display());
    }
    println!("cargo:rerun-if-changed=../src-tauri/tauri.conf.json");

    // L'installeur, et la taille de bhost.exe une fois installé (pour la barre de progression).
    fs::copy(&nsis, PathBuf::from(env::var("OUT_DIR").unwrap()).join("bhost-nsis.exe")).unwrap();
    println!("cargo:rustc-env=TAILLE_BHOST={}", fs::metadata(&bhost).unwrap().len());

    tauri_build::build()
}
