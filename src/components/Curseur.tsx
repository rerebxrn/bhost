import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import "./Curseur.css";

type Props = {
  min: number;
  max: number;
  valeur: number; // un nombre entier
  onChange: (v: number) => void;
  etiquette: string; // pour les lecteurs d’écran
  unite?: string; // lue par les lecteurs d’écran (« Go »)
};

// Un curseur comme sur iPhone : le bouton suit le doigt en douceur, devient une goutte de verre
// tant qu’on le tient, puis se cale sur la valeur entière la plus proche avec un ressort.
export default function Curseur({ min, max, valeur, onChange, etiquette, unite = "" }: Props) {
  const rail = useRef<HTMLDivElement>(null);
  const [tire, setTire] = useState<number | null>(null); // position libre (0 à 1) pendant qu’on tient le bouton
  const etendue = Math.max(1, max - min);
  const part = tire ?? (Math.min(max, Math.max(min, valeur)) - min) / etendue;

  const depuisPointeur = (x: number) => {
    const r = rail.current?.getBoundingClientRect();
    if (!r) return 0;
    return Math.min(1, Math.max(0, (x - r.left) / r.width));
  };

  // La valeur est envoyée au plus une fois par image : un réglage lourd (le verre) ne bloque plus le geste.
  const enAttente = useRef<number | null>(null);
  const image = useRef(0);
  const derniere = useRef(valeur);
  derniere.current = valeur;
  const envoyer = () => {
    image.current = 0;
    const v = enAttente.current;
    enAttente.current = null;
    if (v !== null && v !== derniere.current) {
      derniere.current = v;
      onChange(v);
    }
  };
  useEffect(() => () => cancelAnimationFrame(image.current), []);

  const suivre = (p: number) => {
    setTire(p);
    enAttente.current = Math.round(min + p * etendue);
    if (!image.current) image.current = requestAnimationFrame(envoyer);
  };

  // Fin du geste (relâché, capture perdue, bouton plus appuyé) : on envoie la dernière valeur.
  const finir = () => {
    if (tire === null) return;
    cancelAnimationFrame(image.current);
    envoyer();
    setTire(null);
  };

  const baisse = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    // Pas de sélection de texte ni de « glisser-déposer » de Windows : ils coupaient le geste en route.
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    e.currentTarget.focus();
    suivre(depuisPointeur(e.clientX));
  };

  const touche = (e: KeyboardEvent) => {
    const pas: Record<string, number> = { ArrowRight: 1, ArrowUp: 1, ArrowLeft: -1, ArrowDown: -1, PageUp: 4, PageDown: -4 };
    let v = valeur;
    if (e.key in pas) v += pas[e.key];
    else if (e.key === "Home") v = min;
    else if (e.key === "End") v = max;
    else return;
    e.preventDefault();
    onChange(Math.min(max, Math.max(min, v)));
  };

  const pourcent = `${(part * 100).toFixed(2)}%`;
  return (
    <div
      ref={rail}
      className={"curseur" + (tire !== null ? " tenu" : "")}
      role="slider"
      tabIndex={0}
      aria-label={etiquette}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={valeur}
      aria-valuetext={`${valeur} ${unite}`.trim()}
      style={{ ["--p" as string]: pourcent, ["--k" as string]: part.toFixed(4) }}
      onPointerDown={baisse}
      onPointerMove={(e) => {
        if (tire === null) return;
        // Le bouton de la souris a été lâché sans qu’on le voie (fenêtre quittée…) : le geste est fini.
        if (e.pointerType === "mouse" && (e.buttons & 1) === 0) return finir();
        suivre(depuisPointeur(e.clientX));
      }}
      onPointerUp={finir}
      onPointerCancel={finir}
      onLostPointerCapture={finir}
      onKeyDown={touche}
    >
      <span className="curseur-rail">
        <i />
      </span>
      <span className="curseur-bouton" />
    </div>
  );
}
