import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { ecouterTravail } from "../api";
import "./Menus.css";
import "./Fenetre.css";

type Props = {
  titre: string;
  occupe?: boolean; // un travail est en cours : on ne ferme pas
  classe?: string; // une classe en plus (taille…)
  onFermer: () => void;
  children: ReactNode;
};

// Une grande fenêtre en verre (Changer de version, Importer un monde, Réinitialiser),
// qui arrive comme celles de macOS : un voile, puis la fenêtre qui grandit avec un ressort.
export default function Fenetre({ titre, occupe = false, classe, onFermer, children }: Props) {
  const [ouvert, setOuvert] = useState(false);
  const boite = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const image = requestAnimationFrame(() => setOuvert(true));
    return () => cancelAnimationFrame(image);
  }, []);
  useDialogue(boite, occupe ? null : onFermer);

  return (
    <div
      className={"scrim" + (ouvert ? " open" : "")}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !occupe) onFermer();
      }}
    >
      <div
        ref={boite}
        tabIndex={-1}
        className={"g dialog wide dlg2" + (classe ? ` ${classe}` : "")}
        data-lens="panel"
        role="dialog"
        aria-modal="true"
        aria-label={titre}
      >
        {children}
      </div>
    </div>
  );
}

// Le clavier d'une fenêtre : à l'ouverture, le focus va dedans (et revient où il était à la fermeture) ;
// Tab reste dedans ; Échap la ferme (`fermer` null : pas maintenant). Un menu ouvert par-dessus garde son Échap.
export function useDialogue(boite: RefObject<HTMLElement | null>, fermer: (() => void) | null) {
  const rappel = useRef(fermer);
  rappel.current = fermer;
  useEffect(() => {
    const avant = document.activeElement as HTMLElement | null;
    const d = boite.current;
    if (d && !d.contains(document.activeElement)) d.focus({ preventScroll: true });
    const touche = (e: KeyboardEvent) => {
      const d = boite.current;
      if (!d) return;
      if (e.key === "Escape") {
        if (!rappel.current) return;
        e.preventDefault();
        rappel.current();
      } else if (e.key === "Tab") {
        const cibles = Array.from(
          d.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), select, textarea, [tabindex="0"], a[href]'),
        ).filter((x) => x.offsetParent !== null);
        if (cibles.length === 0) return e.preventDefault();
        const [premier, dernier] = [cibles[0], cibles[cibles.length - 1]];
        const ici = document.activeElement;
        if (e.shiftKey && (ici === premier || !d.contains(ici))) {
          e.preventDefault();
          dernier.focus();
        } else if (!e.shiftKey && (ici === dernier || !d.contains(ici))) {
          e.preventDefault();
          premier.focus();
        }
      }
    };
    window.addEventListener("keydown", touche);
    return () => {
      window.removeEventListener("keydown", touche);
      if (avant && document.contains(avant)) avant.focus({ preventScroll: true });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}

// L'avancement du long travail en cours sur un serveur (« Sauvegarde », 42 %), ou null.
export function useTravail(id: string, actif: boolean) {
  const [travail, setTravail] = useState<{ etape: string; pourcent: number } | null>(null);
  useEffect(() => {
    if (!actif) {
      setTravail(null);
      return;
    }
    return ecouterTravail((qui, etape, pourcent) => qui === id && setTravail({ etape, pourcent }));
  }, [id, actif]);
  return travail;
}

// La barre d'avancement d'un travail : « Sauvegarde… 42 % ».
export function Avancement({ texte, pourcent }: { texte: string; pourcent: number | null }) {
  return (
    <div className="avance" aria-live="polite">
      <div className="avance-t">
        <span className="spin" aria-hidden />
        <span>{texte}…</span>
        {pourcent !== null && <b className="tnum">{pourcent} %</b>}
      </div>
      <div className="avance-b">
        <i style={{ width: `${pourcent ?? 0}%` }} />
      </div>
    </div>
  );
}

// « 1,2 Go », « 640 Mo », « 12 Ko ».
export function poids(octets: number): string {
  const f = (n: number) => n.toLocaleString("fr-FR", { maximumFractionDigits: n < 10 ? 1 : 0 });
  if (octets >= 1024 ** 3) return `${f(octets / 1024 ** 3)} Go`;
  if (octets >= 1024 ** 2) return `${f(octets / 1024 ** 2)} Mo`;
  return `${Math.max(1, Math.round(octets / 1024))} Ko`;
}

// « joué aujourd’hui », « joué hier », « joué le 12 nov. », « jamais joué ».
export function joue(ms: number | null, maintenant = Date.now()): string {
  if (!ms) return "jamais joué";
  const jour = (t: number) => new Date(t).setHours(0, 0, 0, 0);
  const ecart = Math.round((jour(maintenant) - jour(ms)) / 86_400_000);
  if (ecart <= 0) return "joué aujourd’hui";
  if (ecart === 1) return "joué hier";
  const date = new Date(ms);
  const options: Intl.DateTimeFormatOptions = { day: "numeric", month: "short" };
  if (date.getFullYear() !== new Date(maintenant).getFullYear()) options.year = "numeric";
  return `joué le ${date.toLocaleDateString("fr-FR", options)}`;
}

export const NOMS_MODES: Record<string, string> = {
  survie: "Survie",
  creatif: "Créatif",
  aventure: "Aventure",
  spectateur: "Spectateur",
  hardcore: "Hardcore",
};

// « 1.21.4 » contre « 1.21.11 » : négatif si a est plus ancienne. 0 si on ne peut pas comparer (snapshots).
export function comparerVersions(a: string, b: string): number {
  const n = (v: string) => (/^\d+(\.\d+)*$/.test(v) ? v.split(".").map(Number) : null);
  const [x, y] = [n(a), n(b)];
  if (!x || !y) return 0;
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}
