// Les icônes de serveur (dessinées par docs/logo/icones.py, en PNG 192 px dans src/assets/icones : sans grille entre les pixels).
// Rangées par groupe, dans l’ordre de la fenêtre « Icône… » : 10 par groupe.
// Couleurs, Numéros et Serveurs sont gratuits ; les autres groupes viennent avec BHOST+.

const fichiers = import.meta.glob("../assets/icones/*.png", { eager: true, query: "?url", import: "default" }) as Record<string, string>;

export type Icone = { cle: string; nom: string };
export type Groupe = { nom: string; icones: Icone[] };

export const GROUPES: Groupe[] = [
  {
    nom: "Couleurs",
    icones: [
      { cle: "graphite", nom: "Graphite" },
      { cle: "bleu", nom: "Bleu" },
      { cle: "indigo", nom: "Indigo" },
      { cle: "violet", nom: "Violet" },
      { cle: "rose", nom: "Rose" },
      { cle: "rouge", nom: "Rouge" },
      { cle: "orange", nom: "Orange" },
      { cle: "jaune", nom: "Jaune" },
      { cle: "vert", nom: "Vert" },
      { cle: "menthe", nom: "Menthe" },
    ],
  },
  {
    nom: "Numéros",
    icones: [
      { cle: "numero-0", nom: "Numéro 0" },
      { cle: "numero-1", nom: "Numéro 1" },
      { cle: "numero-2", nom: "Numéro 2" },
      { cle: "numero-3", nom: "Numéro 3" },
      { cle: "numero-4", nom: "Numéro 4" },
      { cle: "numero-5", nom: "Numéro 5" },
      { cle: "numero-6", nom: "Numéro 6" },
      { cle: "numero-7", nom: "Numéro 7" },
      { cle: "numero-8", nom: "Numéro 8" },
      { cle: "numero-9", nom: "Numéro 9" },
    ],
  },
  {
    nom: "Serveurs",
    icones: [
      { cle: "serveur-acier", nom: "Acier" },
      { cle: "serveur-titane", nom: "Titane" },
      { cle: "serveur-noir", nom: "Noir" },
      { cle: "serveur-dore", nom: "Doré" },
      { cle: "serveur-orrose", nom: "Or rose" },
      { cle: "serveur-rubis", nom: "Rubis" },
      { cle: "serveur-amethyste", nom: "Améthyste" },
      { cle: "serveur-saphir", nom: "Saphir" },
      { cle: "serveur-diamant", nom: "Diamant" },
      { cle: "serveur-emeraude", nom: "Émeraude" },
    ],
  },
  {
    nom: "Fête",
    icones: [
      { cle: "cadeau-rouge", nom: "Cadeau rouge" },
      { cle: "cadeau-bleu", nom: "Cadeau bleu" },
      { cle: "cadeau-violet", nom: "Cadeau violet" },
      { cle: "cadeau-vert", nom: "Cadeau de Noël" },
      { cle: "pull", nom: "Pull de Noël" },
      { cle: "gateau", nom: "Gâteau d’anniversaire" },
      { cle: "gateau-chocolat", nom: "Gâteau au chocolat" },
      { cle: "bougie", nom: "Bougie" },
      { cle: "citrouille", nom: "Citrouille" },
      { cle: "lanterne", nom: "Lanterne" },
    ],
  },
  {
    nom: "Gourmandises",
    icones: [
      { cle: "burger", nom: "Burger" },
      { cle: "bento", nom: "Bento" },
      { cle: "fromage", nom: "Fromage" },
      { cle: "pain", nom: "Pain de mie" },
      { cle: "pasteque", nom: "Pastèque" },
      { cle: "cookie", nom: "Cookie" },
      { cle: "gaufre", nom: "Gaufre" },
      { cle: "beignet", nom: "Beignet" },
      { cle: "chocolat", nom: "Chocolat" },
      { cle: "guimauve", nom: "Guimauve" },
    ],
  },
  {
    nom: "Objets",
    icones: [
      { cle: "carton", nom: "Carton" },
      { cle: "caisse", nom: "Caisse en bois" },
      { cle: "tresor", nom: "Coffre au trésor" },
      { cle: "livres", nom: "Livres" },
      { cle: "de", nom: "Dé" },
      { cle: "damier", nom: "Damier" },
      { cle: "tele", nom: "Télé rétro" },
      { cle: "camera", nom: "Caméra" },
      { cle: "disquette", nom: "Disquette" },
      { cle: "terminal", nom: "Terminal" },
    ],
  },
  {
    nom: "Nature",
    icones: [
      { cle: "herbe", nom: "Bloc d’herbe" },
      { cle: "buche", nom: "Bûche" },
      { cle: "champignon", nom: "Champignon" },
      { cle: "cactus", nom: "Cactus" },
      { cle: "cerisier", nom: "Cerisier" },
      { cle: "ruche", nom: "Ruche" },
      { cle: "grenouille", nom: "Grenouille" },
      { cle: "plage", nom: "Plage" },
      { cle: "aquarium", nom: "Aquarium" },
      { cle: "hiver", nom: "Hiver" },
    ],
  },
];

export const TOUTES: Icone[] = GROUPES.flatMap((g) => g.icones);

export function image(cle: string): string {
  return fichiers[`../assets/icones/${cle}.png`] ?? fichiers["../assets/icones/graphite.png"];
}

export function nomIcone(cle: string): string {
  return TOUTES.find((i) => i.cle === cle)?.nom ?? "";
}

/** Les groupes gratuits : les autres icônes viennent avec BHOST+. */
const GRATUITS = new Set(["Couleurs", "Numéros", "Serveurs"]);

/** Vrai si l’icône vient avec BHOST+. */
export function payante(cle: string): boolean {
  return !GROUPES.some((g) => GRATUITS.has(g.nom) && g.icones.some((i) => i.cle === cle));
}
