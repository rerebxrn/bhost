import { useState } from "react";
import { IconCircleCheck, IconRefresh } from "@tabler/icons-react";
import Fenetre, { Avancement, useTravail } from "./Fenetre";
import { reinitialiserMonde, type Monde } from "../api";

type Props = {
  id: string; // le serveur
  monde: Monde;
  onFini: (mondes: Monde[]) => void;
  onFermer: () => void;
};

const OPTIONS = [
  { meme: true, nom: "Même graine", desc: "Le même terrain, mais tout neuf." },
  { meme: false, nom: "Nouvelle graine", desc: "Un tout autre monde." },
];

// « Réinitialiser » un monde : sauvegarde, puis le serveur le recrée.
export default function Reinitialiser({ id, monde, onFini, onFermer }: Props) {
  const [meme, setMeme] = useState(true);
  const [envoi, setEnvoi] = useState(false);
  const [erreur, setErreur] = useState("");
  const travail = useTravail(id, envoi);

  const valider = () => {
    setEnvoi(true);
    setErreur("");
    reinitialiserMonde(id, monde.dossier, meme)
      .then(onFini)
      .catch((e) => {
        setErreur(typeof e === "string" ? e : "Ça n’a pas marché.");
        setEnvoi(false);
      });
  };

  return (
    <Fenetre titre="Réinitialiser un monde" occupe={envoi} onFermer={onFermer}>
      <span className="dlg-ic red">
        <IconRefresh size={26} stroke={1.8} aria-hidden />
      </span>
      <h4>Réinitialiser « {monde.nom} » ?</h4>
      <p className="dlg-txt">Le reste ne bouge pas.</p>
      {envoi ? (
        <Avancement texte={travail?.etape ?? "Préparation"} pourcent={travail?.pourcent ?? null} />
      ) : (
        <>
          <div className="rs-opts" role="radiogroup" aria-label="Graine">
            {OPTIONS.map((o) => (
              <button key={o.nom} type="button" role="radio" aria-checked={o.meme === meme} className={"rs-opt" + (o.meme === meme ? " sel" : "")} onClick={() => setMeme(o.meme)}>
                <span className="radio" />
                <span className="rs-t">
                  <b>{o.nom}</b>
                  <span>{o.desc}</span>
                </span>
              </button>
            ))}
          </div>
          <div className="rs-safe">
            <IconCircleCheck size={15} stroke={2} aria-hidden />
            <span>Sauvegarde faite avant.</span>
          </div>
        </>
      )}
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
          Réinitialiser
        </button>
      </div>
    </Fenetre>
  );
}
