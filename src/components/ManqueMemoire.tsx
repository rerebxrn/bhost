import { useRef, useState } from "react";
import { useDialogue } from "./Fenetre";
import type { ManqueMemoire as Manque } from "../api";
import { nombre } from "../types";
import "./Menus.css";
import "./CodeMachine.css";

type Props = {
  nom: string; // le serveur qu'on voulait démarrer
  manque: Manque;
  onArreterEtDemarrer: (id: string) => Promise<void>; // arrête ce serveur-là, puis démarre le nôtre
  onFermer: () => void;
};

const go = (mo: number) => `${nombre(mo / 1024)} Go`;

// Machine dédiée : pas assez de RAM libre pour démarrer ce serveur.
// On montre les serveurs allumés, et on propose d'en arrêter un.
export default function ManqueMemoire({ nom, manque, onArreterEtDemarrer, onFermer }: Props) {
  const boite = useRef<HTMLDivElement>(null);
  const [choix, setChoix] = useState(() => manque.allumes.find((s) => s.ramMo >= manque.manqueMo)?.id ?? manque.allumes[0]?.id ?? "");
  const [envoi, setEnvoi] = useState(false);
  useDialogue(boite, envoi ? null : onFermer);
  const choisi = manque.allumes.find((s) => s.id === choix);

  return (
    <div className="scrim open" onMouseDown={(e) => e.target === e.currentTarget && !envoi && onFermer()}>
      <div ref={boite} tabIndex={-1} className="g dialog code-dlg mm" data-lens="panel" role="alertdialog" aria-modal="true" aria-labelledby="mm-titre">
        <h4 id="mm-titre">Pas assez de mémoire</h4>
        <p className="dlg-txt">
          Il manque {go(manque.manqueMo)} pour démarrer « {nom} ». Arrête un serveur :
        </p>
        <div className="mm-liste" role="radiogroup" aria-label="Serveur à arrêter">
          {manque.allumes.map((s) => (
            <button
              key={s.id}
              type="button"
              role="radio"
              aria-checked={s.id === choix}
              className={"mm-ligne" + (s.id === choix ? " sel" : "")}
              onClick={() => setChoix(s.id)}
            >
              <span className="mm-rond" aria-hidden />
              <span className="mm-nom">{s.nom}</span>
              <span className="mm-ram tnum">{go(s.ramMo)}</span>
            </button>
          ))}
        </div>
        <div className="dlg-btns">
          <button type="button" className="dlg-cancel" disabled={envoi} onClick={onFermer}>
            Annuler
          </button>
          <button
            type="button"
            className="dlg-ok"
            disabled={!choisi || envoi}
            onClick={() => {
              setEnvoi(true);
              onArreterEtDemarrer(choix).finally(onFermer);
            }}
          >
            {envoi ? "Arrêt…" : "Arrêter et démarrer"}
          </button>
        </div>
      </div>
    </div>
  );
}
