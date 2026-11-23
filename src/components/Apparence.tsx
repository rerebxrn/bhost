import { useEffect, useRef, useState } from "react";
import { useDialogue } from "./Fenetre";
import { couleurDe, type Couleur } from "./IconeServeur";
import { image, nomIcone } from "../theme/icones";
import ChoixIcone from "./ChoixIcone";
import type { Serveur } from "../types";
import "./Menus.css";

type Props = {
  serveur: Serveur;
  onEnregistrer: (couleur: Couleur) => Promise<void>; // rejetée avec une phrase si Rust refuse
  onFermer: () => void;
};

// La fenêtre « Icône » d’un serveur : toutes les icônes, une ligne par groupe.
export default function Apparence({ serveur, onEnregistrer, onFermer }: Props) {
  const [choix, setChoix] = useState<Couleur>(couleurDe(serveur));
  const [erreur, setErreur] = useState("");
  const [envoi, setEnvoi] = useState(false);
  const [ouvert, setOuvert] = useState(false);
  const boite = useRef<HTMLDivElement>(null);
  useDialogue(boite, envoi ? null : onFermer);

  useEffect(() => {
    const tour = requestAnimationFrame(() => setOuvert(true));
    return () => cancelAnimationFrame(tour);
  }, []);

  const enregistrer = () => {
    if (choix === couleurDe(serveur)) return onFermer();
    setEnvoi(true);
    onEnregistrer(choix)
      .then(onFermer)
      .catch((raison) => {
        setErreur(typeof raison === "string" ? raison : "Ça n’a pas marché.");
        setEnvoi(false);
      });
  };

  return (
    <div
      className={"scrim" + (ouvert ? " open" : "")}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onFermer();
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") enregistrer();
      }}
    >
      <div ref={boite} tabIndex={-1} className="g dialog look look2" data-lens="panel" role="dialog" aria-modal="true" aria-label={`Icône de ${serveur.nom}`}>
        <div className="look-tete">
          <img className="look-grand" key={choix} src={image(choix)} alt="" draggable={false} />
          <div>
            <b>{serveur.nom}</b>
            <span>{nomIcone(choix)}</span>
          </div>
        </div>

        <div className="look-zone">
          <ChoixIcone valeur={choix} onChoisir={setChoix} />
        </div>

        {erreur && (
          <p className="dlg-err" role="alert">
            {erreur}
          </p>
        )}
        <div className="dlg-btns look-btns">
          <button type="button" className="dlg-cancel" onClick={onFermer}>
            Annuler
          </button>
          <button type="button" className="dlg-ok" disabled={envoi} onClick={enregistrer}>
            Enregistrer
          </button>
        </div>
      </div>
    </div>
  );
}
