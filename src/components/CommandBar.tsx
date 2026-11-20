import { useState, type FormEvent, type KeyboardEvent } from "react";
import { IconArrowUp } from "@tabler/icons-react";

type Props = {
  actif: boolean; // faux quand le serveur est arrêté
  onEnvoyer: (commande: string) => void;
};

// La barre de commande : une capsule de verre où on tape une commande du serveur.
// Flèche haut / bas : retrouver les commandes déjà envoyées.
export default function CommandBar({ actif, onEnvoyer }: Props) {
  const [texte, setTexte] = useState("");
  const [historique, setHistorique] = useState<string[]>([]);
  const [rang, setRang] = useState(-1); // -1 : on tape une nouvelle commande

  const envoyer = (e: FormEvent) => {
    e.preventDefault();
    const commande = texte.trim();
    if (!commande || !actif) return;
    onEnvoyer(commande);
    setHistorique((h) => [commande, ...h.filter((c) => c !== commande)].slice(0, 50));
    setRang(-1);
    setTexte("");
  };

  const touche = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    const suivant = Math.max(-1, Math.min(historique.length - 1, rang + (e.key === "ArrowUp" ? 1 : -1)));
    setRang(suivant);
    setTexte(suivant === -1 ? "" : historique[suivant]);
  };

  return (
    <form className="g cmdbar" data-lens="bar" onSubmit={envoyer}>
      <span className="pr" aria-hidden>
        &gt;
      </span>
      <input
        type="text"
        value={texte}
        disabled={!actif}
        placeholder={actif ? "list, say Bonjour, op Noé, kick Léa…" : "Démarre le serveur pour taper une commande"}
        aria-label="Commande du serveur"
        autoComplete="off"
        spellCheck={false}
        onChange={(e) => setTexte(e.target.value)}
        onKeyDown={touche}
      />
      <button type="submit" className="send" data-gel aria-label="Envoyer" disabled={!actif || !texte.trim()}>
        <IconArrowUp size={16} stroke={2.2} aria-hidden />
      </button>
    </form>
  );
}
