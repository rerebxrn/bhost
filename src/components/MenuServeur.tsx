import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { IconArrowRight, IconCopy, IconCube, IconFolder, IconPencil, IconTrash, IconUnlink } from "@tabler/icons-react";
import "./Menus.css";

type Props = {
  x: number; // là où on a cliqué (dans la fenêtre)
  y: number;
  nom: string;
  onFermer: () => void;
  onOuvrir: () => void;
  onRenommer: () => void;
  onApparence: () => void;
  onDossier: () => void;
  onDupliquer: () => void;
  onRetirer: () => void;
  onSupprimer: () => void;
  allume: boolean; // un serveur allumé ne peut être ni retiré ni supprimé
};

const MARGE = 8; // le menu ne touche jamais le bord de la fenêtre

// Le menu du clic droit sur un serveur, dans la barre latérale.
export default function MenuServeur({ x, y, nom, onFermer, onOuvrir, onRenommer, onApparence, onDossier, onDupliquer, onRetirer, onSupprimer, allume }: Props) {
  const menu = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState({ x, y });

  // S'il dépasse en bas ou à droite, on le décale pour qu'il reste dans la fenêtre.
  useLayoutEffect(() => {
    const el = menu.current;
    if (!el) return;
    const { offsetWidth: l, offsetHeight: h } = el;
    setPlace({
      x: Math.max(MARGE, Math.min(x, window.innerWidth - l - MARGE)),
      y: Math.max(MARGE, Math.min(y, window.innerHeight - h - MARGE)),
    });
    el.querySelector<HTMLButtonElement>("button")?.focus();
  }, [x, y]);

  // La fenêtre change de taille ou perd la main : on ferme.
  useEffect(() => {
    window.addEventListener("resize", onFermer);
    window.addEventListener("blur", onFermer);
    return () => {
      window.removeEventListener("resize", onFermer);
      window.removeEventListener("blur", onFermer);
    };
  }, [onFermer]);

  // Clavier : Échap ferme, les flèches passent d'un choix à l'autre.
  const touche = (e: KeyboardEvent) => {
    if (e.key === "Escape") return onFermer();
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const choix = [...(menu.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
    const i = choix.indexOf(document.activeElement as HTMLButtonElement);
    choix[(i + (e.key === "ArrowDown" ? 1 : choix.length - 1)) % choix.length]?.focus();
  };

  const choisir = (action: () => void) => () => {
    onFermer();
    action();
  };

  return (
    <>
      <div
        className="ctx-catch"
        onClick={onFermer}
        onContextMenu={(e) => {
          e.preventDefault();
          onFermer();
          // Clic droit sur un autre serveur : son menu s'ouvre tout de suite (pas besoin d'un 2e clic).
          const { clientX, clientY } = e;
          const sous = document.elementsFromPoint(clientX, clientY).find((el) => !el.classList.contains("ctx-catch") && !menu.current?.contains(el));
          const serveur = sous?.closest(".srv");
          if (serveur) serveur.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX, clientY }));
        }}
      />
      <div
        ref={menu}
        className="g ctx"
        data-lens="panel"
        role="menu"
        aria-label={nom}
        style={{ left: place.x, top: place.y }}
        onKeyDown={touche}
      >
        <button type="button" role="menuitem" className="pm-item" onClick={choisir(onOuvrir)}>
          <IconArrowRight size={16} stroke={2} aria-hidden />
          Ouvrir
        </button>
        <button type="button" role="menuitem" className="pm-item" onClick={choisir(onRenommer)}>
          <IconPencil size={16} stroke={2} aria-hidden />
          Renommer…
        </button>
        <button type="button" role="menuitem" className="pm-item" onClick={choisir(onApparence)}>
          <IconCube size={16} stroke={2} aria-hidden />
          Icône…
        </button>
        <button
          type="button"
          role="menuitem"
          className="pm-item"
          disabled={allume}
          title={allume ? "Arrête-le d'abord" : undefined}
          onClick={choisir(onDupliquer)}
        >
          <IconCopy size={16} stroke={2} aria-hidden />
          Dupliquer
        </button>
        <button type="button" role="menuitem" className="pm-item" onClick={choisir(onDossier)}>
          <IconFolder size={16} stroke={2} aria-hidden />
          Ouvrir le dossier
        </button>
        <div className="pm-sep" role="separator" />
        <button
          type="button"
          role="menuitem"
          className="pm-item"
          disabled={allume}
          title={allume ? "Arrête-le d'abord" : undefined}
          onClick={choisir(onRetirer)}
        >
          <IconUnlink size={16} stroke={2} aria-hidden />
          Retirer de BHOST…
        </button>
        <button
          type="button"
          role="menuitem"
          className="pm-item danger"
          disabled={allume}
          title={allume ? "Arrête-le d'abord" : undefined}
          onClick={choisir(onSupprimer)}
        >
          <IconTrash size={16} stroke={2} aria-hidden />
          Supprimer…
        </button>
      </div>
    </>
  );
}
