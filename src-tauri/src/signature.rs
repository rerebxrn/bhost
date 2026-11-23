// Ce qui prouve qu'un fichier vient bien de nous : le dépôt GitHub des versions et la clé publique de signature.
// Les fichiers publiés (installeur Windows, agent des machines) sont signés dans GitHub Actions avec la clé secrète
// (`tauri signer sign`, format minisign) ; BHOST et l'agent refusent tout fichier dont la signature ne colle pas.

use base64::Engine;

/// Le dépôt GitHub des versions : « compte/dépôt ».
pub const DEPOT: &str = "rerebxrn/bhost";
/// La clé publique de signature (contenu du fichier .pub de `tauri signer generate`, en base64).
pub const CLE_PUBLIQUE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc4OUE1NzJCOUJBN0E5QjMKUldTenFhZWJLMWVhZVBvV0ppS0Q4K3JrRTAyRHR5QmFDVklBU1dZUEtyYVJBcyswNm9ja3F0aS8K";

/// Une adresse de la dernière version publiée (`fichier` : « latest.json », « bhost-agent-linux-x86_64 »…).
#[cfg_attr(feature = "bureau", allow(dead_code))]
pub fn derniere(fichier: &str) -> String {
    format!("https://github.com/{DEPOT}/releases/latest/download/{fichier}")
}

/// Vérifie la signature `sig` (le fichier .sig de `tauri signer sign`, en base64) des `octets`.
#[cfg_attr(feature = "bureau", allow(dead_code))]
pub fn verifier(octets: &[u8], sig: &str, cle: &str) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let texte = |s: &str| b64.decode(s.trim()).ok().and_then(|o| String::from_utf8(o).ok());
    let cle = texte(cle).and_then(|t| minisign_verify::PublicKey::decode(&t).ok()).ok_or("Clé de signature invalide.")?;
    let sig = texte(sig).and_then(|t| minisign_verify::Signature::decode(&t).ok()).ok_or("Signature illisible.")?;
    cle.verify(octets, &sig, false).map_err(|_| "Signature fausse : fichier refusé.".to_string())
}

/// Vrai si la version `a` (« 0.2.10 ») est plus récente que `b`.
#[cfg_attr(feature = "bureau", allow(dead_code))]
pub fn plus_recente(a: &str, b: &str) -> bool {
    let n = |v: &str| v.trim().trim_start_matches('v').split(['.', '-']).take(3).map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    n(a) > n(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(plus_recente("0.2.0", "0.1.9"));
        assert!(plus_recente("0.10.0", "0.9.0"));
        assert!(!plus_recente("0.1.0", "0.1.0"));
        assert!(!plus_recente("v0.1.0", "0.2.0"));
    }

    #[test]
    fn vraie_signature() {
        // Fichiers faits avec `tauri signer generate` puis `tauri signer sign` (clé de test).
        let cle = include_str!("../tests/signature/cle.pub");
        let sig = include_str!("../tests/signature/f.bin.sig");
        assert_eq!(verifier(b"bonjour\n", sig, cle), Ok(()));
        assert!(verifier(b"bonjour !\n", sig, cle).is_err());
        assert!(verifier(b"bonjour\n", sig, CLE_PUBLIQUE).is_err());
    }

    #[test]
    fn signature_refusee() {
        assert!(verifier(b"x", "", CLE_PUBLIQUE).is_err());
        // Une vraie signature minisign, mais pas pour ces octets.
        let faux = base64::engine::general_purpose::STANDARD.encode(
            "untrusted comment: x\nRUSzqaebK1eaeOPfGPvnWzVwQ9rq/sKy2uIBQi4xJ0g4ZrYzVx8sX0r5+zZ6t8Hq5r1Q1pM1Yk2H0g6vJm1V1rO0d1aZ5mN3XwQ=\ntrusted comment: x\nAAAA\n",
        );
        assert!(verifier(b"x", &faux, CLE_PUBLIQUE).is_err());
    }
}
