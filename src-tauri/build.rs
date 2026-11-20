fn main() {
    // Tauri range l'icône dans le programme (.exe) au moment de le fabriquer, mais ne surveille pas ce fichier :
    // sans ces lignes, une nouvelle icône ne serait pas pris en compte au démarrage.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/azur-sombre.ico");
    println!("cargo:rerun-if-changed=icons/ambre-clair.ico");
    println!("cargo:rerun-if-changed=icons/ambre-sombre.ico");
    tauri_build::build()
}
