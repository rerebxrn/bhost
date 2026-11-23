import { useLayoutEffect, useRef, useState, type PointerEvent } from "react";
import "./Tabs.css";

type Props<T extends string> = {
  onglets: { id: T; nom: string; off?: boolean }[]; // off : onglet grisé (étape pas encore atteinte)
  actif: T;
  onChoisir: (id: T) => void;
  etiquette?: string; // pour les lecteurs d’écran
};

type Tirage = { x0: number; dernier: number; quand: number; bouge: boolean; x: number };

// Les onglets d’un serveur (et les étapes de l’assistant) : une capsule de verre qui flotte en haut du contenu.
// La « goutte » (le fond de l’onglet actif) glisse d’un onglet à l’autre.
// On peut aussi l’attraper et la tirer : elle s’étire selon la vitesse,
// puis s’aimante à l’onglet le plus proche.
export default function Tabs<T extends string>({ onglets, actif, onChoisir, etiquette = "Sections du serveur" }: Props<T>) {
  const rangee = useRef<HTMLDivElement>(null);
  const goutteEl = useRef<HTMLSpanElement>(null);
  const tirage = useRef<Tirage | null>(null);
  const [goutte, setGoutte] = useState({ x: 0, largeur: 0 });
  const noms = onglets.map((o) => o.nom).join("|");

  // On mesure l’onglet actif pour poser la goutte dessus.
  // On remesure si la barre change de taille (police chargée, fenêtre redimensionnée).
  useLayoutEffect(() => {
    const el = rangee.current;
    if (!el) return;
    const mesurer = () => {
      const bouton = el.querySelector<HTMLElement>('[aria-current="page"]');
      if (!bouton) return;
      const x = bouton.offsetLeft;
      const largeur = bouton.offsetWidth;
      setGoutte((g) => (g.x === x && g.largeur === largeur ? g : { x, largeur }));
    };
    mesurer();
    const observateur = new ResizeObserver(mesurer);
    observateur.observe(el);
    return () => observateur.disconnect();
  }, [actif, noms]);

  const appui = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    tirage.current = { x0: e.clientX, dernier: e.clientX, quand: performance.now(), bouge: false, x: goutte.x };
  };

  const mouvement = (e: PointerEvent<HTMLDivElement>) => {
    const t = tirage.current;
    const el = rangee.current;
    const g = goutteEl.current;
    if (!t || !el || !g) return;
    // On commence à tirer après 4 px, pour ne pas gêner un simple clic.
    if (!t.bouge && Math.abs(e.clientX - t.x0) > 4) {
      t.bouge = true;
      el.setPointerCapture(e.pointerId);
    }
    if (!t.bouge) return;
    const maintenant = performance.now();
    const vitesse = Math.min(Math.abs((e.clientX - t.dernier) / Math.max(maintenant - t.quand, 8)), 2);
    t.dernier = e.clientX;
    t.quand = maintenant;
    const cadre = el.getBoundingClientRect();
    t.x = Math.max(4, Math.min(cadre.width - goutte.largeur - 4, e.clientX - cadre.left - goutte.largeur / 2));
    g.classList.add("moving");
    g.style.transition = "transform .09s linear, background .2s, box-shadow .2s";
    g.style.transform = `translateX(${t.x.toFixed(1)}px) scale(${(1.14 + vitesse * 0.2).toFixed(3)},${(1.12 - vitesse * 0.05).toFixed(3)})`;
  };

  const relache = () => {
    const t = tirage.current;
    const el = rangee.current;
    const g = goutteEl.current;
    tirage.current = null;
    if (!t || !t.bouge || !el || !g) return;
    // On cherche l’onglet dont le centre est le plus proche du centre de la goutte.
    const centre = t.x + goutte.largeur / 2;
    let choix = actif;
    let ecart = Infinity;
    el.querySelectorAll<HTMLElement>(".tab").forEach((b, i) => {
      const d = Math.abs(b.offsetLeft + b.offsetWidth / 2 - centre);
      if (d < ecart && !onglets[i].off) {
        ecart = d;
        choix = onglets[i].id;
      }
    });
    g.style.transition = "";
    g.style.transform = `translateX(${goutte.x}px)`;
    window.setTimeout(() => g.classList.remove("moving"), 280);
    onChoisir(choix);
  };

  return (
    <nav className="g tabs" data-lens="bar" aria-label={etiquette}>
      <div
        className="tabs-row"
        ref={rangee}
        onPointerDown={appui}
        onPointerMove={mouvement}
        onPointerUp={relache}
        onPointerCancel={relache}
      >
        <span
          className="drop"
          ref={goutteEl}
          aria-hidden
          style={{ width: goutte.largeur, transform: `translateX(${goutte.x}px)`, opacity: goutte.largeur ? 1 : 0 }}
        />
        {onglets.map((o) => (
          <button
            key={o.id}
            type="button"
            className={"tab" + (o.id === actif ? " on" : "")}
            aria-current={o.id === actif ? "page" : undefined}
            data-nom={o.nom}
            disabled={o.off}
            onClick={() => onChoisir(o.id)}
          >
            {o.nom}
          </button>
        ))}
      </div>
    </nav>
  );
}
