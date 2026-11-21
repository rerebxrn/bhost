// Les icônes de serveur (dessinées par docs/logo/icones.py, en PNG 192 px dans src/assets/icones : sans grille entre les pixels).
// Rangées par groupe, dans l'ordre de la fenêtre « Icône… ».

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
      { cle: "serveur-dore", nom: "Doré" },
      { cle: "serveur-titane", nom: "Titane" },
      { cle: "serveur-noir", nom: "Noir" },
      { cle: "serveur-orrose", nom: "Or rose" },
    ],
  },
  {
    nom: "Fête",
    icones: [
      { cle: "cadeau-rouge", nom: "Cadeau rouge" },
      { cle: "cadeau-bleu", nom: "Cadeau bleu" },
      { cle: "cadeau-vert", nom: "Cadeau de Noël" },
      { cle: "gateau", nom: "Gâteau d'anniversaire" },
      { cle: "gateau-chocolat", nom: "Gâteau au chocolat" },
    ],
  },
  {
    nom: "Gourmandises",
    icones: [
      { cle: "fromage", nom: "Fromage" },
      { cle: "pain", nom: "Pain de mie" },
      { cle: "guimauve", nom: "Guimauve" },
      { cle: "bento", nom: "Bento" },
    ],
  },
  {
    nom: "Objets",
    icones: [
      { cle: "carton", nom: "Carton" },
      { cle: "de", nom: "Dé" },
      { cle: "camera", nom: "Caméra" },
      { cle: "terminal", nom: "Terminal" },
      { cle: "damier", nom: "Damier" },
      { cle: "caisse", nom: "Caisse en bois" },
    ],
  },
  {
    nom: "Nature",
    icones: [
      { cle: "herbe", nom: "Bloc d'herbe" },
      { cle: "hiver", nom: "Hiver" },
      { cle: "aquarium", nom: "Aquarium" },
      { cle: "grenouille", nom: "Grenouille" },
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
