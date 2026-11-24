import { useState } from "react";
import { IconChevronDown, IconLock, IconSparkles } from "@tabler/icons-react";
import { GROUPES, TOUTES, image, payante } from "../theme/icones";
import { ouvert as plusOuvert, ouvrirPlus, usePlus } from "../plus/etat";
import "../plus/Plus.css";
import "./Menus.css";

type Props = {
  valeur: string;
  onChoisir: (cle: string) => void;
  // Assistant : on ne montre d’abord que les couleurs, avec « Plus d’icônes ».
  repliable?: boolean;
};

// Toutes les icônes de serveur, une ligne par groupe (fenêtre « Icône… » et assistant de création).
export default function ChoixIcone({ valeur, onChoisir, repliable }: Props) {
  // Replié, sauf si l’icône choisie n’est pas une couleur (sinon on ne la verrait pas).
  const [ouvert, setOuvert] = useState(!repliable || !GROUPES[0].icones.some((i) => i.cle === valeur));
  const groupes = ouvert ? GROUPES : GROUPES.slice(0, 1);
  // Sans BHOST+ : les icônes payantes ont un cadenas et ouvrent la fenêtre BHOST+.
  const ferme = !plusOuvert(usePlus());
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
                className={"look-c" + (i.cle === valeur ? " sel" : "") + (ferme && payante(i.cle) ? " paye" : "")}
                onClick={() => (ferme && payante(i.cle) ? ouvrirPlus() : onChoisir(i.cle))}
              >
                <img src={image(i.cle)} alt="" draggable={false} />
                {ferme && payante(i.cle) && (
                  <b className="cad">
                    <IconLock size={10} stroke={2.2} aria-hidden />
                  </b>
                )}
              </button>
            ))}
          </div>
        </div>
      ))}
      {ferme && ouvert && (
        <div className="look-offre">
          <span>
            <IconSparkles size={14} stroke={2} aria-hidden />
            40 icônes en plus avec BHOST+.
          </span>
          <button type="button" onClick={ouvrirPlus}>
            Découvrir
          </button>
        </div>
      )}
      {repliable && (
        <button type="button" className="look-plus" aria-expanded={ouvert} onClick={() => setOuvert(!ouvert)}>
          {ouvert ? "Moins d’icônes" : `Plus d’icônes (${TOUTES.length})`}
          <IconChevronDown size={14} stroke={2} aria-hidden className={ouvert ? "tourne" : ""} />
        </button>
      )}
    </div>
  );
}
