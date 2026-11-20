import { useEffect, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import { createPortal } from "react-dom";
import { IconCopy, IconPlus, IconUserPlus, IconX } from "@tabler/icons-react";
import { actionJoueur, joueursServeur, type Joueurs } from "../api";
import type { Acces, Serveur } from "../types";
import "./Menus.css";
import "./Inviter.css";

type Props = {
  serveur: Serveur;
  acces?: Acces;
  ancre: DOMRect; // le bouton « Inviter » : le panneau s'ouvre dessous, aligné à droite
  onFermer: () => void;
  onChange?: (listes: Joueurs) => void; // la liste blanche a changé
  onInfo: (texte: string) => void;
  onErreur: (texte: string) => void;
};

const MARGE = 8;
const LARGEUR = 360;
const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n'a pas marché.");

// « Inviter des amis » (d'après la maquette) : l'adresse, un message prêt à envoyer,
// et l'ajout à la liste blanche quand elle est active.
// Un panneau de verre qui s'ouvre en ressort sous le bouton (rendu dans <body>).
export default function Inviter({ serveur, acces, ancre, onFermer, onChange, onInfo, onErreur }: Props) {
  const panneau = useRef<HTMLDivElement>(null);
  const [listes, setListes] = useState<Joueurs | null>(null);
  const [pseudo, setPseudo] = useState("");
  const [enCours, setEnCours] = useState(false);
  const [sortie, setSortie] = useState(false);
  const [place, setPlace] = useState({ x: 0, y: 0 });

  useEffect(() => {
    joueursServeur(serveur.id).then(setListes, (e) => onErreur(phrase(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id]);

  // Sous le bouton, aligné sur son bord droit, sans sortir de la fenêtre.
  useLayoutEffect(() => {
    const h = panneau.current?.offsetHeight ?? 300;
    setPlace({
      x: Math.max(MARGE, Math.min(ancre.right - LARGEUR, window.innerWidth - LARGEUR - MARGE)),
      y: Math.max(MARGE, Math.min(ancre.bottom + 8, window.innerHeight - h - MARGE)),
    });
  }, [ancre, listes]);

  const fermer = () => {
    setSortie(true);
    window.setTimeout(onFermer, 160);
  };

  useEffect(() => {
    const touche = (e: KeyboardEvent) => e.key === "Escape" && fermer();
    window.addEventListener("keydown", touche);
    window.addEventListener("resize", onFermer);
    return () => {
      window.removeEventListener("keydown", touche);
      window.removeEventListener("resize", onFermer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const ajouter = (e: FormEvent) => {
    e.preventDefault();
    const nom = pseudo.trim();
    if (nom.length < 3) return;
    setEnCours(true);
    actionJoueur(serveur.id, "blanc_ajouter", nom)
      .then((l) => {
        setListes(l);
        setPseudo("");
        onChange?.(l);
        onInfo(`${nom} peut entrer`);
      })
      .catch((er) => onErreur(phrase(er)))
      .finally(() => setEnCours(false));
  };

  // L'adresse pour les amis (Paramètres › Ouvert à tes amis).
  const pourAmis = serveur.ouvertAmis ? acces : undefined;
  const publique = pourAmis?.etat === "ouvert" ? pourAmis.adresse : undefined;
  const sansAdresse = !serveur.ouvertAmis
    ? "Ouvre-le à tes amis dans Paramètres."
    : pourAmis?.etat === "erreur"
      ? "Fermé · voir Paramètres"
      : "Ouverture de ta box…";
  const message = publique
    ? `Viens jouer sur mon serveur Minecraft « ${serveur.nom} » ! Adresse : ${publique}` + (serveur.version ? ` · Version ${serveur.version}` : "")
    : "";
  const copier = (texte: string, info: string) => navigator.clipboard.writeText(texte).then(() => onInfo(info));

  return createPortal(
    <>
      <div className="ctx-catch inv-catch" onMouseDown={fermer} />
      <div
        ref={panneau}
        className={"g inv" + (sortie ? " sortie" : "")}
        data-lens="panel"
        role="dialog"
        aria-label="Inviter des amis"
        style={{ left: place.x, top: place.y, width: LARGEUR }}
      >
        <div className="inv-h">
          <IconUserPlus size={18} stroke={2} aria-hidden />
          Inviter des amis
          <button type="button" className="x" aria-label="Fermer" onClick={fermer}>
            <IconX size={16} stroke={2} aria-hidden />
          </button>
        </div>
        <p className="inv-intro">À coller dans Multijoueur › Ajouter un serveur.</p>

        <div className="inv-sec2">Adresse du serveur</div>
        {publique ? (
          <div className="inv-box">
            <code>{publique}</code>
            <button type="button" className="btn tint" onClick={() => copier(publique, `« ${publique} » copié`)}>
              <IconCopy size={13} stroke={2} aria-hidden />
              Copier
            </button>
          </div>
        ) : (
          <div className="inv-box off">
            <span>{sansAdresse}</span>
          </div>
        )}

        {publique && (
          <>
            <div className="inv-sec2">Message prêt à envoyer</div>
            <div className="inv-box msg">
              <span>{message}</span>
              <button type="button" className="btn tint" onClick={() => copier(message, "Message copié")}>
                <IconCopy size={13} stroke={2} aria-hidden />
                Copier
              </button>
            </div>
          </>
        )}

        {listes?.listeBlanche && (
          <>
            <div className="inv-sec2">Ajouter à la liste blanche</div>
            <form className="inv-wl" onSubmit={ajouter}>
              <label>
                <IconPlus size={14} stroke={2} aria-hidden />
                <input
                  type="text"
                  value={pseudo}
                  maxLength={17}
                  placeholder="Son pseudo Minecraft"
                  aria-label="Pseudo à ajouter à la liste blanche"
                  spellCheck={false}
                  onChange={(e) => setPseudo(e.target.value)}
                />
              </label>
              <button type="submit" className="btn acc" disabled={pseudo.trim().length < 3 || enCours}>
                Ajouter
              </button>
            </form>
          </>
        )}
        <div className="inv-fin" />
      </div>
    </>,
    document.body,
  );
}
