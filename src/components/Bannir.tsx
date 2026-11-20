import { useEffect, useState } from "react";
import { IconBan } from "@tabler/icons-react";
import "./Menus.css";

type Props = {
  nom: string;
  onBannir: (motif: string) => Promise<void>; // rejetée avec une phrase si Rust refuse
  onFermer: () => void;
};

// La petite fenêtre « Bannir… » : un motif facultatif, puis le joueur ne peut plus revenir.
export default function Bannir({ nom, onBannir, onFermer }: Props) {
  const [motif, setMotif] = useState("");
  const [erreur, setErreur] = useState("");
  const [envoi, setEnvoi] = useState(false);
  const [ouvert, setOuvert] = useState(false);

  useEffect(() => {
    const image = requestAnimationFrame(() => setOuvert(true));
    return () => cancelAnimationFrame(image);
  }, []);

  const valider = () => {
    setEnvoi(true);
    onBannir(motif.trim())
      .then(onFermer)
      .catch((raison) => {
        setErreur(typeof raison === "string" ? raison : "Ça n'a pas marché.");
        setEnvoi(false);
      });
  };

  return (
    <div
      className={"scrim" + (ouvert ? " open" : "")}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !envoi) onFermer();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !envoi) onFermer();
      }}
    >
      <div className="g dialog" data-lens="panel" role="alertdialog" aria-modal="true" aria-labelledby="ban-titre">
        <IconBan size={30} stroke={2} className="ban-ic" aria-hidden />
        <h4 id="ban-titre">Bannir {nom} ?</h4>
        <p className="dlg-txt">Il ne pourra plus revenir.</p>
        <input
          className="field dlg-field"
          type="text"
          maxLength={100}
          value={motif}
          placeholder="Motif (facultatif) : triche…"
          aria-label="Motif du bannissement"
          autoFocus
          onChange={(e) => setMotif(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !envoi && valider()}
        />
        {erreur && (
          <p className="dlg-err" role="alert">
            {erreur}
          </p>
        )}
        <div className="dlg-btns">
          <button type="button" className="dlg-cancel" onClick={onFermer} disabled={envoi}>
            Annuler
          </button>
          <button type="button" className="dlg-ok danger" disabled={envoi} onClick={valider}>
            Bannir
          </button>
        </div>
      </div>
    </div>
  );
}
