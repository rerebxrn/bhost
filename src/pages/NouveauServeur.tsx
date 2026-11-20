import { useEffect, useMemo, useState, type ComponentType } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  IconAdjustmentsHorizontal,
  IconAlertTriangle,
  IconArrowLeft,
  IconArrowRight,
  IconBrush,
  IconCheck,
  IconCube,
  IconDroplet,
  IconFolder,
  IconFolderOpen,
  IconHammer,
  IconMap2,
  IconPackage,
  IconPlug,
  IconPuzzle,
  IconSearch,
  IconSkull,
  IconStack2,
  IconSword,
} from "@tabler/icons-react";
import Tabs from "../components/Tabs";
import Segment from "../components/Segment";
import Curseur from "../components/Curseur";
import { LOGOS } from "../theme/logiciels";
import { couleurDe, type Couleur } from "../components/IconeServeur";
import ChoixIcone from "../components/ChoixIcone";
import { image } from "../theme/icones";
import {
  changerDossierServeurs,
  choisirDossier,
  creerServeur,
  ecouterCreation,
  importerServeur,
  infosCreation,
  versionsMinecraft,
  type InfosCreation,
  type VersionMc,
} from "../api";
import { nombre, type Logiciel as NomLogiciel, type Serveur } from "../types";
import "./NouveauServeur.css";

type Icone = ComponentType<{ size?: number; stroke?: number; "aria-hidden"?: boolean }>;

// Les logiciels, comme dans la maquette. « pret » : BHOST sait déjà l'installer.
type Logiciel = { id: string; nom: string; desc: string; couleur: string; Icone: Icone; ram: number; pret?: boolean; recommande?: boolean };
const LOGICIELS: Record<string, Logiciel[]> = {
  "Avec plugins": [
    { id: "Paper", nom: "Paper", desc: "Rapide et stable.", couleur: "#4F6C8C", Icone: IconPlug, ram: 4, pret: true, recommande: true },
    { id: "Purpur", nom: "Purpur", desc: "Paper, en plus réglable.", couleur: "#626471", Icone: IconAdjustmentsHorizontal, ram: 4, pret: true },
    { id: "Spigot", nom: "Spigot", desc: "L’ancêtre de Paper.", couleur: "#987C5E", Icone: IconDroplet, ram: 4, pret: true },
  ],
  "Avec mods": [
    { id: "Fabric", nom: "Fabric", desc: "Léger, idéal pour les mods récents.", couleur: "#8A7458", Icone: IconPuzzle, ram: 6, pret: true },
    { id: "Forge", nom: "Forge", desc: "Le plus de mods, surtout anciens.", couleur: "#433A40", Icone: IconHammer, ram: 8, pret: true },
    { id: "NeoForge", nom: "NeoForge", desc: "Forge, en moderne.", couleur: "#6E533E", Icone: IconHammer, ram: 8, pret: true },
  ],
  "Mods et plugins ensemble": [
    { id: "Youer", nom: "Youer", desc: "NeoForge + plugins Paper.", couleur: "#55606E", Icone: IconStack2, ram: 6, pret: true },
    { id: "Arclight", nom: "Arclight", desc: "NeoForge ou Forge + plugins.", couleur: "#6B6150", Icone: IconStack2, ram: 6, pret: true },
    { id: "Mohist", nom: "Mohist", desc: "Forge + plugins.", couleur: "#7A5C48", Icone: IconStack2, ram: 6, pret: true },
  ],
  "Pack tout prêt": [{ id: "Modpack", nom: "Modpack", desc: "Un pack Modrinth en un clic.", couleur: "#2E5FA8", Icone: IconPackage, ram: 8 }],
  "Sans ajout": [{ id: "Vanilla", nom: "Vanilla", desc: "Le jeu officiel, sans ajout.", couleur: "#5E7A45", Icone: IconCube, ram: 3, pret: true }],
};
const TOUS = Object.values(LOGICIELS).flat();
// Ces logiciels s'installent avec leur installateur officiel (plus long qu'un simple téléchargement).
const INSTALLATEUR = ["Fabric", "Forge", "NeoForge"];
// Ces logiciels installent leurs bibliothèques au premier démarrage (quelques minutes).
const HYBRIDES = ["Youer", "Arclight", "Mohist"];

const MODES: { id: string; nom: string; desc: string; Icone: Icone }[] = [
  { id: "survie", nom: "Survie", desc: "Récolter, construire, survivre.", Icone: IconSword },
  { id: "creatif", nom: "Créatif", desc: "Blocs illimités, on ne meurt pas.", Icone: IconBrush },
  { id: "aventure", nom: "Aventure", desc: "Pour les cartes à explorer.", Icone: IconMap2 },
  { id: "hardcore", nom: "Hardcore", desc: "Une seule vie, difficulté max.", Icone: IconSkull },
];
const DIFFICULTES = [
  { id: "paisible", nom: "Paisible" },
  { id: "facile", nom: "Facile" },
  { id: "normale", nom: "Normale" },
  { id: "difficile", nom: "Difficile" },
];

type Etape = "dossier" | "logiciel" | "version" | "monde" | "resume";
const NOMS_ETAPES: Record<Etape, string> = { dossier: "Dossier", logiciel: "Logiciel", version: "Version", monde: "Monde", resume: "Résumé" };

type Props = {
  onCree: (s: Serveur) => void; // serveur créé ou importé : on l'ouvre
  onAnnuler?: () => void; // absent s'il n'y a encore aucun serveur
};

// L'assistant « Créer un serveur », comme dans la maquette.
export default function NouveauServeur({ onCree, onAnnuler }: Props) {
  const [infos, setInfos] = useState<InfosCreation | null>(null);
  const [etape, setEtape] = useState<Etape>("logiciel");
  const [logiciel, setLogiciel] = useState("Paper");
  const [snapshots, setSnapshots] = useState(false);
  const [versions, setVersions] = useState<Record<string, VersionMc[] | string>>({}); // liste, ou phrase d'erreur
  const [version, setVersion] = useState<string | null>(null);
  const [filtre, setFiltre] = useState("");
  const [couleur, setCouleur] = useState<Couleur | null>(null); // null : selon le logiciel
  const [nom, setNom] = useState("");
  const [mode, setMode] = useState("survie");
  const [difficulte, setDifficulte] = useState("normale");
  const [joueurs, setJoueurs] = useState(10);
  const [ram, setRam] = useState(4);
  const [eula, setEula] = useState(false);
  const [amis, setAmis] = useState(true); // Ouvert à tes amis (oui par défaut)
  const [erreur, setErreur] = useState("");
  // Pendant la création : l'avancement de chaque tâche, puis le résultat.
  const [creation, setCreation] = useState<{ taches: number[]; erreur?: string; fini?: boolean } | null>(null);

  // Au départ : le dossier des serveurs, la place libre, la mémoire du PC.
  const relireInfos = () =>
    infosCreation()
      .then((i) => {
        setInfos(i);
        return i;
      })
      .catch(() => null);
  useEffect(() => {
    relireInfos().then((i) => {
      if (i?.premier) setEtape("dossier");
    });
  }, []);

  const etapes: Etape[] = infos?.premier ? ["dossier", "logiciel", "version", "monde", "resume"] : ["logiciel", "version", "monde", "resume"];
  const rang = etapes.indexOf(etape);
  const choisi = TOUS.find((l) => l.id === logiciel) ?? TOUS[0];

  // La liste des versions, demandée une fois par logiciel (et par choix « snapshots »).
  const cle = `${logiciel}|${logiciel === "Vanilla" && snapshots}`;
  const liste = versions[cle];
  const charger = () => {
    setVersions((v) => {
      const copie = { ...v };
      delete copie[cle];
      return copie;
    });
    versionsMinecraft(logiciel, logiciel === "Vanilla" && snapshots)
      .then((l) => setVersions((v) => ({ ...v, [cle]: l })))
      .catch((e) => setVersions((v) => ({ ...v, [cle]: typeof e === "string" ? e : "Pas de connexion. Réessaie." })));
  };
  useEffect(() => {
    if (etape === "version" && liste === undefined) charger();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [etape, cle]);
  // Une version choisie qui n'est plus dans la liste (autre logiciel) : on prend la dernière stable.
  useEffect(() => {
    if (!Array.isArray(liste)) return;
    if (!version || !liste.some((v) => v.id === version)) setVersion((liste.find((v) => !v.instable) ?? liste[0])?.id ?? null);
  }, [liste, version]);
  const versionChoisie = Array.isArray(liste) ? liste.find((v) => v.id === version) : undefined;
  const derniere = Array.isArray(liste) ? liste.find((v) => !v.instable)?.id : undefined;
  const cherche = filtre.trim().toLowerCase();
  const visibles = Array.isArray(liste) ? liste.filter((v) => v.id.toLowerCase().startsWith(cherche)) : [];
  const cube = couleur ?? couleurDe({ couleur: undefined, logiciel: logiciel as NomLogiciel });

  const ramMax = Math.max(2, Math.min(16, Math.floor((infos?.memoireGo ?? 8) - 2)));
  const nomPropre = nom.trim();
  const peutContinuer =
    etape === "version" ? !!versionChoisie : etape === "monde" ? nomPropre.length > 0 && eula : etape === "logiciel" ? !!choisi.pret : true;

  const aller = (e: Etape) => {
    setErreur("");
    setEtape(e);
  };

  const importer = async () => {
    setErreur("");
    const dossier = await choisirDossier("Choisis le dossier du serveur");
    if (!dossier) return;
    try {
      onCree(await importerServeur(dossier));
    } catch (e) {
      setErreur(typeof e === "string" ? e : "L'import a échoué. Réessaie.");
    }
  };

  const changerDossier = async () => {
    const dossier = await choisirDossier("Où ranger tes serveurs ?");
    if (!dossier) return;
    changerDossierServeurs(dossier)
      .then(relireInfos)
      .catch((e) => setErreur(typeof e === "string" ? e : "Ce dossier ne convient pas."));
  };

  // La création elle-même : Rust donne l'avancement de chaque tâche.
  useEffect(
    () =>
      ecouterCreation((tache, pourcent) =>
        setCreation((c) =>
          c && !c.fini && !c.erreur ? { ...c, taches: c.taches.map((p, i) => (i < tache ? 100 : i === tache ? pourcent : p)) } : c,
        ),
      ),
    [],
  );
  const creer = () => {
    if (!versionChoisie) return;
    setCreation({ taches: [0, 0, 0] });
    creerServeur({ nom: nomPropre, logiciel, version: versionChoisie.id, ramGo: ram, eula, mode, difficulte, joueursMax: joueurs, couleur: cube, ouvertAmis: amis })
      .then((s) => {
        setCreation({ taches: [100, 100, 100], fini: true });
        window.setTimeout(() => onCree(s), 900);
      })
      .catch((e) => setCreation((c) => ({ taches: c?.taches ?? [0, 0, 0], erreur: typeof e === "string" ? e : "La création a échoué." })));
  };

  const java = versionChoisie?.java;
  const taches = [
    java ? `Installer ${logiciel === "Spigot" ? "le JDK" : "Java"} ${java} (si besoin)` : "Vérifier Java",
    logiciel === "Spigot"
      ? `Fabriquer Spigot ${version ?? ""} (5 à 15 min)`
      : INSTALLATEUR.includes(logiciel)
        ? `Installer ${choisi.nom} ${version ?? ""} (1 à 3 min)`
        : HYBRIDES.includes(logiciel)
          ? `Télécharger ${choisi.nom} ${version ?? ""} (1er démarrage plus long)`
          : `Télécharger ${choisi.nom} ${version ?? ""}`,
    `Préparer le monde « ${nomPropre || "Nouveau serveur"} »`,
  ];
  const enCreation = creation !== null;

  const TITRES: Record<Etape, [string, string]> = {
    dossier: ["Où ranger tes serveurs ?", ""],
    logiciel: ["Quel logiciel ?", ""],
    version: ["Quelle version de Minecraft ?", ""],
    monde: ["Ton monde", ""],
    resume: ["Tout est prêt ?", ""],
  };
  const [titre, sousTitre] = enCreation ? [`Création de « ${nomPropre} »`, ""] : TITRES[etape];
  const pourcentTotal = creation ? Math.round(creation.taches.reduce((a, b) => a + b, 0) / creation.taches.length) : 0;

  const onglets = useMemo(
    () => etapes.map((e, i) => ({ id: e, nom: NOMS_ETAPES[e], off: enCreation || i > rang })),
    [etapes.join(), rang, enCreation],
  );

  return (
    <>
      <Tabs onglets={onglets} actif={etape} onChoisir={(e) => etapes.indexOf(e) < rang && aller(e)} etiquette="Étapes de création" />

      <div className="scroll">
        <div className="page wz" key={enCreation ? "creation" : etape}>
          <div className="wz-head">
            <span className="wz-kick">{enCreation ? "Installation" : `Étape ${rang + 1} sur ${etapes.length}`}</span>
            <h1>{titre}</h1>
            {sousTitre && <p>{sousTitre}</p>}
          </div>

          {enCreation ? (
            <section className="surf inst" aria-live="polite">
              <div className="inst-top">
                <b>{pourcentTotal} %</b>
                <span>{creation.erreur ? "Arrêté" : creation.fini ? "Terminé" : (taches[creation.taches.findIndex((p) => p < 100)] ?? taches[2]) + "…"}</span>
              </div>
              <div className="disk big">
                <i style={{ width: `${pourcentTotal}%` }} />
              </div>
              {taches.map((t, i) => {
                const p = creation.taches[i];
                const enCours = p < 100 && creation.taches.slice(0, i).every((x) => x >= 100);
                return (
                  <div key={t} className={"task" + (p >= 100 ? " done" : enCours ? "" : " wait")}>
                    <span className="ic">
                      {p >= 100 ? <IconCheck size={16} stroke={2.4} aria-hidden /> : enCours && !creation.erreur ? <span className="spin" aria-hidden /> : <span className="pt" aria-hidden />}
                    </span>
                    <span>{t}</span>
                    {enCours && p > 0 && p < 100 && <span className="task-p tnum">{p} %</span>}
                  </div>
                );
              })}
              {creation.erreur && (
                <div className="inst-err" role="alert">
                  <IconAlertTriangle size={16} stroke={2} aria-hidden />
                  <span>{creation.erreur}</span>
                </div>
              )}
            </section>
          ) : etape === "dossier" ? (
            <section className="surf dir">
              <span className="dir-ico">
                <IconFolderOpen size={26} stroke={1.8} aria-hidden />
              </span>
              <div className="dir-main">
                <b>Dossier des serveurs</b>
                <span className="dir-path">{infos?.dossier ?? "…"}</span>
                {infos && infos.totalGo > 0 && (
                  <>
                    <div className="disk">
                      <i style={{ width: `${Math.round((1 - infos.libreGo / infos.totalGo) * 100)}%` }} />
                    </div>
                    <span className="t2">
                      {nombre(infos.libreGo)} Go libres sur {nombre(infos.totalGo)} Go{infos.disque && ` (disque ${infos.disque})`}
                    </span>
                  </>
                )}
              </div>
              <button type="button" className="btn" onClick={changerDossier}>
                Changer…
              </button>
            </section>
          ) : etape === "logiciel" ? (
            <div role="radiogroup" aria-label="Logiciel du serveur">
              {Object.entries(LOGICIELS).map(([groupe, liste]) => (
                <div key={groupe}>
                  <div className="wz-sec">{groupe}</div>
                  <div className="opts">
                    {liste.map((l) => (
                      <button
                        key={l.id}
                        type="button"
                        role="radio"
                        aria-checked={l.id === logiciel}
                        disabled={!l.pret}
                        className={"opt" + (l.id === logiciel ? " sel" : "")}
                        onClick={() => {
                          setLogiciel(l.id);
                          setRam(Math.min(l.ram, ramMax));
                        }}
                      >
                        {LOGOS[l.id] ? (
                          <span className={"opt-ico logo" + (LOGOS[l.id].plein ? " plein" : "")}>
                            <img src={LOGOS[l.id].src} alt="" draggable={false} />
                          </span>
                        ) : (
                          <span className="opt-ico logo" style={{ color: l.couleur }}>
                            <l.Icone size={19} stroke={2} aria-hidden />
                          </span>
                        )}
                        <span className="opt-txt">
                          <b>
                            {l.nom}
                            {l.recommande && <span className="rec">Recommandé</span>}
                            {!l.pret && <span className="bientot">Bientôt</span>}
                          </b>
                          <span>{l.desc}</span>
                        </span>
                        <span className="opt-ck">
                          <IconCheck size={16} stroke={2.4} aria-hidden />
                        </span>
                      </button>
                    ))}
                  </div>
                  {groupe === "Mods et plugins ensemble" && <p className="wz-note">Moins stables.</p>}
                </div>
              ))}
              <button type="button" className="imp" onClick={importer}>
                <IconFolder size={16} stroke={2} aria-hidden />
                <b>Importer un serveur existant</b>
                <IconArrowRight size={15} stroke={2} aria-hidden />
              </button>
            </div>
          ) : etape === "version" ? (
            <>
              {logiciel === "Vanilla" && (
                <section className="surf">
                  <div className="row">
                    <div className="lbl">
                      <span>Afficher les snapshots</span>
                      <span className="t2">Risqué pour un monde</span>
                    </div>
                    <button
                      type="button"
                      role="switch"
                      aria-checked={snapshots}
                      aria-label="Afficher les snapshots"
                      className={"sw" + (snapshots ? " on" : "")}
                      onClick={() => setSnapshots(!snapshots)}
                    >
                      <i />
                    </button>
                  </div>
                </section>
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
                      <input
                        type="search"
                        value={filtre}
                        placeholder={`Rechercher parmi ${liste.length} versions`}
                        spellCheck={false}
                        onChange={(e) => setFiltre(e.target.value)}
                      />
                    </label>
                  )}
                  <section className="surf versions" role="radiogroup" aria-label="Version de Minecraft">
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
                        {v.id === derniere && <span className="rec">Dernière</span>}
                        {v.instable && <span className="instable">Instable</span>}
                        {v.java && <span className="j">Java {v.java}</span>}
                      </button>
                    ))}
                  </section>
                </>
              )}
            </>
          ) : etape === "monde" ? (
            <>
              <label className="bigname">
                <img className="wz-grand" key={cube} src={image(cube)} alt="" draggable={false} />
                <span>Nom du serveur</span>
                <input
                  type="text"
                  value={nom}
                  maxLength={40}
                  placeholder="Survie du week-end"
                  spellCheck={false}
                  autoFocus
                  onChange={(e) => setNom(e.target.value)}
                />
              </label>
              <div className="wz-sec">Icône</div>
              <section className="surf wz-icone">
                <ChoixIcone valeur={cube} onChoisir={setCouleur} repliable />
              </section>
              <div className="wz-sec">Mode de jeu</div>
              <div className="modes" role="radiogroup" aria-label="Mode de jeu">
                {MODES.map((m) => (
                  <button
                    key={m.id}
                    type="button"
                    role="radio"
                    aria-checked={m.id === mode}
                    className={"mode-c" + (m.id === mode ? " on" : "")}
                    onClick={() => setMode(m.id)}
                  >
                    <m.Icone size={18} stroke={2} aria-hidden />
                    <b>{m.nom}</b>
                    <span>{m.desc}</span>
                  </button>
                ))}
              </div>
              <div className="wz-sec">Partie</div>
              <section className="surf">
                <div className={"row" + (mode === "hardcore" ? " off" : "")}>
                  <span>Difficulté</span>
                  <Segment
                    etiquette="Difficulté"
                    options={DIFFICULTES}
                    valeur={mode === "hardcore" ? "difficile" : difficulte}
                    off={mode === "hardcore"}
                    onChoisir={setDifficulte}
                  />
                </div>
                <div className="row">
                  <span>Joueurs en même temps</span>
                  <div className="stepper">
                    <button type="button" aria-label="Moins" onClick={() => setJoueurs(Math.max(1, joueurs - 1))}>
                      −
                    </button>
                    <span className="tnum">
                      <b key={joueurs}>{joueurs}</b>
                    </span>
                    <button type="button" aria-label="Plus" onClick={() => setJoueurs(Math.min(100, joueurs + 1))}>
                      +
                    </button>
                  </div>
                </div>
                <div className="row">
                  <div className="lbl">
                    <span>Mémoire réservée</span>
                    <span className={"t2" + (ram < choisi.ram ? " warn" : "")}>
                      {ram < choisi.ram ? "Un peu juste. " : ""}
                      {infos ? `Ton PC a ${nombre(Math.round(infos.memoireGo))} Go. ` : ""}Conseillé : {choisi.ram} Go.
                    </span>
                  </div>
                  <div className="ram">
                    <Curseur min={1} max={ramMax} valeur={ram} onChange={setRam} etiquette="Mémoire" unite="Go" />
                    <b className="tnum">{ram} Go</b>
                  </div>
                </div>
              </section>
              <div className="wz-sec">Accès</div>
              <section className="surf">
                <div className="row">
                  <div className="lbl">
                    <span id="wz-amis">Ouvert à tes amis</span>
                    <span className="t2">Tu pourras changer ça plus tard dans Paramètres.</span>
                  </div>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={amis}
                    aria-labelledby="wz-amis"
                    className={"sw" + (amis ? " on" : "")}
                    onClick={() => setAmis(!amis)}
                  >
                    <i />
                  </button>
                </div>
              </section>
              <section className={"surf eula" + (eula ? "" : " todo")}>
                <label className="chk">
                  <input type="checkbox" checked={eula} onChange={(e) => setEula(e.target.checked)} />
                  <span>J'accepte le contrat de licence de Minecraft (EULA)</span>
                </label>
                <button type="button" className="lien" onClick={() => openUrl("https://www.minecraft.net/eula").catch(() => {})}>
                  Lire le contrat
                </button>
              </section>
            </>
          ) : (
            <>
              <section className="surf sum">
                <div className="sum-top">
                  <img className="wz-grand" src={image(cube)} alt="" draggable={false} />
                  <div>
                    <b>{nomPropre}</b>
                    <span>
                      {choisi.nom} {version}
                      {java ? ` · Java ${java}` : ""}
                    </span>
                  </div>
                </div>
                <div className="facts">
                  {[
                    ["Mode de jeu", MODES.find((m) => m.id === mode)?.nom],
                    ["Difficulté", mode === "hardcore" ? "Difficile" : DIFFICULTES.find((d) => d.id === difficulte)?.nom],
                    ["Joueurs", `${joueurs} en même temps`],
                    ["Mémoire", `${ram} Go`],
                    ["Accès", amis ? "Ouvert à tes amis" : "Sur ce PC"],
                    ["Dossier", infos ? `${infos.dossier}\\…` : "…"],
                  ].map(([k, v]) => (
                    <div className="fact" key={k}>
                      <span>{k}</span>
                      {k === "Dossier" ? (
                        // Un long chemin est coupé au début : on garde la fin, la plus parlante.
                        <b className="chemin" title={v}>
                          <span dir="ltr">{v}</span>
                        </b>
                      ) : (
                        <b title={v}>{v}</b>
                      )}
                    </div>
                  ))}
                </div>
              </section>
              <div className="wz-sec">Ce que BHOST va faire</div>
              <ol className="plan">
                {taches.map((t, i) => (
                  <li key={t}>
                    <span className="plan-n">{i + 1}</span>
                    {t}
                  </li>
                ))}
              </ol>
            </>
          )}

          {erreur && (
            <p className="alerte" role="alert">
              {erreur}
            </p>
          )}
        </div>
      </div>

      {/* La capsule du bas : Annuler, Retour, Continuer. */}
      {(!enCreation || creation.erreur) && (
        <div className="dock">
          {/* « Annuler » est à part : il ne fait pas avancer l'assistant. */}
          {onAnnuler && !creation?.erreur && (
            <div className="g wz-nav" data-lens="bar">
              <button type="button" data-gel className="ab" onClick={onAnnuler}>
                Annuler
              </button>
            </div>
          )}
          <div className="g wz-nav" data-lens="bar" role="group" aria-label="Navigation de l'assistant">
            {creation?.erreur ? (
              <>
                <button type="button" data-gel className="ab" onClick={() => setCreation(null)}>
                  <IconArrowLeft size={15} stroke={2} aria-hidden />
                  Retour
                </button>
                <button type="button" data-gel className="ab go" onClick={creer}>
                  Réessayer
                </button>
              </>
            ) : (
              <>
                {rang > 0 && (
                  <button type="button" data-gel className="ab" onClick={() => aller(etapes[rang - 1])}>
                    <IconArrowLeft size={15} stroke={2} aria-hidden />
                    Retour
                  </button>
                )}
                <button
                  type="button"
                  data-gel
                  className="ab go"
                  disabled={!peutContinuer}
                  onClick={() => (etape === "resume" ? creer() : aller(etapes[rang + 1]))}
                >
                  {etape === "resume" ? "Créer le serveur" : "Continuer"}
                  <IconArrowRight size={15} stroke={2} aria-hidden />
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </>
  );
}
