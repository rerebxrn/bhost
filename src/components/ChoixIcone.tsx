import { useState } from "react";
import { IconChevronDown } from "@tabler/icons-react";
import { GROUPES, TOUTES, image } from "../theme/icones";
import "./Menus.css";

type Props = {
  valeur: string;
  onChoisir: (cle: string) => void;
  // Assistant : on ne montre d'abord que les couleurs, avec « Plus d'icônes ».
  repliable?: boolean;
};

// Toutes les icônes de serveur, une ligne par groupe (fenêtre « Icône… » et assistant de création).
export default function ChoixIcone({ valeur, onChoisir, repliable }: Props) {
  // Replié, sauf si l'icône choisie n'est pas une couleur (sinon on ne la verrait pas).
  const [ouvert, setOuvert] = useState(!repliable || !GROUPES[0].icones.some((i) => i.cle === valeur));
  const groupes = ouvert ? GROUPES : GROUPES.slice(0, 1);
  return (
    <div className="look-groupes" role="radiogroup" aria-label="Icône">
      {groupes.map((g) => (
        <div className="look-ligne" key={g.nom}>
          <span className="look-nom">{g.nom}</span>
          <div className="look-rang">
            {g.icones.map((i) => (
              <button
                key={i.cle}
                type="button"
                role="radio"
                aria-checked={i.cle === valeur}
                aria-label={i.nom}
                title={i.nom}
                className={"look-c" + (i.cle === valeur ? " sel" : "")}
                onClick={() => onChoisir(i.cle)}
              >
                <img src={image(i.cle)} alt="" draggable={false} />
              </button>
            ))}
          </div>
        </div>
      ))}
      {repliable && (
        <button type="button" className="look-plus" aria-expanded={ouvert} onClick={() => setOuvert(!ouvert)}>
          {ouvert ? "Moins d'icônes" : `Plus d'icônes (${TOUTES.length})`}
          <IconChevronDown size={14} stroke={2} aria-hidden className={ouvert ? "tourne" : ""} />
        </button>
      )}
    </div>
  );
}
