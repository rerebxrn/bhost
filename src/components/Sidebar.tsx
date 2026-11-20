import { IconPlus, IconSettings } from "@tabler/icons-react";
import IconeServeur, { couleurDe } from "./IconeServeur";
import { nomLogiciel, texteStatut, type Serveur } from "../types";
import "./Sidebar.css";

type Props = {
  serveurs: Serveur[];
  idActif: string | null; // null quand on n'est pas sur un serveur
  nouveauOuvert: boolean;
  reglagesOuverts: boolean;
  onChoisir: (id: string) => void;
  onMenu: (id: string, x: number, y: number) => void; // clic droit sur un serveur
  onCreer: () => void;
  onReglages: () => void;
};

export default function Sidebar({ serveurs, idActif, nouveauOuvert, reglagesOuverts, onChoisir, onMenu, onCreer, onReglages }: Props) {
  return (
    <aside className="side">
      <div className="sec-title">Mes serveurs</div>

      {serveurs.map((s) => (
        <button
          key={s.id}
          type="button"
          className={"srv" + (s.id === idActif ? " sel" : "")}
          aria-current={s.id === idActif ? "page" : undefined}
          onClick={() => onChoisir(s.id)}
          onContextMenu={(e) => {
            e.preventDefault();
            onMenu(s.id, e.clientX, e.clientY);
          }}
        >
          <IconeServeur couleur={couleurDe(s)} />
          <span className="srv-txt">
            <b>{s.nom}</b>
            <span>
              {nomLogiciel(s)} · {texteStatut(s)}
            </span>
          </span>
        </button>
      ))}

      <button
        type="button"
        className={"new" + (nouveauOuvert ? " sel" : "")}
        aria-current={nouveauOuvert ? "page" : undefined}
        onClick={onCreer}
      >
        <IconPlus size={16} stroke={2} aria-hidden />
        Créer un serveur
      </button>

      <div className="side-foot">
        <button
          type="button"
          className={"side-set" + (reglagesOuverts ? " sel" : "")}
          aria-current={reglagesOuverts ? "page" : undefined}
          onClick={onReglages}
        >
          <IconSettings size={16} stroke={2} aria-hidden />
          Réglages
        </button>
      </div>
    </aside>
  );
}
