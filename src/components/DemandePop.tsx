import { useState } from "react";
import { IconX } from "@tabler/icons-react";
import Tete from "./Tete";
import "./Inviter.css";

type Props = {
  nom: string; // le joueur refusé
  serveur: string; // le nom du serveur
  ouvert: boolean;
  onAccepter: () => Promise<unknown>;
  onRefuser: () => Promise<unknown>;
  onFermer: () => void; // ✕ : la demande reste dans Joueurs › Demandes
};

// Quelqu’un vient d’essayer d’entrer sans être sur la liste blanche : un message en verre, en bas à droite.
export default function DemandePop({ nom, serveur, ouvert, onAccepter, onRefuser, onFermer }: Props) {
  const [enCours, setEnCours] = useState(false);
  const repondre = (action: () => Promise<unknown>) => () => {
    setEnCours(true);
    action().finally(() => setEnCours(false));
  };

  return (
    <div className={"g req" + (ouvert ? " show" : "")} data-lens="panel" role="alert" aria-hidden={!ouvert}>
      <span className="avw grand">
        <Tete nom={nom} />
      </span>
      <div className="req-b">
        <p>
          <b>{nom}</b> veut rejoindre <b>{serveur}</b>.
          <br />
          Il n’est pas sur la liste blanche.
        </p>
        <div className="btns">
          <button type="button" className="ok" data-gel disabled={enCours || !ouvert} onClick={repondre(onAccepter)}>
            Accepter
          </button>
          <button type="button" className="no" data-gel disabled={enCours || !ouvert} onClick={repondre(onRefuser)}>
            Refuser
          </button>
        </div>
      </div>
      <button type="button" className="x" aria-label="Plus tard" title="Plus tard" disabled={!ouvert} onClick={onFermer}>
        <IconX size={14} stroke={2} aria-hidden />
      </button>
    </div>
  );
}
