import { memo } from "react";
import type { Logiciel, Serveur } from "../types";
import { TOUTES, image } from "../theme/icones";

// L’icône d’un serveur : un petit cube en pixels (6 × 6 par face), choisi dans une liste (clic droit › Icône…).
// Les dessins viennent de docs/logo/icones.py ; la liste est dans theme/icones.ts (vérifiée aussi côté Rust).
// Le champ s’appelle encore « couleur » : au début, on ne choisissait que la couleur du cube.
export type Couleur = string;

// Sans choix de l’utilisateur : une couleur selon le logiciel.
const PAR_LOGICIEL: Record<Logiciel, Couleur> = {
  Vanilla: "vert",
  Paper: "bleu",
  Purpur: "violet",
  Spigot: "orange",
  Fabric: "jaune",
  Forge: "graphite",
  NeoForge: "orange",
  Youer: "indigo",
  Arclight: "indigo",
  Mohist: "indigo",
};

export function couleurDe(s: Pick<Serveur, "couleur" | "logiciel">): Couleur {
  return s.couleur && TOUTES.some((i) => i.cle === s.couleur) ? s.couleur : (PAR_LOGICIEL[s.logiciel] ?? "graphite");
}

// L’icône, prête à poser dans la barre latérale (28 px) ou en grand.
function IconeServeur({ couleur, taille = 28 }: { couleur: Couleur; taille?: number }) {
  return (
    <span className="srv-ico" style={{ width: taille, height: taille, flexBasis: taille }}>
      <img key={couleur} src={image(couleur)} alt="" draggable={false} />
    </span>
  );
}

export default memo(IconeServeur);
