import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { IconCheck, IconSelector } from "@tabler/icons-react";
import "./Menus.css";
import "./Choix.css";

type Props<T extends string> = {
  options: { id: T; nom: string }[];
  valeur: T;
  onChoisir: (id: T) => void;
  etiquette: string; // pour les lecteurs d’écran
};

// Un bouton menu façon macOS (« Chaque jour ⌃⌄ ») : un clic ouvre un menu en verre,
// posé sous le bouton, avec une coche devant le choix actuel.
// Le menu est rendu dans <body> : dans une fenêtre (qui a une transformation), il serait coupé.
export default function Choix<T extends string>({
  options,
  valeur,
  onChoisir,
  etiquette,
}: Props<T>) {
  const bouton = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [ouvert, setOuvert] = useState(false);
  const [place, setPlace] = useState<{
    x: number;
    y: number;
    haut: boolean;
  } | null>(null);
  const actuel = options.find((o) => o.id === valeur);

  // Le menu s’aligne sur le bord droit du bouton ; s’il n’y a pas la place en dessous, il s’ouvre au-dessus.
  useLayoutEffect(() => {
    if (!ouvert || !bouton.current) return;
    const b = bouton.current.getBoundingClientRect();
    const hauteur = menu.current?.offsetHeight ?? options.length * 30 + 10;
    const haut = b.bottom + 4 + hauteur > window.innerHeight - 8;
    setPlace({
      x: window.innerWidth - b.right,
      y: haut ? window.innerHeight - b.top + 4 : b.bottom + 4,
      haut,
    });
  }, [ouvert, options.length]);

  const fermer = () => {
    setOuvert(false);
    setPlace(null);
    bouton.current?.focus();
  };

  // Échap ferme le menu même si le clavier n’est pas dedans (et pas la fenêtre qui est derrière).
  useEffect(() => {
    if (!ouvert) return;
    const touche = (e: KeyboardEvent) => {
      // Une flèche alors que le clavier est resté sur le bouton : on entre dans le menu.
      if ((e.key === "ArrowDown" || e.key === "ArrowUp") && !menu.current?.contains(document.activeElement)) {
        e.preventDefault();
        (menu.current?.querySelector<HTMLButtonElement>("[aria-checked=true]") ?? menu.current?.querySelector("button"))?.focus();
        return;
      }
      if (e.key !== "Escape") return;
      e.stopPropagation();
      e.preventDefault();
      fermer();
    };
    window.addEventListener("keydown", touche, true);
    return () => window.removeEventListener("keydown", touche, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ouvert]);

  return (
    <>
      <button
        ref={bouton}
        type="button"
        className={"choix" + (ouvert ? " on" : "")}
        aria-haspopup="menu"
        aria-expanded={ouvert}
        aria-label={`${etiquette} : ${actuel?.nom ?? ""}`}
        onClick={() => setOuvert(!ouvert)}
      >
        <span>{actuel?.nom ?? "…"}</span>
        <IconSelector size={13} stroke={2} aria-hidden />
      </button>
      {ouvert &&
        createPortal(
          <>
            <div className="ctx-catch choix-catch" onMouseDown={fermer} />
            <div
              ref={menu}
              className={"g ctx choix-menu" + (place?.haut ? " haut" : "")}
              role="menu"
              aria-label={etiquette}
              data-lens="panel"
              style={
                place
                  ? place.haut
                    ? { right: place.x, bottom: place.y }
                    : { right: place.x, top: place.y }
                  : { visibility: "hidden" }
              }
              onKeyDown={(e) => {
                // Échap ferme le menu, pas la fenêtre qui est derrière.
                if (e.key === "Escape") {
                  e.stopPropagation();
                  fermer();
                }
                // Flèches, Début, Fin : on se déplace dans le menu.
                const items = Array.from(menu.current?.querySelectorAll<HTMLButtonElement>("button") ?? []);
                const i = items.indexOf(document.activeElement as HTMLButtonElement);
                const aller = (n: number) => {
                  e.preventDefault();
                  items[(n + items.length) % items.length]?.focus();
                };
                if (e.key === "ArrowDown") aller(i + 1);
                else if (e.key === "ArrowUp") aller(i < 0 ? items.length - 1 : i - 1);
                else if (e.key === "Home") aller(0);
                else if (e.key === "End") aller(items.length - 1);
              }}
            >
              {options.map((o) => (
                <button
                  key={o.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={o.id === valeur}
                  className={
                    "pm-item choix-item" + (o.id === valeur ? " sel" : "")
                  }
                  autoFocus={o.id === valeur}
                  onClick={() => {
                    onChoisir(o.id);
                    fermer();
                  }}
                >
                  <i>
                    <IconCheck size={13} stroke={2.4} aria-hidden />
                  </i>
                  {o.nom}
                </button>
              ))}
            </div>
          </>,
          document.body,
        )}
    </>
  );
}
