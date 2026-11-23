import { useEffect, useRef, useState, type FormEvent } from "react";
import "./Menus.css";

type Props = {
  nom: string; // le nom actuel
  titre?: string; // « Renommer la machine » ; par défaut, un serveur
  onEnregistrer: (nom: string) => Promise<void>; // rejetée avec une phrase si Rust refuse
  onFermer: () => void;
};

// La petite fenêtre « Renommer le serveur » (ou « Renommer la machine »).
export default function Renommer({ nom, titre = "Renommer le serveur", onEnregistrer, onFermer }: Props) {
  const [valeur, setValeur] = useState(nom);
  const [erreur, setErreur] = useState("");
  const [envoi, setEnvoi] = useState(false);
  const champ = useRef<HTMLInputElement>(null);
  const [ouvert, setOuvert] = useState(false);

  useEffect(() => {
    champ.current?.select();
    // Une image plus tard, pour que l’apparition soit animée.
    const image = requestAnimationFrame(() => setOuvert(true));
    return () => cancelAnimationFrame(image);
  }, []);

  const propre = valeur.trim();
  const valide = propre.length > 0 && propre !== nom && !envoi;

  const enregistrer = (e: FormEvent) => {
    e.preventDefault();
    if (!valide) return;
    setEnvoi(true);
    onEnregistrer(propre)
      .then(onFermer)
      .catch((raison) => {
        setErreur(typeof raison === "string" ? raison : "Ça n’a pas marché.");
        setEnvoi(false);
      });
  };

  return (
    <div
      className={"scrim" + (ouvert ? " open" : "")}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onFermer();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onFermer();
      }}
    >
      <form className="g dialog" data-lens="panel" role="dialog" aria-modal="true" aria-labelledby="ren-titre" onSubmit={enregistrer}>
        <h4 id="ren-titre">{titre}</h4>
        <input
          ref={champ}
          className="field dlg-field"
          type="text"
          value={valeur}
          maxLength={40}
          aria-label="Nouveau nom"
          onChange={(e) => {
            setValeur(e.target.value);
            setErreur("");
          }}
          autoFocus
        />
        {erreur && (
          <p className="dlg-err" role="alert">
            {erreur}
          </p>
        )}
        <div className="dlg-btns">
          <button type="button" className="dlg-cancel" onClick={onFermer}>
            Annuler
          </button>
          <button type="submit" className="dlg-ok" disabled={!valide}>
            Enregistrer
          </button>
        </div>
      </form>
    </div>
  );
}
