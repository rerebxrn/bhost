//! Machine dédiée (BHOST OS) : ce que le tutoriel peut déjà faire sous Windows.
//! L'écriture de la clé et la liaison viendront avec BHOST OS.

use serde::Serialize;

/// Une clé USB branchée sur ce PC.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleUsb {
    /// Lettre du lecteur, par exemple « E: ».
    pub lecteur: String,
    /// Nom de la clé (son étiquette), ou « Clé USB » si elle n'en a pas.
    pub nom: String,
    /// Taille en Go, arrondie au dixième.
    pub total_go: f64,
}

/// Les disques amovibles branchés (clés USB), pour l'étape « Prépare la clé USB ».
#[tauri::command]
pub fn lister_cles_usb() -> Vec<CleUsb> {
    let go = |octets: u64| (octets as f64 / 1_000_000_000.0 * 10.0).round() / 10.0;
    let Ok(disques) = sysinfo::Disks::new_with_refreshed_list() else {
        return Vec::new();
    };
    let mut cles: Vec<CleUsb> = disques
        .iter()
        .filter(|d| d.is_removable() && d.total_space() > 0)
        .map(|d| {
            let nom = d.name().to_string_lossy().trim().to_string();
            CleUsb {
                lecteur: d.mount_point().to_string_lossy().trim_end_matches(['\\', '/']).to_string(),
                nom: if nom.is_empty() { "Clé USB".into() } else { nom },
                total_go: go(d.total_space()),
            }
        })
        .collect();
    cles.sort_by(|a, b| a.lecteur.cmp(&b.lecteur));
    cles
}
