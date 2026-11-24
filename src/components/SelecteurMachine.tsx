import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { IconCheck, IconDeviceDesktop, IconPlus, IconSelector, IconServer2 } from "@tabler/icons-react";
import type { Machine } from "../types";
import "./Menus.css";
import "./Choix.css";

const ETATS: Record<Machine["etat"], string> = {
  "en-ligne": "En ligne",
  "hors-ligne": "Hors ligne",
  connexion: "Connexion…",
};

type Props = {
  machines: Machine[];
  active: string; // « ici » (ce PC) ou l’id d’une machine dédiée
  onChoisir: (id: string) => void;
  onAjouter: () => void;
};

// Le choix de la machine, dans la barre de titre à droite du logo (décidé le 22/11).
// Toujours là, même sans machine dédiée (« Ce PC ») : ajouter une machine passe par son menu.
export default function SelecteurMachine({ machines, active, onChoisir, onAjouter }: Props) {
  const bouton = useRef<HTMLButtonElement>(null);
  const [place, setPlace] = useState<{ x: number; y: number } | null>(null);
  const [ouvert, setOuvert] = useState(false);
  const m = machines.find((x) => x.id === active);
  const nom = m ? m.nom : "Ce PC";

  useLayoutEffect(() => {
    if (!ouvert || !bouton.current) return;
    const b = bouton.current.getBoundingClientRect();
    setPlace({ x: b.left, y: b.bottom + 6 });
  }, [ouvert]);

  const fermer = () => {
    setOuvert(false);
    setPlace(null);
  };

  useEffect(() => {
    if (!ouvert) return;
    const touche = (e: KeyboardEvent) => e.key === "Escape" && fermer();
    window.addEventListener("keydown", touche);
    window.addEventListener("blur", fermer);
    return () => {
      window.removeEventListener("keydown", touche);
      window.removeEventListener("blur", fermer);
    };
  }, [ouvert]);

  const item = (id: string, texte: string, icone: ReactNode, etat?: Machine["etat"]) => (
    <button
      key={id}
      type="button"
      role="menuitemradio"
      aria-checked={id === active}
      className={"pm-item choix-item sm-item" + (id === active ? " sel" : "")}
      title={texte}
      onClick={() => {
        onChoisir(id);
        fermer();
      }}
    >
      <i>
        <IconCheck size={13} stroke={2.4} aria-hidden />
      </i>
      {icone}
      <span className="sm-nom">{texte}</span>
      {etat && <span className={"sm-point " + etat} aria-label={ETATS[etat]} />}
    </button>
  );

  return (
    <>
      <button
        ref={bouton}
        type="button"
        className={"mach" + (ouvert ? " on" : "")}
        aria-haspopup="menu"
        aria-expanded={ouvert}
        aria-label={`Machine : ${nom}${m ? ` (${ETATS[m.etat]})` : ""}`}
        onClick={() => (ouvert ? fermer() : setOuvert(true))}
      >
        <span className="ic">{m ? <IconServer2 size={15} stroke={1.8} aria-hidden /> : <IconDeviceDesktop size={15} stroke={1.8} aria-hidden />}</span>
        <span className="nm">{nom}</span>
        {m && <i className={"sm-point " + m.etat} aria-hidden />}
        <span className="fl">
          <IconSelector size={13} stroke={2} aria-hidden />
        </span>
      </button>
      {ouvert &&
        createPortal(
          <>
            <div className="ctx-catch choix-catch" onMouseDown={fermer} />
            <div
              className="g ctx choix-menu sm-menu mach-menu"
              role="menu"
              aria-label="Machine"
              data-lens="panel"
              style={place ? { left: place.x, top: place.y } : { visibility: "hidden" }}
            >
              {item("ici", "Ce PC", <IconDeviceDesktop size={15} stroke={1.8} aria-hidden />)}
              {machines.map((x) => item(x.id, x.nom, <IconServer2 size={15} stroke={1.8} aria-hidden />, x.etat))}
              <div className="pm-sep" />
              <button
                type="button"
                role="menuitem"
                className="pm-item choix-item sm-item"
                onClick={() => {
                  fermer();
                  onAjouter();
                }}
              >
                <i />
                <IconPlus size={15} stroke={1.8} aria-hidden />
                <span className="sm-nom">Ajouter une machine…</span>
              </button>
            </div>
          </>,
          document.body,
        )}
    </>
  );
}
