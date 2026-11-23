import { useEffect, useState } from "react";
import { teteJoueur } from "../api";

// Les têtes déjà demandées pendant cette session : on ne repose pas la question à chaque affichage.
const connues = new Map<string, { quand: number; tete: Promise<string | null> }>();
const REESSAI = 10 * 60 * 1000; // sans tête (joueur inconnu, pas d’Internet) : on réessaie dix minutes plus tard

function chercher(nom: string): Promise<string | null> {
  const deja = connues.get(nom);
  if (deja) return deja.tete;
  const tete = teteJoueur(nom).catch(() => null);
  connues.set(nom, { quand: Date.now(), tete });
  tete.then((image) => {
    if (!image) window.setTimeout(() => connues.delete(nom), REESSAI);
  });
  return tete;
}

// La tête d’un joueur : son vrai visage Minecraft, ou sa première lettre en attendant (ou s’il n’en a pas).
export default function Tete({ nom }: { nom: string }) {
  const [image, setImage] = useState<string | null>(null);

  useEffect(() => {
    let ferme = false;
    setImage(null);
    chercher(nom).then((trouvee) => {
      if (!ferme) setImage(trouvee);
    });
    return () => {
      ferme = true;
    };
  }, [nom]);

  if (image) return <img className="av tete" src={image} alt="" />;
  return (
    <span className="av" aria-hidden>
      {nom.replace(/^\./, "").charAt(0).toUpperCase()}
    </span>
  );
}
