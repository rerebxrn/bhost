import { useEffect, useRef, useState } from "react";
import { IconDownload, IconFileImport, IconPlus, IconRefresh, IconTrash, IconUnlink, IconWorld } from "@tabler/icons-react";
import Segment from "../components/Segment";
import Curseur from "../components/Curseur";
import ChangerVersion, { familles } from "../components/ChangerVersion";
import ImporterMonde from "../components/ImporterMonde";
import Reinitialiser from "../components/Reinitialiser";
import { Avancement, joue, poids, useTravail } from "../components/Fenetre";
import {
  choisirEnregistrement,
  choisirJava,
  exporterMonde,
  mondesServeur,
  nouveauMonde,
  parametresServeur,
  reglerPartie,
  reglerRegle,
  reglerServeur,
  reglesServeur,
  supprimerMonde,
  utiliserMonde,
  type Monde,
  type Parametres as Infos,
  type Partie,
  type Regle,
  type ReglagesServeur,
  ouvrirAuxAmis,
  bedrockServeur,
  codeConduite,
  reglerCodeConduite,
  reglerBedrock,
  type VueBedrock,
} from "../api";
import { conduitePossible, nombre, nomLogiciel, ramMaxMachine, type Acces, type Priorite, type Serveur } from "../types";
import "./Parametres.css";

const MODES = [
  { id: "survie", nom: "Survie" },
  { id: "creatif", nom: "Créatif" },
  { id: "aventure", nom: "Aventure" },
  { id: "hardcore", nom: "Hardcore" },
];
const PRIORITES: { id: Priorite; nom: string }[] = [
  { id: "basse", nom: "Basse" },
  { id: "normale", nom: "Normale" },
  { id: "haute", nom: "Haute" },
];
const AIDE_PRIORITE: Record<Priorite, string> = {
  basse: "Tes jeux passent d’abord.",
  normale: "Partagé avec tes autres programmes.",
  haute: "Le serveur passe avant tes jeux.",
};
const DIFFICULTES = [
  { id: "paisible", nom: "Paisible" },
  { id: "facile", nom: "Facile" },
  { id: "normale", nom: "Normale" },
  { id: "difficile", nom: "Difficile" },
];
const JAVAS = ["8", "17", "21", "25"];

// Les règles du jeu, dans l’ordre de la maquette (les clés viennent de regles.rs).
const NOMS_REGLES: Record<string, string> = {
  garder: "Garder l’inventaire à la mort",
  jour: "Cycle jour et nuit",
  meteo: "Changements de météo",
  degats: "Les monstres abîment le monde",
  feu: "Le feu se propage",
  monstres: "Apparition des monstres",
  progres: "Annoncer les progrès",
  coordonnees: "Afficher les coordonnées",
};

type Props = {
  serveur: Serveur;
  acces?: Acces; // Ouvert à tes amis : où en est la box
  onChange: (s: Serveur) => void; // le serveur à jour, renvoyé par Rust
  onRenommer: (nom: string) => Promise<void>;
  onRetirer: () => void;
  onSupprimer: () => void;
  onErreur: (texte: string) => void;
  onInfo: (texte: string) => void; // petit message en verre (« Monde créé »)
};

// La ligne sous « Ouvert à tes amis ».
function texteAmis(serveur: Serveur, acces?: Acces): string {
  if (!serveur.ouvertAmis) return "Tes amis rejoignent par Internet.";
  if (acces?.etat === "ouvert") return `Adresse : ${acces.adresse}`;
  if (acces?.etat === "erreur") return acces.message ?? "Ça n’a pas marché.";
  return "Ouverture de ta box…";
}

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n’a pas marché. Réessaie.");

// L’onglet Paramètres d’un serveur : Général, Performances, Partie, Règles du jeu, Mondes, Avancé, Zone sensible.
// Chaque changement est enregistré tout de suite.
export default function Parametres({ serveur, acces, onChange, onRenommer, onRetirer, onSupprimer, onErreur, onInfo }: Props) {
  const [infos, setInfos] = useState<Infos | null>(null);
  const [nom, setNom] = useState(serveur.nom);
  const [ram, setRam] = useState(Math.max(1, Math.round(serveur.ramMo / 1024)));
  const [arguments_, setArguments] = useState(serveur.arguments ?? "");
  const allume = serveur.statut !== "arrete" && serveur.statut !== "plante";

  const [regles, setRegles] = useState<Regle[] | null>(null);
  const [bedrock, setBedrock] = useState<VueBedrock | null>(null);
  const [bedrockEnCours, setBedrockEnCours] = useState(false);
  const [mondes, setMondes] = useState<Monde[] | null>(null);
  const [fenetre, setFenetre] = useState<null | "version" | "importer" | { reinit: Monde }>(null);
  const [archive, setArchive] = useState(false); // « Télécharger en .zip » en cours
  const travail = useTravail(serveur.id, archive);
  const enLigne = serveur.statut === "en-ligne";
  // Ce qui n’a pas pu être lu : on le dit au lieu de laisser « Lecture… ».
  const [illisible, setIllisible] = useState({ infos: false, mondes: false });
  const lecture = (rate: boolean) => (
    <div className="row">
      <span className="t2">{rate ? "Impossible de lire pour l’instant." : "Lecture…"}</span>
    </div>
  );

  useEffect(() => {
    setRegles(null);
    setMondes(null);
    reglesServeur(serveur.id).then(setRegles, () => setRegles([]));
    setIllisible((r) => ({ ...r, mondes: false }));
    mondesServeur(serveur.id).then(setMondes, (e) => {
      setIllisible((r) => ({ ...r, mondes: true }));
      onErreur(phrase(e));
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id, serveur.version]);

  // Le code de conduite (1.21.9 et plus) : enregistré en quittant la zone de texte.
  const [conduite, setConduite] = useState<string | null>(null);
  const [conduiteLue, setConduiteLue] = useState("");
  const avecConduite = conduitePossible(serveur.version);
  useEffect(() => {
    setConduite(null);
    if (!avecConduite) return;
    codeConduite(serveur.id).then(
      (t) => {
        setConduite(t);
        setConduiteLue(t);
      },
      () => setConduite(""),
    );
  }, [serveur.id, avecConduite]);
  const garderConduite = () => {
    if (conduite === null || conduite.trim() === conduiteLue) return;
    reglerCodeConduite(serveur.id, conduite).then(
      (t) => {
        setConduite(t);
        setConduiteLue(t);
        onInfo(t ? "Code de conduite enregistré." : "Code de conduite retiré.");
      },
      (e) => onErreur(phrase(e)),
    );
  };

  // Joueurs Bedrock : relu aussi quand le serveur démarre ou s’arrête (« Au prochain démarrage » s’efface).
  useEffect(() => {
    bedrockServeur(serveur.id).then(setBedrock, () => setBedrock(null));
  }, [serveur.id, serveur.version, serveur.statut]);

  useEffect(() => {
    setIllisible((r) => ({ ...r, infos: false }));
    parametresServeur(serveur.id).then(setInfos, (e) => {
      setIllisible((r) => ({ ...r, infos: true }));
      onErreur(phrase(e));
    });
    // On ne relit qu’en changeant de serveur : ensuite, chaque réponse de Rust met à jour l’aperçu.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id]);

  // Les réglages gardés par BHOST : on renvoie tout, avec le changement.
  // Toujours le serveur à jour : un curseur qui se calme après un autre changement ne remet pas l’ancien.
  const dernier = useRef(serveur);
  dernier.current = serveur;
  const regler = (change: Partial<ReglagesServeur>) => {
    const serveur = dernier.current;
    const reglages: ReglagesServeur = {
      ramMo: serveur.ramMo,
      demarrerAuto: !!serveur.demarrerAuto,
      javaChoix: serveur.javaChoix ?? null,
      optimise: !!serveur.optimise,
      arguments: serveur.arguments ?? "",
      priorite: serveur.priorite ?? "normale",
      ...change,
    };
    return reglerServeur(serveur.id, reglages).then(
      ([s, i]) => {
        onChange(s);
        setInfos(i);
      },
      (e) => onErreur(phrase(e)),
    );
  };

  // La mémoire : on attend que le curseur se calme avant d’enregistrer.
  const attenteRam = useRef(0);
  const changerRam = (go: number) => {
    setRam(go);
    window.clearTimeout(attenteRam.current);
    const id = serveur.id;
    attenteRam.current = window.setTimeout(() => dernier.current.id === id && regler({ ramMo: go * 1024 }), 400);
  };

  // Machine dédiée : le stockage max, enregistré quand le curseur se calme.
  const [stockage, setStockage] = useState(Math.max(1, Math.round((serveur.disqueMaxMo ?? 0) / 1024)));
  const attenteStockage = useRef(0);
  const changerStockage = (go: number) => {
    setStockage(go);
    window.clearTimeout(attenteStockage.current);
    const id = serveur.id;
    attenteStockage.current = window.setTimeout(() => dernier.current.id === id && regler({ disqueMaxMo: go * 1024 }), 400);
  };

  const changerPartie = (change: Partial<Partie>) => {
    if (!infos) return;
    const partie = { ...infos.partie, ...change };
    setInfos({ ...infos, partie });
    reglerPartie(serveur.id, partie).then(onChange, (e) => onErreur(phrase(e)));
  };

  const garderNom = () => {
    const propre = nom.trim();
    if (propre === serveur.nom) return;
    onRenommer(propre).catch((e) => {
      onErreur(phrase(e));
      setNom(serveur.nom);
    });
  };

  const choix = serveur.javaChoix;
  const javaValeur = !choix ? "auto" : JAVAS.includes(choix) ? choix : "autre";
  const javaOptions = [
    { id: "auto", nom: infos?.javaAuto ? `Auto (${infos.javaAuto})` : "Auto" },
    ...JAVAS.map((v) => ({ id: v, nom: v })),
    { id: "autre", nom: "Autre…" },
  ];
  const choisirVersion = (id: string) => {
    if (id === "auto") regler({ javaChoix: null });
    else if (id === "autre")
      choisirJava().then((chemin) => {
        if (chemin) regler({ javaChoix: chemin });
      });
    else regler({ javaChoix: id });
  };

  // Une règle : on l’affiche tout de suite, Rust confirme (ou on revient en arrière).
  const changerRegle = (cle: string, valeur: number) => {
    const avant = regles;
    setRegles((l) => l?.map((r) => (r.cle === cle ? { ...r, valeur } : r)) ?? l);
    reglerRegle(serveur.id, cle, valeur).then(setRegles, (e) => {
      setRegles(avant);
      onErreur(phrase(e));
    });
  };
  const dormir = regles?.find((r) => r.cle === "dormir");
  const texteDormir = (v: number) => (v >= 100 ? "Tout le monde." : v <= 0 ? "Un seul joueur suffit." : `${v} % des joueurs.`);

  const surMondes = (action: Promise<Monde[]>, texte?: string) =>
    action.then(
      (l) => {
        setMondes(l);
        if (texte) onInfo(texte);
      },
      (e) => onErreur(phrase(e)),
    );
  const telecharger = async (m: Monde) => {
    const fichier = await choisirEnregistrement(`${m.nom}.zip`);
    if (!fichier) return;
    setArchive(true);
    exporterMonde(serveur.id, m.dossier, fichier)
      .then(() => onInfo(`« ${m.nom}.zip » enregistré`), (e) => onErreur(phrase(e)))
      .finally(() => setArchive(false));
  };

  const versionPossible = familles(serveur.logiciel).length > 0;

  const memoireMax = serveur.machine && infos ? ramMaxMachine(infos.memoireGo) : Math.max(2, Math.floor(infos?.memoireGo ?? 16));
  const stockageMax = Math.max(5, Math.min(2000, Math.floor((serveur.disqueMaxMo ?? 0) / 1024 + (infos?.disqueLibreGo ?? 0))));
  const plusTard = allume ? <span className="t2">Au prochain démarrage</span> : null;

  // Joueurs Bedrock : installer (ou couper) Geyser et Floodgate.
  const basculerBedrock = () => {
    if (!bedrock || bedrockEnCours) return;
    const voulu = !bedrock.actif;
    setBedrockEnCours(true);
    reglerBedrock(serveur.id, voulu)
      .then(
        (v) => {
          setBedrock(v);
          if (v.note) onErreur(v.note);
          else onInfo(voulu ? "Joueurs Bedrock autorisés" : "Joueurs Bedrock coupés");
        },
        (e) => onErreur(phrase(e)),
      )
      .finally(() => setBedrockEnCours(false));
  };
  const texteBedrock = !bedrock
    ? "Consoles et téléphones"
    : !bedrock.possible
      ? (bedrock.raison ?? "Pas possible avec ce logiciel.")
      : bedrockEnCours
        ? "Installation de Geyser et Floodgate…"
        : bedrock.redemarrer
          ? "Au prochain démarrage"
          : "Consoles et téléphones";
  // L’adresse pour eux : celle de la box (avec le port Bedrock), sinon le port seul.
  const adresseBedrock = !bedrock ? "" : acces?.bedrock ?? (serveur.ouvertAmis && acces?.etat !== "erreur" && !acces?.bedrockMessage ? "Ouverture…" : `Port ${bedrock.port}`);
  const aideBedrock = !serveur.ouvertAmis
    ? "Active « Ouvert à tes amis » pour Internet."
    : acces?.bedrockMessage ?? (acces?.etat === "erreur" ? acces.message : "Adresse et port, dans le jeu.");
  const partie = infos?.partie;

  return (
    <div className="params">
      <div className="grp-t">Général</div>
      <section className="surf">
        <div className="row">
          <label htmlFor="ps-nom">Nom du serveur</label>
          <input
            id="ps-nom"
            className="field texte"
            type="text"
            maxLength={40}
            value={nom}
            spellCheck={false}
            onChange={(e) => setNom(e.target.value)}
            onBlur={garderNom}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
              if (e.key === "Escape") setNom(serveur.nom);
            }}
          />
        </div>
        <div className="row">
          <div className="lbl">
            <span>Logiciel et version</span>
            <span className="t2">{nomLogiciel(serveur)}</span>
          </div>
          <button
            type="button"
            className="btn"
            disabled={allume || !versionPossible}
            title={!versionPossible ? "Bientôt" : allume ? "Arrête-le d’abord" : undefined}
            onClick={() => setFenetre("version")}
          >
            Changer de version…
          </button>
        </div>
        <div className="row">
          <div className="lbl">
            <span id="ps-amis">Ouvert à tes amis</span>
            <span className={"t2" + (serveur.ouvertAmis && acces?.etat === "erreur" ? " ps-err" : "")}>{texteAmis(serveur, acces)}</span>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={!!serveur.ouvertAmis}
            aria-labelledby="ps-amis"
            className={"sw" + (serveur.ouvertAmis ? " on" : "")}
            onClick={() => ouvrirAuxAmis(serveur.id, !serveur.ouvertAmis).then(onChange, (e) => onErreur(phrase(e)))}
          >
            <i />
          </button>
        </div>
        <div className="row">
          <span id="ps-auto">Démarrer avec BHOST</span>
          <button
            type="button"
            role="switch"
            aria-checked={!!serveur.demarrerAuto}
            aria-labelledby="ps-auto"
            className={"sw" + (serveur.demarrerAuto ? " on" : "")}
            onClick={() => regler({ demarrerAuto: !serveur.demarrerAuto })}
          >
            <i />
          </button>
        </div>
      </section>

      <div className="grp-t">Performances</div>
      <section className="surf">
        <div className="row">
          <div className="lbl">
            <span>Mémoire allouée</span>
            {plusTard ?? (infos && <span className="t2">Sur {Math.round(infos.memoireGo)} Go</span>)}
          </div>
          <div className="ps-ram">
            <Curseur min={1} max={memoireMax} valeur={ram} onChange={changerRam} etiquette="Mémoire allouée" unite="Go" />
            <b className="tnum">{ram} Go</b>
          </div>
        </div>
        {serveur.disqueMaxMo ? (
          <div className="row">
            <div className="lbl">
              <span>Stockage max</span>
              <span className="t2">
                {serveur.disqueMo !== undefined ? `${nombre(serveur.disqueMo / 1024)} Go pris. ` : ""}Le monde, les plugins et les sauvegardes.
              </span>
            </div>
            <div className="ps-ram">
              <Curseur min={5} max={stockageMax} valeur={Math.min(stockage, stockageMax)} onChange={changerStockage} etiquette="Stockage max" unite="Go" />
              <b className="tnum">{Math.min(stockage, stockageMax)} Go</b>
            </div>
          </div>
        ) : null}
        <div className="row">
          <div className="lbl">
            <span>Priorité</span>
            <span className="t2">{AIDE_PRIORITE[serveur.priorite ?? "normale"]}</span>
          </div>
          <Segment etiquette="Priorité" options={PRIORITES} valeur={serveur.priorite ?? "normale"} onChoisir={(priorite) => regler({ priorite })} />
        </div>
      </section>

      <div className="grp-t">Partie</div>
      <section className="surf">
        {partie ? (
          <>
            <div className="row">
              <div className="lbl">
                <span>Mode de jeu</span>
                {plusTard}
              </div>
              <Segment etiquette="Mode de jeu" options={MODES} valeur={partie.mode} onChoisir={(mode) => changerPartie({ mode })} />
            </div>
            <div className="row">
              <span>Difficulté</span>
              <Segment
                etiquette="Difficulté"
                options={DIFFICULTES}
                valeur={partie.mode === "hardcore" ? "difficile" : partie.difficulte}
                off={partie.mode === "hardcore"}
                onChoisir={(difficulte) => changerPartie({ difficulte })}
              />
            </div>
            <div className="row">
              <div className="lbl">
                <span>Joueurs en même temps</span>
                {plusTard}
              </div>
              <div className="stepper">
                <button type="button" aria-label="Moins" onClick={() => changerPartie({ joueursMax: Math.max(1, partie.joueursMax - 1) })}>
                  −
                </button>
                <span className="tnum">
                  <b key={partie.joueursMax}>{partie.joueursMax}</b>
                </span>
                <button type="button" aria-label="Plus" onClick={() => changerPartie({ joueursMax: Math.min(100, partie.joueursMax + 1) })}>
                  +
                </button>
              </div>
            </div>
          </>
        ) : (
          lecture(illisible.infos)
        )}
      </section>

      <div className="grp-t">Règles du jeu</div>
      <section className="surf">
        {regles === null || regles.length === 0 ? (
          lecture(regles !== null)
        ) : (
          <>
            {regles
              .filter((r) => NOMS_REGLES[r.cle])
              .map((r) => (
                <div className="row" key={r.cle}>
                  <span id={`gr-${r.cle}`}>{NOMS_REGLES[r.cle]}</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={r.valeur !== 0}
                    aria-labelledby={`gr-${r.cle}`}
                    className={"sw" + (r.valeur !== 0 ? " on" : "")}
                    onClick={() => changerRegle(r.cle, r.valeur !== 0 ? 0 : 1)}
                  >
                    <i />
                  </button>
                </div>
              ))}
            {dormir && (
              <div className="row">
                <div className="lbl">
                  <span>Joueurs qui doivent dormir</span>
                  <span className="t2">{texteDormir(dormir.valeur)}</span>
                </div>
                <div className="stepper">
                  <button type="button" aria-label="Moins" onClick={() => changerRegle("dormir", Math.max(0, Math.ceil(dormir.valeur / 10) * 10 - 10))}>
                    −
                  </button>
                  <span className="tnum ps-pct">
                    <b key={dormir.valeur}>{dormir.valeur} %</b>
                  </span>
                  <button type="button" aria-label="Plus" onClick={() => changerRegle("dormir", Math.min(100, Math.floor(dormir.valeur / 10) * 10 + 10))}>
                    +
                  </button>
                </div>
              </div>
            )}
            {!enLigne && <p className="ps-note">Appliquées au prochain démarrage.</p>}
          </>
        )}
      </section>

      {avecConduite && (
        <>
          <div className="grp-t">Code de conduite</div>
          <section className="surf">
            <div className="row">
              <div className="lbl">
                <span>Montré aux joueurs à leur arrivée</span>
                <span className="t2">Ils doivent l’accepter pour jouer. Vide : pas de code de conduite.</span>
              </div>
            </div>
            <div className="ps-conduite">
              <textarea
                className="field"
                aria-label="Code de conduite"
                rows={5}
                maxLength={4000}
                placeholder={"Sois respectueux avec tout le monde.\nPas de triche, pas de grief.\nPas de pub pour d’autres serveurs."}
                value={conduite ?? ""}
                disabled={conduite === null}
                onChange={(e) => setConduite(e.target.value)}
                onBlur={garderConduite}
              />
            </div>
            {allume && <p className="ps-note">Au prochain démarrage.</p>}
          </section>
        </>
      )}

      <div className="grp-t">Mondes</div>
      <section className="surf">
        {mondes === null ? (
          lecture(illisible.mondes)
        ) : (
          mondes.map((m) => (
            <div className={"wrow" + (m.actuel ? " cur" : "")} key={m.dossier}>
              <span className="w-ic">
                <IconWorld size={17} stroke={1.8} aria-hidden />
              </span>
              <div className="mod-txt">
                <b>
                  <span className="w-nom">{m.nom}</span>
                  {m.actuel && <span className="rec">Monde actuel</span>}
                </b>
                <span>{[m.version, m.taille > 0 ? poids(m.taille) : null, joue(m.joue)].filter(Boolean).join(" · ")}</span>
              </div>
              <button type="button" className="icb" aria-label="Télécharger en .zip" title={serveur.machine ? "Seulement pour les serveurs de ce PC" : "Télécharger en .zip"} disabled={archive || m.taille === 0 || !!serveur.machine} onClick={() => telecharger(m)}>
                <IconDownload size={15} stroke={2} aria-hidden />
              </button>
              <button
                type="button"
                className="icb"
                aria-label="Réinitialiser"
                title={m.actuel && allume ? "Arrête-le d’abord" : "Réinitialiser"}
                disabled={m.taille === 0 || (m.actuel && allume)}
                onClick={() => setFenetre({ reinit: m })}
              >
                <IconRefresh size={15} stroke={2} aria-hidden />
              </button>
              {m.actuel ? (
                // La place des boutons des autres mondes : les icônes restent en colonne.
                <span className="w-place" aria-hidden>
                  <span className="icb" />
                  <span className="btn">Utiliser</span>
                </span>
              ) : (
                <>
                  <button
                    type="button"
                    className="icb del"
                    aria-label="Supprimer"
                    title="Mettre à la Corbeille"
                    onClick={() => surMondes(supprimerMonde(serveur.id, m.dossier), `« ${m.nom} » mis à la Corbeille`)}
                  >
                    <IconTrash size={15} stroke={2} aria-hidden />
                  </button>
                  <button
                    type="button"
                    className="btn"
                    onClick={() =>
                      surMondes(utiliserMonde(serveur.id, m.dossier), allume ? `Redémarre pour jouer sur « ${m.nom} »` : `« ${m.nom} » choisi`)
                    }
                  >
                    Utiliser
                  </button>
                </>
              )}
            </div>
          ))
        )}
        {archive && (
          <div className="row">
            <Avancement texte={travail?.etape ?? "Archive"} pourcent={travail?.pourcent ?? null} />
          </div>
        )}
        <div className="row w-acts">
          <button type="button" className="btn" onClick={() => setFenetre("importer")}>
            <IconFileImport size={14} stroke={2} aria-hidden />
            Importer un monde…
          </button>
          <button type="button" className="btn" onClick={() => surMondes(nouveauMonde(serveur.id), "Monde créé")}>
            <IconPlus size={14} stroke={2} aria-hidden />
            Nouveau monde
          </button>
        </div>
      </section>

      <div className="grp-t">Joueurs Bedrock</div>
      <section className="surf">
        <div className="row">
          <div className="lbl">
            <span id="ps-bedrock">Autoriser les joueurs Bedrock</span>
            <span className="t2">{texteBedrock}</span>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={!!bedrock?.actif}
            aria-labelledby="ps-bedrock"
            aria-busy={bedrockEnCours}
            disabled={!bedrock?.possible || bedrockEnCours}
            className={"sw" + (bedrock?.actif ? " on" : "")}
            onClick={basculerBedrock}
          >
            <i />
          </button>
        </div>
        {bedrock?.actif && (
          <div className="row">
            <div className="lbl">
              <span>Adresse pour eux</span>
              <span className={"t2" + (serveur.ouvertAmis && (acces?.bedrockMessage || acces?.etat === "erreur") ? " ps-err" : "")}>{aideBedrock}</span>
            </div>
            <input className="field ps-bedrock tnum" type="text" readOnly value={adresseBedrock} aria-label="Adresse pour les joueurs Bedrock" />
          </div>
        )}
      </section>

      <div className="grp-t">Avancé</div>
      <section className="surf">
        <div className="row">
          <div className="lbl">
            <span>Version de Java</span>
            {plusTard}
          </div>
          <Segment etiquette="Version de Java" options={javaOptions} valeur={javaValeur} onChoisir={choisirVersion} />
        </div>
        {javaValeur === "autre" && choix && (
          <div className="row">
            <input className="field chemin" type="text" readOnly value={choix} aria-label="Java choisi" title={choix} />
            <button type="button" className="btn" onClick={() => choisirVersion("autre")}>
              Changer…
            </button>
          </div>
        )}
        <div className="row">
          <span id="ps-opt">Réglages de démarrage optimisés</span>
          <button
            type="button"
            role="switch"
            aria-checked={!!serveur.optimise}
            aria-labelledby="ps-opt"
            className={"sw" + (serveur.optimise ? " on" : "")}
            onClick={() => regler({ optimise: !serveur.optimise })}
          >
            <i />
          </button>
        </div>
        <div className="row">
          <label htmlFor="ps-args">Arguments en plus</label>
          <input
            id="ps-args"
            className="field"
            type="text"
            value={arguments_}
            placeholder="-Dfile.encoding=UTF-8"
            spellCheck={false}
            onChange={(e) => setArguments(e.target.value)}
            onBlur={() => arguments_.trim() !== (serveur.arguments ?? "") && regler({ arguments: arguments_ })}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </div>
        {infos && (
          <div className="ps-cmd">
            <span>Commande lancée</span>
            <code>{infos.commande}</code>
          </div>
        )}
      </section>

      <div className="grp-t">Zone sensible</div>
      <section className="surf">
        <div className="row">
          <span>Retirer de BHOST</span>
          <button type="button" className="btn" disabled={allume} title={allume ? "Arrête-le d’abord" : serveur.machine ? "Le dossier reste sur la machine" : "Le dossier reste sur ton PC"} onClick={onRetirer}>
            <IconUnlink size={14} stroke={2} aria-hidden />
            Retirer…
          </button>
        </div>
        <div className="row">
          <span>Supprimer le serveur</span>
          <button type="button" className="btn danger" disabled={allume} title={allume ? "Arrête-le d’abord" : undefined} onClick={onSupprimer}>
            <IconTrash size={14} stroke={2} aria-hidden />
            Supprimer…
          </button>
        </div>
      </section>

      {fenetre === "version" && (
        <ChangerVersion
          serveur={serveur}
          onFermer={() => setFenetre(null)}
          onFini={(s) => {
            setFenetre(null);
            onChange(s);
            onInfo(`Passage à ${s.logiciel} ${s.version}`);
            parametresServeur(s.id).then(setInfos, () => {});
          }}
        />
      )}
      {fenetre === "importer" && (
        <ImporterMonde
          serveur={serveur}
          onFermer={() => setFenetre(null)}
          onFini={(l, nom, actuel) => {
            setFenetre(null);
            setMondes(l);
            onInfo(`« ${nom} » importé${actuel && allume ? " : redémarre pour y jouer" : ""}`);
          }}
        />
      )}
      {fenetre && typeof fenetre === "object" && (
        <Reinitialiser
          id={serveur.id}
          monde={fenetre.reinit}
          onFermer={() => setFenetre(null)}
          onFini={(l) => {
            setFenetre(null);
            setMondes(l);
            onInfo("Monde réinitialisé");
          }}
        />
      )}
    </div>
  );
}
