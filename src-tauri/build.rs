fn main() {
    // Tauri range l'icône dans le programme (.exe) au moment de le fabriquer, mais ne surveille pas ce fichier :
    // sans ces lignes, une nouvelle icône ne serait pas pris en compte au démarrage.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/azur-sombre.ico");
    println!("cargo:rerun-if-changed=icons/ambre-clair.ico");
    println!("cargo:rerun-if-changed=icons/ambre-sombre.ico");
    // L'agent de BHOST OS (sans « bureau ») n'a ni fenêtre ni icône : rien à préparer pour Tauri.
    if std::env::var_os("CARGO_FEATURE_BUREAU").is_some() {
        tauri_build::build()
    }
}
