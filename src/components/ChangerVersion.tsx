import { useEffect, useState } from "react";
import { IconAlertTriangle, IconCircleCheck, IconInfoCircle, IconSearch } from "@tabler/icons-react";
import Fenetre, { Avancement, comparerVersions, useTravail } from "./Fenetre";
import Segment from "./Segment";
import { changerVersion, versionsMinecraft, type VersionMc } from "../api";
import type { Serveur } from "../types";
import "../pages/NouveauServeur.css";

// Les logiciels vers lesquels un serveur peut passer (les mêmes que familles() dans lib.rs).
export function familles(logiciel: string): string[] {
  if (logiciel === "Vanilla") return ["Vanilla", "Paper", "Fabric"];
  if (logiciel === "Paper" || logiciel === "Purpur" || logiciel === "Spigot") return ["Paper", "Purpur", "Spigot"];
  if (["Fabric", "Forge", "NeoForge", "Mohist", "Youer", "Arclight"].includes(logiciel)) return [logiciel];
  return [];
}

type Props = {
  serveur: Serveur;
  onFini: (s: Serveur) => void; // le serveur à jour
  onFermer: () => void;
};

// « Changer de version… » (Paramètres › Général) : logiciel, version, avertissements, puis installation.
export default function ChangerVersion({ serveur, onFini, onFermer }: Props) {
  const choix = familles(serveur.logiciel);
  const [logiciel, setLogiciel] = useState(choix.includes(serveur.logiciel) ? serveur.logiciel : choix[0]);
  const [listes, setListes] = useState<Record<string, VersionMc[] | string>>({});
  const [version, setVersion] = useState(serveur.version);
  const [filtre, setFiltre] = useState("");
  const [envoi, setEnvoi] = useState(false);
  const [erreur, setErreur] = useState("");
  const travail = useTravail(serveur.id, envoi);

  const liste = listes[logiciel];
  const charger = () => {
    setListes((l) => {
      const copie = { ...l };
      delete copie[logiciel];
      return copie;
    });
    versionsMinecraft(logiciel, false)
      .then((v) => setListes((l) => ({ ...l, [logiciel]: v })))
      .catch((e) => setListes((l) => ({ ...l, [logiciel]: typeof e === "string" ? e : "Pas de connexion. Réessaie." })));
  };
  useEffect(() => {
    if (liste === undefined) charger();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logiciel]);

  const cherche = filtre.trim().toLowerCase();
  const visibles = Array.isArray(liste) ? liste.filter((v) => v.id.toLowerCase().startsWith(cherche)) : [];
  const meme = logiciel === serveur.logiciel && version === serveur.version;
  const ecart = comparerVersions(version, serveur.version);
  const connue = Array.isArray(liste) && liste.some((v) => v.id === version);

  const valider = () => {
    setEnvoi(true);
    setErreur("");
    changerVersion(serveur.id, logiciel, version)
      .then(onFini)
      .catch((e) => {
        setErreur(typeof e === "string" ? e : "Ça n'a pas marché.");
        setEnvoi(false);
      });
  };

  return (
    <Fenetre titre="Changer de version" occupe={envoi} onFermer={onFermer}>
      <h4>Changer de version</h4>
      <p className="dlg-txt">
        Actuellement : {serveur.logiciel} {serveur.version || "?"}
      </p>
      {envoi ? (
        <>
          <Avancement texte={travail?.etape ?? "Préparation"} pourcent={travail?.pourcent ?? null} />
          {logiciel === "Spigot" && <p className="dlg-txt">Spigot se fabrique sur ton PC : 5 à 15 min.</p>}
        </>
      ) : (
        <>
          {choix.length > 1 && (
            <div className="ver-seg">
              <Segment etiquette="Logiciel" options={choix.map((c) => ({ id: c, nom: c }))} valeur={logiciel} onChoisir={setLogiciel} />
            </div>
          )}
          {liste === undefined ? (
            <div className="wz-load">
              <span className="spin" aria-hidden />
              Recherche des versions…
            </div>
          ) : typeof liste === "string" ? (
            <div className="wz-load">
              <span>{liste}</span>
              <button type="button" className="btn" onClick={charger}>
                Réessayer
              </button>
            </div>
          ) : (
            <>
              {liste.length > 12 && (
                <label className="vsearch">
                  <IconSearch size={15} stroke={2} aria-hidden />
                  <input type="search" value={filtre} placeholder={`Rechercher parmi ${liste.length} versions`} spellCheck={false} onChange={(e) => setFiltre(e.target.value)} />
                </label>
              )}
              <section className="ver-list" role="radiogroup" aria-label="Version">
                {visibles.length === 0 && <div className="vvide">Aucune version « {filtre.trim()} »</div>}
                {visibles.map((v) => (
                  <button
                    key={v.id}
                    type="button"
                    role="radio"
                    aria-checked={v.id === version}
                    className={"vrow" + (v.id === version ? " sel" : "")}
                    onClick={() => setVersion(v.id)}
                  >
                    <span className="radio" />
                    <b>{v.id}</b>
                    {v.id === serveur.version && logiciel === serveur.logiciel && <span className="rec">Actuelle</span>}
                    {v.instable && <span className="instable">Instable</span>}
                    {v.java && <span className="j">Java {v.java}</span>}
                  </button>
                ))}
              </section>
            </>
          )}
          {!meme && connue && ecart < 0 && (
            <div className="v-warn red">
              <IconAlertTriangle size={15} stroke={2} aria-hidden />
              <span>Version plus ancienne : risque d’abîmer le monde.</span>
            </div>
          )}
          {!meme && connue && ecart > 0 && (
            <div className="v-warn">
              <IconInfoCircle size={15} stroke={2} aria-hidden />
              <span>Tes amis devront aussi changer de version.</span>
            </div>
          )}
          <div className="rs-safe">
            <IconCircleCheck size={15} stroke={2} aria-hidden />
            <span>BHOST fait une sauvegarde juste avant.</span>
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
        <button type="button" className="dlg-ok" disabled={envoi || meme || !connue} onClick={valider}>
          Changer de version
        </button>
      </div>
    </Fenetre>
  );
}
