//! Machine dédiée (BHOST OS) : ce que le tutoriel peut déjà faire sous Windows.
//! L'écriture de la clé et la vraie liaison viendront avec BHOST OS.

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

// ---- Liaison avec une machine (code de 8 caractères) ----
// Pour l'instant, seulement la forme du code : le contact avec la machine (par le relais)
// arrive avec BHOST OS. Les commandes existent déjà pour que l'interface ait une vraie réponse.

/// Les caractères possibles d'un code : pas de 0, O, 1 ni I (on les confond en recopiant).
pub const ALPHABET: &str = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
pub const LONGUEUR: usize = 8;

/// « k7m2-q9xr » → « K7M2Q9XR » ; `None` si le code n'a pas 8 caractères valides.
pub fn nettoyer_code(texte: &str) -> Option<String> {
    let mut code = String::new();
    for c in texte.chars() {
        if c == '-' || c.is_whitespace() {
            continue;
        }
        let c = c.to_ascii_uppercase();
        if !ALPHABET.contains(c) {
            return None;
        }
        code.push(c);
    }
    (code.chars().count() == LONGUEUR).then_some(code)
}

/// Les machines reliées à ce PC (aucune tant que BHOST OS n'existe pas).
#[tauri::command]
pub fn machines_liees() -> Vec<serde_json::Value> {
    Vec::new()
}

/// Relie une machine avec son code.
#[tauri::command]
pub fn lier_machine(code: String) -> Result<serde_json::Value, String> {
    if nettoyer_code(&code).is_none() {
        return Err("Ce code n'est pas valide. Vérifie-le.".into());
    }
    Err("BHOST OS n'est pas encore disponible : la liaison marchera dans une prochaine version.".into())
}

/// Retire une machine de BHOST (ses serveurs continuent de tourner dessus).
#[tauri::command]
pub fn delier_machine(id: String) -> Result<(), String> {
    let _ = id;
    Ok(())
}

/// Donne un autre nom à une machine (« Salon », « Grenier »…).
#[tauri::command]
pub fn renommer_machine(id: String, nom: String) -> Result<(), String> {
    let _ = id;
    nom_valide(&nom)?;
    Err("Machine introuvable.".into())
}

/// Un nom de machine : pas vide, 40 caractères au plus.
pub fn nom_valide(nom: &str) -> Result<String, String> {
    let nom = nom.trim();
    if nom.is_empty() {
        return Err("Donne-lui un nom.".into());
    }
    if nom.chars().count() > 40 {
        return Err("40 caractères au plus.".into());
    }
    Ok(nom.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_nettoye() {
        assert_eq!(nettoyer_code("k7m2-q9xr").as_deref(), Some("K7M2Q9XR"));
        assert_eq!(nettoyer_code(" K7M2 Q9XR ").as_deref(), Some("K7M2Q9XR"));
        assert_eq!(nettoyer_code("K7M2Q9X"), None); // trop court
        assert_eq!(nettoyer_code("K7M2Q9XRA"), None); // trop long
        assert_eq!(nettoyer_code("K0M2Q9XR"), None); // 0 interdit
        assert_eq!(nettoyer_code("KIM2Q9XR"), None); // I interdit
    }

    #[test]
    fn nom_de_machine() {
        assert_eq!(nom_valide("  Salon ").as_deref(), Ok("Salon"));
        assert!(nom_valide("   ").is_err());
        assert!(nom_valide(&"a".repeat(41)).is_err());
    }

    #[test]
    fn meme_alphabet_que_l_interface() {
        let ts = include_str!("../../src/code.ts");
        assert!(ts.contains(&format!("ALPHABET = \"{ALPHABET}\"")));
        assert!(ts.contains(&format!("LONGUEUR = {LONGUEUR};")));
    }
}
