import { useRef } from "react";
import { useDialogue } from "./Fenetre";
import "./Menus.css";

type Props = {
  titre: string;
  texte: string;
  bouton: string; // « Retirer quand même »
  onOui: () => void;
  onFermer: () => void;
};

// Une petite fenêtre « Tu es sûr ? » avant une action qui peut faire du dégât (bouton rouge).
export default function Confirmer({ titre, texte, bouton, onOui, onFermer }: Props) {
  const boite = useRef<HTMLDivElement>(null);
  useDialogue(boite, onFermer);
  return (
    <div
      className="scrim open"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onFermer();
      }}
    >
      <div ref={boite} tabIndex={-1} className="g dialog" data-lens="panel" role="alertdialog" aria-modal="true" aria-labelledby="conf-titre" aria-describedby="conf-texte">
        <h4 id="conf-titre">{titre}</h4>
        <p className="dlg-txt" id="conf-texte">
          {texte}
        </p>
        <div className="dlg-btns">
          <button type="button" className="dlg-cancel" onClick={onFermer} autoFocus>
            Annuler
          </button>
          <button
            type="button"
            className="dlg-ok danger"
            onClick={() => {
              onFermer();
              onOui();
            }}
          >
            {bouton}
          </button>
        </div>
      </div>
    </div>
  );
}
