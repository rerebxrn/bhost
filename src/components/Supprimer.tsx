import { useEffect, useState } from "react";
import "./Menus.css";

type Props = {
  nom: string;
  garder?: boolean; // vrai : « Retirer de BHOST » (le dossier reste sur le PC)
  machine?: boolean; // serveur d’une machine dédiée : pas de Corbeille là-bas
  onSupprimer: () => Promise<void>; // rejetée avec une phrase si Rust refuse
  onFermer: () => void;
};

// La petite fenêtre « Supprimer » ou « Retirer de BHOST ».
// Supprimer : le dossier va à la Corbeille. Retirer : BHOST l’oublie, le dossier ne bouge pas.
export default function Supprimer({ nom, garder = false, machine = false, onSupprimer, onFermer }: Props) {
  const ou = machine ? "la machine" : "ton PC";
  const [erreur, setErreur] = useState("");
  const [envoi, setEnvoi] = useState(false);
  const [ouvert, setOuvert] = useState(false);

  useEffect(() => {
    const image = requestAnimationFrame(() => setOuvert(true));
    return () => cancelAnimationFrame(image);
  }, []);

  const valider = () => {
    setEnvoi(true);
    onSupprimer()
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
        if (e.target === e.currentTarget && !envoi) onFermer();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !envoi) onFermer();
      }}
    >
      <div className="g dialog" data-lens="panel" role="alertdialog" aria-modal="true" aria-labelledby="sup-titre" aria-describedby="sup-texte">
        <h4 id="sup-titre">{garder ? `Retirer « ${nom} » de BHOST ?` : `Supprimer « ${nom} » ?`}</h4>
        <p className="dlg-txt" id="sup-texte">
          {garder ? `Son dossier reste sur ${ou}.` : machine ? "Son dossier sera effacé de la machine." : "Son dossier ira dans la Corbeille."}
        </p>
        {erreur && (
          <p className="dlg-err" role="alert">
            {erreur}
          </p>
        )}
        <div className="dlg-btns">
          <button type="button" className="dlg-cancel" onClick={onFermer} disabled={envoi} autoFocus>
            Annuler
          </button>
          <button type="button" className={"dlg-ok" + (garder ? "" : " danger")} disabled={envoi} onClick={valider}>
            {garder ? "Retirer" : envoi ? "Suppression…" : "Supprimer"}
          </button>
        </div>
      </div>
    </div>
  );
}
