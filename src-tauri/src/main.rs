// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Copie administrateur de BHOST : écrire BHOST OS sur une clé USB (voir src/cle.rs), puis s'arrêter.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(bhost_lib::cle::OPTION) {
        let travail = args.get(2).map(std::path::PathBuf::from).unwrap_or_default();
        std::process::exit(bhost_lib::cle::ecrire_depuis_commande(&travail));
    }
    bhost_lib::run()
}
