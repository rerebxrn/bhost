// Lire les fichiers .dat de Minecraft (level.dat, game_rules.dat…).
// Ils sont au format « NBT », presque toujours compressés en gzip. On ne fait que les lire : BHOST n'écrit jamais dedans.
// Ce fichier ne dépend pas de Tauri : il se teste tout seul (cargo test).

use std::io::Read;
use std::path::Path;

/// Taille maximale d'un fichier .dat une fois décompressé : 32 Mo (un level.dat fait quelques Ko).
const TAILLE_MAX: u64 = 32 * 1024 * 1024;
/// Profondeur maximale (un fichier abîmé ne doit pas faire planter BHOST).
const PROFONDEUR_MAX: usize = 256;

/// Une valeur lue dans un fichier NBT.
#[derive(Debug, Clone, PartialEq)]
pub enum Nbt {
    Octet(i8),
    Court(i16),
    Entier(i32),
    Long(i64),
    Flottant(f32),
    Double(f64),
    Texte(String),
    Liste(Vec<Nbt>),
    /// Des valeurs nommées, dans l'ordre du fichier.
    Groupe(Vec<(String, Nbt)>),
    /// Un tableau d'entiers (position du point d'apparition…).
    Entiers(Vec<i32>),
    /// Tableaux d'octets et de longs : BHOST n'en a pas besoin, on garde juste leur taille.
    Tableau(usize),
}

impl Nbt {
    /// La valeur nommée `cle` dans un groupe.
    pub fn get(&self, cle: &str) -> Option<&Nbt> {
        match self {
            Nbt::Groupe(valeurs) => valeurs.iter().find(|(k, _)| k == cle).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Suit un chemin de noms : `chemin(&["Data", "Version", "Name"])`.
    pub fn chemin(&self, noms: &[&str]) -> Option<&Nbt> {
        noms.iter().try_fold(self, |v, nom| v.get(nom))
    }

    /// Les entiers d'un tableau, ou d'une liste de nombres.
    pub fn entiers(&self) -> Option<Vec<i64>> {
        match self {
            Nbt::Entiers(l) => Some(l.iter().map(|&x| i64::from(x)).collect()),
            Nbt::Liste(l) => l.iter().map(|x| x.nombre().or_else(|| x.decimal().map(|d| d.floor() as i64))).collect(),
            _ => None,
        }
    }

    /// Un nombre à virgule (position d'un joueur…).
    pub fn decimal(&self) -> Option<f64> {
        match self {
            Nbt::Flottant(n) => Some(f64::from(*n)),
            Nbt::Double(n) => Some(*n),
            autre => autre.nombre().map(|n| n as f64),
        }
    }

    pub fn texte(&self) -> Option<&str> {
        match self {
            Nbt::Texte(t) => Some(t),
            _ => None,
        }
    }

    /// Un nombre entier, quelle que soit sa taille. Les textes « 42 » comptent aussi (vieux level.dat).
    pub fn nombre(&self) -> Option<i64> {
        match self {
            Nbt::Octet(n) => Some(i64::from(*n)),
            Nbt::Court(n) => Some(i64::from(*n)),
            Nbt::Entier(n) => Some(i64::from(*n)),
            Nbt::Long(n) => Some(*n),
            Nbt::Texte(t) => t.trim().parse().ok(),
            _ => None,
        }
    }

    /// Vrai ou faux : un octet (0 ou 1), ou un texte « true » / « false » (vieux level.dat).
    pub fn booleen(&self) -> Option<bool> {
        match self {
            Nbt::Texte(t) => match t.trim() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            autre => autre.nombre().map(|n| n != 0),
        }
    }
}

/// Lit un fichier NBT (compressé en gzip ou non). `None` s'il est absent ou abîmé.
pub fn lire_fichier(fichier: &Path) -> Option<Nbt> {
    let octets = crate::garde::lire_debut(fichier, TAILLE_MAX + 1).ok()?;
    if octets.len() as u64 > TAILLE_MAX {
        return None; // un level.dat ne fait jamais 32 Mo
    }
    lire(&octets)
}

/// Au plus un million de valeurs dans un fichier : un petit fichier piégé ne doit pas remplir la mémoire.
const VALEURS_MAX: usize = 1_000_000;

/// Lit le contenu d'un fichier NBT. `None` s'il est abîmé.
pub fn lire(octets: &[u8]) -> Option<Nbt> {
    let brut;
    let donnees = if octets.starts_with(&[0x1f, 0x8b]) {
        let mut sortie = Vec::new();
        flate2::read::GzDecoder::new(octets).take(TAILLE_MAX).read_to_end(&mut sortie).ok()?;
        brut = sortie;
        &brut[..]
    } else {
        octets
    };
    let mut lecteur = Lecteur { octets: donnees, pos: 0, budget: VALEURS_MAX };
    let genre = lecteur.octet()?;
    if genre != 10 {
        return None; // la racine est toujours un groupe
    }
    let _nom = lecteur.texte()?;
    lecteur.valeur(10, 0)
}

struct Lecteur<'a> {
    octets: &'a [u8],
    pos: usize,
    /// Combien de valeurs on peut encore lire.
    budget: usize,
}

impl Lecteur<'_> {
    fn prendre(&mut self, n: usize) -> Option<&[u8]> {
        let fin = self.pos.checked_add(n)?;
        let morceau = self.octets.get(self.pos..fin)?;
        self.pos = fin;
        Some(morceau)
    }
    fn octet(&mut self) -> Option<u8> {
        Some(self.prendre(1)?[0])
    }
    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_be_bytes(self.prendre(2)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.prendre(4)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.prendre(8)?.try_into().ok()?))
    }
    /// Une longueur (jamais négative, jamais plus grande que ce qui reste).
    fn longueur(&mut self, taille_element: usize) -> Option<usize> {
        let n = usize::try_from(self.i32()?).ok()?;
        (n.checked_mul(taille_element)? <= self.octets.len() - self.pos).then_some(n)
    }
    fn texte(&mut self) -> Option<String> {
        let n = u16::from_be_bytes(self.prendre(2)?.try_into().ok()?) as usize;
        // Java écrit un UTF-8 « modifié » : pour des noms de mondes, l'UTF-8 ordinaire suffit.
        Some(String::from_utf8_lossy(self.prendre(n)?).into_owned())
    }

    fn valeur(&mut self, genre: u8, profondeur: usize) -> Option<Nbt> {
        if profondeur > PROFONDEUR_MAX {
            return None;
        }
        self.budget = self.budget.checked_sub(1)?;
        Some(match genre {
            1 => Nbt::Octet(self.octet()? as i8),
            2 => Nbt::Court(self.i16()?),
            3 => Nbt::Entier(self.i32()?),
            4 => Nbt::Long(self.i64()?),
            5 => Nbt::Flottant(f32::from_bits(self.i32()? as u32)),
            6 => Nbt::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.longueur(1)?;
                self.prendre(n)?;
                Nbt::Tableau(n)
            }
            8 => Nbt::Texte(self.texte()?),
            9 => {
                let genre_elements = self.octet()?;
                let n = self.longueur(if genre_elements == 0 { 0 } else { 1 })?;
                let mut liste = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    liste.push(self.valeur(genre_elements, profondeur + 1)?);
                }
                Nbt::Liste(liste)
            }
            10 => {
                let mut valeurs = Vec::new();
                loop {
                    let genre_valeur = self.octet()?;
                    if genre_valeur == 0 {
                        break;
                    }
                    let nom = self.texte()?;
                    valeurs.push((nom, self.valeur(genre_valeur, profondeur + 1)?));
                }
                Nbt::Groupe(valeurs)
            }
            11 => {
                let n = self.longueur(4)?;
                let mut entiers = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    entiers.push(self.i32()?);
                }
                Nbt::Entiers(entiers)
            }
            12 => {
                let n = self.longueur(8)?;
                self.prendre(n * 8)?;
                Nbt::Tableau(n)
            }
            _ => return None,
        })
    }
}

/// Pour les tests (ici et dans les autres fichiers) : écrire un petit fichier NBT.
#[cfg(test)]
pub mod ecrire {
    use super::Nbt;
    use std::io::Write;

    fn genre(v: &Nbt) -> u8 {
        match v {
            Nbt::Octet(_) => 1,
            Nbt::Court(_) => 2,
            Nbt::Entier(_) => 3,
            Nbt::Long(_) => 4,
            Nbt::Flottant(_) => 5,
            Nbt::Double(_) => 6,
            Nbt::Texte(_) => 8,
            Nbt::Liste(_) => 9,
            Nbt::Groupe(_) => 10,
            Nbt::Tableau(_) => 7,
            Nbt::Entiers(_) => 11,
        }
    }

    fn texte(sortie: &mut Vec<u8>, t: &str) {
        sortie.extend((t.len() as u16).to_be_bytes());
        sortie.extend(t.as_bytes());
    }

    fn valeur(sortie: &mut Vec<u8>, v: &Nbt) {
        match v {
            Nbt::Octet(n) => sortie.push(*n as u8),
            Nbt::Court(n) => sortie.extend(n.to_be_bytes()),
            Nbt::Entier(n) => sortie.extend(n.to_be_bytes()),
            Nbt::Long(n) => sortie.extend(n.to_be_bytes()),
            Nbt::Flottant(n) => sortie.extend(n.to_be_bytes()),
            Nbt::Double(n) => sortie.extend(n.to_be_bytes()),
            Nbt::Texte(t) => texte(sortie, t),
            Nbt::Liste(l) => {
                sortie.push(l.first().map(genre).unwrap_or(0));
                sortie.extend((l.len() as i32).to_be_bytes());
                l.iter().for_each(|x| valeur(sortie, x));
            }
            Nbt::Groupe(g) => {
                for (k, x) in g {
                    sortie.push(genre(x));
                    texte(sortie, k);
                    valeur(sortie, x);
                }
                sortie.push(0);
            }
            Nbt::Tableau(n) => {
                sortie.extend((*n as i32).to_be_bytes());
                sortie.extend(std::iter::repeat_n(0u8, *n));
            }
            Nbt::Entiers(l) => {
                sortie.extend((l.len() as i32).to_be_bytes());
                l.iter().for_each(|x| sortie.extend(x.to_be_bytes()));
            }
        }
    }

    /// Le fichier compressé en gzip, comme Minecraft l'écrit.
    pub fn fichier(racine: &Nbt) -> Vec<u8> {
        let mut brut = vec![10];
        texte(&mut brut, "");
        valeur(&mut brut, racine);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&brut).unwrap();
        gz.finish().unwrap()
    }

    /// Raccourci : un groupe à partir de paires.
    pub fn groupe(valeurs: Vec<(&str, Nbt)>) -> Nbt {
        Nbt::Groupe(valeurs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::ecrire::{fichier, groupe};
    use super::*;

    #[test]
    fn lit_un_level_dat() {
        let racine = groupe(vec![(
            "Data",
            groupe(vec![
                ("LevelName", Nbt::Texte("Île paradisiaque".into())),
                ("LastPlayed", Nbt::Long(1_794_000_000_000)),
                ("GameType", Nbt::Entier(1)),
                ("Version", groupe(vec![("Name", Nbt::Texte("1.21.4".into())), ("Snapshot", Nbt::Octet(0))])),
                ("GameRules", groupe(vec![("keepInventory", Nbt::Texte("true".into())), ("playersSleepingPercentage", Nbt::Texte("50".into()))])),
                ("DataPacks", groupe(vec![("Enabled", Nbt::Liste(vec![Nbt::Texte("vanilla".into())])), ("Disabled", Nbt::Liste(vec![]))])),
                ("Seeds", Nbt::Tableau(3)),
                ("spawn", groupe(vec![("pos", Nbt::Entiers(vec![12, 64, -340]))])),
                ("Temps", Nbt::Double(1.5)),
            ]),
        )]);
        let lu = lire(&fichier(&racine)).unwrap();
        assert_eq!(lu, racine);
        assert_eq!(lu.chemin(&["Data", "Version", "Name"]).and_then(Nbt::texte), Some("1.21.4"));
        assert_eq!(lu.chemin(&["Data", "LevelName"]).and_then(Nbt::texte), Some("Île paradisiaque"));
        assert_eq!(lu.chemin(&["Data", "GameRules", "keepInventory"]).and_then(Nbt::booleen), Some(true));
        assert_eq!(lu.chemin(&["Data", "GameRules", "playersSleepingPercentage"]).and_then(Nbt::nombre), Some(50));
        assert_eq!(lu.chemin(&["Data", "GameType"]).and_then(Nbt::nombre), Some(1));
        assert!(lu.chemin(&["Data", "Rien"]).is_none());
    }

    #[test]
    fn refuse_un_fichier_abime() {
        let racine = groupe(vec![("a", Nbt::Texte("bonjour".into()))]);
        let mut brut = fichier(&racine);
        assert!(lire(&brut[..brut.len() / 2]).is_none());
        brut.truncate(3);
        assert!(lire(&brut).is_none());
        assert!(lire(b"").is_none());
        // Une liste qui annonce deux milliards d'éléments : refusée sans rien réserver.
        let faux = [10u8, 0, 0, 9, 0, 1, b'l', 3, 0x7f, 0xff, 0xff, 0xff];
        assert!(lire(&faux).is_none());
    }
}
