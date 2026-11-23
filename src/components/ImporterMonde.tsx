import { useEffect, useState } from "react";
import Fenetre, { Avancement, NOMS_MODES, joue, poids, useTravail } from "./Fenetre";
import { choisirDossier, choisirZip, importerMonde, mondesSolo, type Monde, type MondeSolo } from "../api";
import type { Serveur } from "../types";

type Props = {
  serveur: Serveur;
  onFini: (mondes: Monde[], nom: string, actuel: boolean) => void;
  onFermer: () => void;
};

// « Importer un monde… » : un monde du jeu solo, un autre dossier, ou une archive .zip. Le monde est copié.
export default function ImporterMonde({ serveur, onFini, onFermer }: Props) {
  const [solo, setSolo] = useState<MondeSolo[] | null>(null);
  const [choix, setChoix] = useState<{ chemin: string; nom: string } | null>(null);
  const [autre, setAutre] = useState<{ chemin: string; nom: string } | null>(null); // dossier ou .zip choisi à la main
  const [actuel, setActuel] = useState(false);
  const [envoi, setEnvoi] = useState(false);
  const [erreur, setErreur] = useState("");
  const travail = useTravail(serveur.id, envoi);

  useEffect(() => {
    mondesSolo(serveur.id, serveur.version).then(
      (l) => {
        setSolo(l);
        const premier = l.find((m) => !m.tropRecent);
        if (premier) setChoix({ chemin: premier.chemin, nom: premier.nom });
      },
      () => setSolo([]),
    );
  }, [serveur.id]);

  const prendre = async (zip: boolean) => {
    const chemin = zip ? await choisirZip() : await choisirDossier("Choisis le dossier du monde");
    if (!chemin) return;
    const nom = chemin.split(/[\\/]/).pop() || chemin;
    setAutre({ chemin, nom });
    setChoix({ chemin, nom });
  };

  const valider = () => {
    if (!choix) return;
    setEnvoi(true);
    setErreur("");
    importerMonde(serveur.id, choix.chemin, actuel)
      .then((mondes) => onFini(mondes, choix.nom, actuel))
      .catch((e) => {
        setErreur(typeof e === "string" ? e : "L’import a échoué.");
        setEnvoi(false);
      });
  };

  return (
    <Fenetre titre="Importer un monde" occupe={envoi} onFermer={onFermer}>
      <h4>Importer un monde</h4>
      <p className="dlg-txt">Copiés, pas déplacés.</p>
      {envoi ? (
        <Avancement texte={travail?.etape ?? "Préparation"} pourcent={travail?.pourcent ?? null} />
      ) : (
        <>
          <div className="rs-opts" role="radiogroup" aria-label="Monde à importer">
            {autre && (
              <button type="button" role="radio" aria-checked={choix?.chemin === autre.chemin} className={"rs-opt" + (choix?.chemin === autre.chemin ? " sel" : "")} onClick={() => setChoix(autre)}>
                <span className="radio" />
                <span className="rs-t">
                  <b>{autre.nom}</b>
                  <span title={autre.chemin}>Choisi sur ton PC</span>
                </span>
              </button>
            )}
            {solo === null ? (
              <div className="rs-vide">
                <span className="spin" aria-hidden /> Recherche de tes mondes…
              </div>
            ) : solo.length === 0 && !autre ? (
              <div className="rs-vide">Aucun monde solo sur ce PC.</div>
            ) : (
              solo.map((m) => {
                const sel = choix?.chemin === m.chemin;
                const infos = [m.mode && NOMS_MODES[m.mode], m.version, poids(m.taille), joue(m.joue)].filter(Boolean).join(" · ");
                return (
                  <button
                    key={m.chemin}
                    type="button"
                    role="radio"
                    aria-checked={sel}
                    disabled={m.tropRecent}
                    className={"rs-opt" + (sel ? " sel" : "")}
                    onClick={() => setChoix({ chemin: m.chemin, nom: m.nom })}
                  >
                    <span className="radio" />
                    <span className="rs-t">
                      <b>{m.nom}</b>
                      <span>{infos}</span>
                      {m.tropRecent && <em className="bad">Plus récent que ton serveur ({serveur.version}) : il ne pourra pas s’ouvrir.</em>}
                    </span>
                  </button>
                );
              })
            )}
          </div>
          <div className="liens">
            <button type="button" className="err-link" onClick={() => prendre(false)}>
              Choisir un autre dossier…
            </button>
            <button type="button" className="err-link" onClick={() => prendre(true)}>
              Ou un .zip…
            </button>
          </div>
          <div className="tk-opt">
            <button type="button" role="switch" aria-checked={actuel} aria-label="En faire le monde actuel" className={"sw" + (actuel ? " on" : "")} onClick={() => setActuel(!actuel)}>
              <i />
            </button>
            <span>En faire le monde actuel (redémarrage)</span>
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
        <button type="button" className="dlg-ok" disabled={envoi || !choix} onClick={valider}>
          Importer
        </button>
      </div>
    </Fenetre>
  );
}
