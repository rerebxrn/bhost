import { useEffect, useState } from "react";
import { IconCrown, IconMapPin, IconX } from "@tabler/icons-react";
import Fenetre from "./Fenetre";
import Segment from "./Segment";
import Tete from "./Tete";
import { commandeJoueur, ficheJoueur, type Fiche } from "../api";
import { duree } from "../types";
import "./FicheJoueur.css";

type Props = {
  id: string; // le serveur
  nom: string;
  depuis: number | null; // connecté depuis (ms), null s'il n'est pas connecté
  connecte: boolean;
  op: boolean;
  onOp: () => void;
  onExpulser: () => void;
  onBannir: () => void;
  onErreur: (texte: string) => void;
  onInfo: (texte: string) => void;
  onFermer: () => void;
};

const MODES = [
  { id: "survie", nom: "Survie" },
  { id: "creatif", nom: "Créatif" },
  { id: "aventure", nom: "Aventure" },
  { id: "spectateur", nom: "Spectateur" },
];

// « 41 h 20 », « 38 min ».
function tempsDeJeu(secondes: number): string {
  const minutes = Math.floor(secondes / 60);
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, "0")}`;
}

const DIMENSIONS: Record<string, string> = {
  the_nether: "Nether",
  the_end: "End",
};

// La fiche d'un joueur : quelques chiffres (au dernier enregistrement du monde), et les actions.
export default function FicheJoueur(p: Props) {
  const [fiche, setFiche] = useState<Fiche | null | undefined>(undefined); // undefined : lecture
  const [mode, setMode] = useState<string>("survie");

  useEffect(() => {
    ficheJoueur(p.id, p.nom).then(
      (f) => {
        setFiche(f);
        if (f?.mode) setMode(f.mode);
      },
      () => setFiche(null),
    );
  }, [p.id, p.nom]);

  const agir = (action: string, ok: string) =>
    commandeJoueur(p.id, p.nom, action).then(
      () => p.onInfo(ok),
      (e) => p.onErreur(typeof e === "string" ? e : "Ça n'a pas marché."),
    );

  const stats: [string, string][] = fiche
    ? [
        [
          "Temps de jeu",
          fiche.tempsJeu !== null ? tempsDeJeu(fiche.tempsJeu) : "–",
        ],
        [
          "Première venue",
          fiche.premiere
            ? new Date(fiche.premiere).toLocaleDateString("fr-FR", {
                day: "numeric",
                month: "short",
              })
            : "–",
        ],
        ["Morts", fiche.morts !== null ? String(fiche.morts) : "–"],
        [
          "Position",
          fiche.position
            ? fiche.position.map((n) => String(n).replace("-", "−")).join(" · ")
            : "–",
        ],
      ]
    : [];
  const ailleurs = fiche?.dimension ? DIMENSIONS[fiche.dimension] : undefined;
  const depuis = p.depuis ? duree(p.depuis, Date.now()) : "";

  return (
    <Fenetre titre={`Fiche de ${p.nom}`} classe="pf" onFermer={p.onFermer}>
      <div className="pf-h">
        <span className="pf-tete">
          <Tete nom={p.nom} />
        </span>
        <div className="pf-n">
          <b>
            {p.nom}
            {p.op && <span className="op">OP</span>}
          </b>
          <span>
            {p.connecte
              ? depuis
                ? `En ligne depuis ${depuis}`
                : "En ligne"
              : "Hors ligne"}
          </span>
        </div>
        <button
          type="button"
          className="icb"
          aria-label="Fermer"
          onClick={p.onFermer}
        >
          <IconX size={15} stroke={2} aria-hidden />
        </button>
      </div>

      {fiche === undefined ? (
        <div className="rs-vide">
          <span className="spin" aria-hidden /> Lecture…
        </div>
      ) : fiche === null ? (
        <p className="pf-rien">
          Pas encore de fiche : il n'a jamais joué sur ce monde.
        </p>
      ) : (
        <>
          <div className="pf-stats">
            {stats.map(([k, v]) => (
              <div key={k}>
                <span>{k}</span>
                <b className="tnum">{v}</b>
              </div>
            ))}
          </div>
          {ailleurs && <p className="pf-where">Dans le {ailleurs}.</p>}
          <p className="pf-sauve">
            {fiche.enregistre
              ? `Enregistré ${new Date(fiche.enregistre).toLocaleString("fr-FR", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}.`
              : "Au dernier enregistrement du monde."}
          </p>
        </>
      )}

      <div className="pf-acts">
        <Segment
          etiquette="Mode de jeu"
          options={MODES}
          valeur={mode}
          off={!p.connecte}
          onChoisir={(m) => {
            setMode(m);
            agir(
              `mode:${m}`,
              `${p.nom} en ${MODES.find((x) => x.id === m)?.nom.toLowerCase()}`,
            );
          }}
        />
        <div className="pf-btns">
          <button
            type="button"
            className="btn"
            disabled={!p.connecte}
            onClick={() => agir("apparition", `${p.nom} au point d’apparition`)}
          >
            <IconMapPin size={14} stroke={2} aria-hidden />
            Au point d’apparition
          </button>
          <button type="button" className="btn" onClick={p.onOp}>
            <IconCrown size={14} stroke={2} aria-hidden />
            {p.op ? "Retirer l’OP" : "Donner l’OP"}
          </button>
          <button
            type="button"
            className="btn"
            disabled={!p.connecte}
            onClick={p.onExpulser}
          >
            Expulser
          </button>
          <button type="button" className="btn danger" onClick={p.onBannir}>
            Bannir
          </button>
        </div>
      </div>
    </Fenetre>
  );
}
