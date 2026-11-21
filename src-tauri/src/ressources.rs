//! Règles des machines dédiées (mode « sérieux ») : la mémoire et le stockage de chaque serveur.
//! - La RAM n'est réservée que pendant qu'un serveur est allumé : un serveur éteint ne bloque rien.
//! - Démarrer un serveur n'est possible que si tout tient dans la RAM de la machine ; sinon BHOST
//!   dit combien il manque et propose d'arrêter un des serveurs allumés.
//! - Le stockage max est un plafond, pas une réservation : la somme peut dépasser le disque.
//!   Alerte à 90 %, et à 100 % : sauvegarde puis arrêt propre.
//! Sans Tauri : le cœur de BHOST OS se sert des mêmes fonctions (certaines ne servent pas encore sur PC).
#![allow(dead_code)]

use serde::Serialize;

const GO: u64 = 1024;

/// Ce que le système de la machine garde pour lui (BHOST OS est minimal : Linux et l'agent tiennent en ≈ 400 Mo).
pub const SYSTEME_MO: u64 = 768;

/// La limite dure de chaque serveur (cgroup) : sa mémoire + 1 Go pour Java. Au-delà, il est arrêté.
pub const MARGE_JAVA_MO: u64 = 1024;

/// Ce que Java prend vraiment en plus de sa mémoire (-Xmx), pour compter la place : 15 % (512 Mo au moins).
pub fn marge_java(ram_mo: u64) -> u64 {
    (ram_mo * 15 / 100).max(512)
}

/// Un serveur allumé sur la machine.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Allume {
    pub id: String,
    pub nom: String,
    pub ram_mo: u64,
}

/// Le refus de démarrer : combien il manque, et les serveurs qu'on peut arrêter
/// (type `ManqueMemoire` de src/api.ts).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manque {
    pub manque_mo: u64,
    pub allumes: Vec<Allume>,
}

/// Ce qu'un serveur prend vraiment quand il est allumé.
pub fn empreinte(ram_mo: u64) -> u64 {
    ram_mo + marge_java(ram_mo)
}

/// La RAM réservée par les serveurs allumés (la jauge « Mémoire réservée » de la barre de gauche).
pub fn reservee_mo(allumes: &[Allume]) -> u64 {
    allumes.iter().map(|s| s.ram_mo).sum()
}

/// Peut-on démarrer un serveur de `ram_mo` à côté de ceux qui tournent déjà ?
/// Non : combien il manque, et les serveurs allumés, du plus gros au plus petit.
pub fn verifier_demarrage(total_mo: u64, allumes: &[Allume], ram_mo: u64) -> Result<(), Manque> {
    let besoin = SYSTEME_MO + allumes.iter().map(|s| empreinte(s.ram_mo)).sum::<u64>() + empreinte(ram_mo);
    if besoin <= total_mo {
        return Ok(());
    }
    let mut allumes = allumes.to_vec();
    allumes.sort_by(|a, b| b.ram_mo.cmp(&a.ram_mo).then_with(|| a.nom.cmp(&b.nom)));
    Err(Manque { manque_mo: besoin - total_mo, allumes })
}

/// La machine redémarre : les serveurs « Démarrer avec la machine », dans l'ordre de la liste.
/// Ceux qui ne rentrent pas restent éteints (on essaie quand même les suivants, plus petits).
/// Renvoie (à démarrer, laissés éteints).
pub fn au_demarrage(total_mo: u64, serveurs: &[(String, u64)]) -> (Vec<String>, Vec<String>) {
    let mut libre = total_mo.saturating_sub(SYSTEME_MO);
    let (mut lances, mut laisses) = (Vec::new(), Vec::new());
    for (id, ram) in serveurs {
        if empreinte(*ram) <= libre {
            libre -= empreinte(*ram);
            lances.push(id.clone());
        } else {
            laisses.push(id.clone());
        }
    }
    (lances, laisses)
}

/// Créer un serveur : la mémoire la plus haute du curseur, en Go (un serveur seul doit pouvoir démarrer).
pub fn ram_max_go(total_mo: u64) -> u64 {
    (1..=1024).take_while(|go| SYSTEME_MO + empreinte(go * GO) <= total_mo).last().unwrap_or(1)
}

/// Le plus petit stockage max (5 Go).
pub const STOCKAGE_MIN_GO: u64 = 5;
/// Le plus grand stockage max (500 Go).
pub const STOCKAGE_PLAFOND_GO: u64 = 500;

/// Créer un serveur : les bornes du curseur « Stockage max » (5 Go → place libre, 500 Go au plus).
pub fn bornes_stockage_go(libre_go: u64) -> (u64, u64) {
    (STOCKAGE_MIN_GO, libre_go.clamp(STOCKAGE_MIN_GO, STOCKAGE_PLAFOND_GO))
}

/// Où en est le stockage d'un serveur (ou du disque de la machine).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Disque {
    Normal,
    /// ≥ 90 % : on prévient.
    Alerte,
    /// Plein : on sauvegarde, puis on arrête proprement.
    Plein,
}

pub fn etat_disque(pris_mo: u64, max_mo: u64) -> Disque {
    if max_mo == 0 || pris_mo >= max_mo {
        Disque::Plein
    } else if pris_mo * 10 >= max_mo * 9 {
        Disque::Alerte
    } else {
        Disque::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str, go: u64) -> Allume {
        Allume { id: id.into(), nom: id.into(), ram_mo: go * GO }
    }

    #[test]
    fn demarrage_quand_ca_rentre() {
        // 16 Go : système + 8 Go et 2 Go (+ leur marge Java) ; un serveur de 1 Go rentre encore.
        let allumes = [s("public", 8), s("test", 2)];
        assert_eq!(verifier_demarrage(16 * GO, &allumes, GO), Ok(()));
        // Rien d'allumé : un serveur de 12 Go rentre sur 16 Go, pas un de 14 Go.
        assert_eq!(verifier_demarrage(16 * GO, &[], 12 * GO), Ok(()));
        assert!(verifier_demarrage(16 * GO, &[], 14 * GO).is_err());
    }

    #[test]
    fn manque_et_liste_du_plus_gros_au_plus_petit() {
        // L'aperçu « Pas assez de mémoire » : Entre amis (4 Go) à côté de Serveur public (8) et Test (2).
        let allumes = [s("test", 2), s("public", 8)];
        let m = verifier_demarrage(16 * GO, &allumes, 4 * GO).unwrap_err();
        // 768 Mo + (8 Go + 15 %) + (2 Go + 512 Mo) + (4 Go + 15 %) = 17 458 Mo pour 16 384 : il manque 1 074 Mo.
        assert_eq!(m.manque_mo, 1074);
        assert_eq!(m.allumes.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["public", "test"]);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"manqueMo\":1074") && json.contains("\"ramMo\":8192"));
    }

    #[test]
    fn redemarrage_de_la_machine() {
        // 8 Go : système + 4 Go (+ marge) ; le 3 Go ne rentre plus ; le 512 Mo si.
        let liste = [("a".to_string(), 4 * GO), ("b".to_string(), 3 * GO), ("c".to_string(), 512)];
        let (lances, laisses) = au_demarrage(8 * GO, &liste);
        assert_eq!(lances, ["a", "c"]);
        assert_eq!(laisses, ["b"]);
        assert_eq!(au_demarrage(GO, &liste).0.len(), 0);
    }

    #[test]
    fn curseurs_de_l_assistant() {
        assert_eq!(ram_max_go(16 * GO), 13);
        // Une machine de 8 Go (7,8 Go vus par Linux) : 6 Go pour un serveur.
        assert_eq!(ram_max_go(7987), 6);
        assert_eq!(ram_max_go(4 * GO), 2);
        assert_eq!(ram_max_go(2 * GO), 1); // jamais moins de 1 Go
        // Un serveur seul réglé au maximum peut toujours démarrer.
        assert_eq!(verifier_demarrage(16 * GO, &[], ram_max_go(16 * GO) * GO), Ok(()));
        assert_eq!(bornes_stockage_go(308), (5, 308));
        assert_eq!(bornes_stockage_go(2000), (5, 500));
        assert_eq!(bornes_stockage_go(2), (5, 5));
    }

    #[test]
    fn stockage_plein() {
        assert_eq!(etat_disque(10 * GO, 20 * GO), Disque::Normal);
        assert_eq!(etat_disque(18 * GO, 20 * GO), Disque::Alerte);
        assert_eq!(etat_disque(20 * GO, 20 * GO), Disque::Plein);
        assert_eq!(etat_disque(0, 0), Disque::Plein);
    }

    #[test]
    fn memoire_reservee() {
        assert_eq!(reservee_mo(&[s("a", 8), s("b", 2)]), 10 * GO);
    }
}
