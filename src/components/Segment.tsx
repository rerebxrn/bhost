import { useLayoutEffect, useRef, useState, type PointerEvent } from "react";
import "./Segment.css";

type Props<T extends string> = {
  options: { id: T; nom: string; n?: number }[]; // n : un petit compteur après le nom (Joueurs)
  valeur: T;
  onChoisir: (id: T) => void;
  etiquette: string; // pour les lecteurs d'écran
  off?: boolean; // tout grisé
};

type Tirage = { x0: number; x: number; bouge: boolean };

// Un choix parmi quelques options (Difficulté, Thème…), comme sur iPhone :
// la pastille glisse d'une option à l'autre ; quand on appuie, elle devient une goutte de verre
// qu'on peut tirer, puis elle s'aimante à l'option la plus proche.
export default function Segment<T extends string>({ options, valeur, onChoisir, etiquette, off }: Props<T>) {
  const rangee = useRef<HTMLDivElement>(null);
  const pouceEl = useRef<HTMLSpanElement>(null);
  const tirage = useRef<Tirage | null>(null);
  const [pouce, setPouce] = useState({ x: 0, largeur: 0 });
  const [appui, setAppui] = useState(false);
  const noms = options.map((o) => o.nom).join("|");

  // On mesure l'option choisie pour poser la pastille dessus.
  useLayoutEffect(() => {
    const el = rangee.current;
    if (!el) return;
    const mesurer = () => {
      const b = el.querySelector<HTMLElement>('[aria-checked="true"]');
      if (!b) return;
      setPouce((p) => (p.x === b.offsetLeft && p.largeur === b.offsetWidth ? p : { x: b.offsetLeft, largeur: b.offsetWidth }));
    };
    mesurer();
    const observateur = new ResizeObserver(mesurer);
    observateur.observe(el);
    return () => observateur.disconnect();
  }, [valeur, noms]);

  const placer = (x: number, avecRessort: boolean) => {
    const p = pouceEl.current;
    if (!p) return;
    p.style.transition = avecRessort ? "" : "transform .08s linear, background .2s, box-shadow .2s";
    p.style.transform = `translateX(${x.toFixed(1)}px)`;
  };

  const baisse = (e: PointerEvent<HTMLDivElement>) => {
    if (off || e.button !== 0) return;
    setAppui(true);
    tirage.current = { x0: e.clientX, x: pouce.x, bouge: false };
  };

  const bouge = (e: PointerEvent<HTMLDivElement>) => {
    const t = tirage.current;
    const el = rangee.current;
    if (!t || !el) return;
    // On ne tire la pastille que si on part d'elle, et après 4 px (pour ne pas gêner un simple clic).
    const cadre = el.getBoundingClientRect();
    if (!t.bouge) {
      const depart = t.x0 - cadre.left;
      if (Math.abs(e.clientX - t.x0) <= 4 || depart < pouce.x || depart > pouce.x + pouce.largeur) return;
      t.bouge = true;
      el.setPointerCapture(e.pointerId);
    }
    t.x = Math.max(2, Math.min(cadre.width - pouce.largeur - 2, pouce.x + e.clientX - t.x0));
    placer(t.x, false);
  };

  const leve = () => {
    const t = tirage.current;
    tirage.current = null;
    setAppui(false);
    if (!t?.bouge || !rangee.current) return;
    // L'option dont le centre est le plus proche du centre de la pastille.
    const centre = t.x + pouce.largeur / 2;
    let choix = valeur;
    let ecart = Infinity;
    rangee.current.querySelectorAll<HTMLElement>("button").forEach((b, i) => {
      const d = Math.abs(b.offsetLeft + b.offsetWidth / 2 - centre);
      if (d < ecart) [ecart, choix] = [d, options[i].id];
    });
    placer(pouce.x, true);
    onChoisir(choix);
  };

  return (
    <div
      ref={rangee}
      className={"segc" + (appui ? " appui" : "") + (off ? " off" : "")}
      role="radiogroup"
      aria-label={etiquette}
      aria-disabled={off || undefined}
      onPointerDown={baisse}
      onPointerMove={bouge}
      onPointerUp={leve}
      onPointerCancel={leve}
    >
      <span
        ref={pouceEl}
        className="segc-pouce"
        aria-hidden
        style={{ width: pouce.largeur, transform: `translateX(${pouce.x}px)`, opacity: pouce.largeur ? 1 : 0 }}
      />
      {options.map((o) => (
        <button
          key={o.id}
          type="button"
          role="radio"
          aria-checked={o.id === valeur}
          className={o.id === valeur ? "on" : ""}
          disabled={off}
          data-nom={o.nom}
          onClick={() => !tirage.current?.bouge && onChoisir(o.id)}
        >
          {o.nom}
          {o.n !== undefined && <span className="cnt tnum">{o.n}</span>}
        </button>
      ))}
    </div>
  );
}
