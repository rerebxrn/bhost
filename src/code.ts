// Le code d'une machine dédiée : 8 caractères, lettres majuscules et chiffres (« K7M2-Q9XR »).
// Pas de 0, O, 1 ni I : on ne peut pas les confondre en le recopiant.
// La machine l'affiche au premier démarrage et le garde dans ses réglages.
export const ALPHABET = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
export const LONGUEUR = 8;

// Ce que la personne tape, nettoyé : majuscules, sans tiret ni espace, 8 caractères au plus.
export function nettoyerCode(texte: string): string {
  return texte
    .toUpperCase()
    .split("")
    .filter((c) => ALPHABET.includes(c))
    .join("")
    .slice(0, LONGUEUR);
}

// « K7M2Q9XR » → « K7M2-Q9XR » (le tiret n'apparaît qu'après le 4ᵉ caractère).
export function afficherCode(code: string): string {
  return code.length > 4 ? `${code.slice(0, 4)}-${code.slice(4)}` : code;
}

export const codeComplet = (code: string) => code.length === LONGUEUR;
