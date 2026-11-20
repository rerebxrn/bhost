import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { IconBolt, IconPlayerPlay, IconPlayerStop, IconRefresh } from "@tabler/icons-react";
import type { Statut } from "../types";
import "./ActionBar.css";

type Props = {
  statut: Statut;
  onDemarrer: () => void;
  onArreter: () => void;
  onRedemarrer: () => void;
  onForcer: () => void;
  surConsole?: boolean; // la capsule flotte au-dessus de la console : verre sombre
  commande?: ReactNode; // la barre de commande « > », à gauche de la capsule
  avecPanneau?: boolean; // un panneau (joueurs) est à droite de la console : on se décale
};

// La capsule d'actions : elle flotte en bas du contenu
// et change de forme selon l'état du serveur.
export default function ActionBar({ statut, onDemarrer, onArreter, onRedemarrer, onForcer, surConsole, commande, avecPanneau }: Props) {
  const dedans = useRef<HTMLDivElement>(null);
  const [largeur, setLargeur] = useState<number>();

  // La capsule prend la largeur de ses boutons, avec une animation en ressort.
  useLayoutEffect(() => {
    const el = dedans.current;
    if (!el) return;
    const mesurer = () => setLargeur(el.offsetWidth);
    mesurer();
    const observateur = new ResizeObserver(mesurer);
    observateur.observe(el);
    return () => observateur.disconnect();
  }, [statut]);

  const occupe = statut === "demarrage" || statut === "arret";
  const forcer = (
    <button type="button" data-gel className="ab ico kill" title="Forcer l'arrêt" aria-label="Forcer l'arrêt" onClick={onForcer}>
      <IconBolt size={17} stroke={2} aria-hidden />
    </button>
  );

  return (
    <div className={"dock" + (surConsole ? " on-console" : "") + (avecPanneau ? " panneau" : "")}>
      {commande}
      <div className="g actbar" data-lens="bar" role="group" aria-label="Actions du serveur" style={{ width: largeur }}>
        {/* key={statut} : le contenu réapparaît en douceur à chaque changement d'état */}
        <div className="act-in" ref={dedans} key={statut}>
          {occupe ? (
            <>
              <span className="busy" role="status">
                <span className="spin" aria-hidden />
                {statut === "demarrage" ? "Démarrage…" : "Arrêt…"}
              </span>
              {forcer}
            </>
          ) : statut === "en-ligne" ? (
            <>
              <button type="button" data-gel className="ab ico" title="Redémarrer" aria-label="Redémarrer" onClick={onRedemarrer}>
                <IconRefresh size={17} stroke={2} aria-hidden />
              </button>
              <button type="button" data-gel className="ab stop" onClick={onArreter}>
                <IconPlayerStop size={15} stroke={2} aria-hidden />
                Arrêter
              </button>
              {forcer}
            </>
          ) : (
            <button type="button" data-gel className="ab go" onClick={onDemarrer}>
              <IconPlayerPlay size={15} stroke={2.2} aria-hidden />
              Démarrer
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
