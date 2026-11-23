import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent as SourisReact } from "react";
import { createPortal } from "react-dom";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  IconAlertTriangle,
  IconArrowLeft,
  IconChevronRight,
  IconCopy,
  IconDots,
  IconExternalLink,
  IconFile,
  IconFileText,
  IconFileZip,
  IconFolder,
  IconFolderOpen,
  IconFolderPlus,
  IconInfoCircle,
  IconLock,
  IconPencil,
  IconPhoto,
  IconSettings,
  IconTrash,
  IconUpload,
} from "@tabler/icons-react";
import Segment from "../components/Segment";
import Confirmer from "../components/Confirmer";
import Renommer from "../components/Renommer";
import { poids } from "../components/Fenetre";
import {
  changerListeBlanche,
  choisirFichiers,
  choisirImage,
  ecrireFichier,
  fichiersServeur,
  iconeServeur,
  iconeDuJeu,
  importerFichiers,
  ecouterTravail,
  lireFichier,
  montrerFichier,
  nouveauDossier,
  renommerFichier,
  supprimerFichiers,
  parametresServeur,
  proprietesServeur,
  reglerPartie,
  reglerProprietes,
  type Element,
  type FichierLu,
  type Partie,
} from "../api";
import type { Onglet, Serveur } from "../types";
import "../components/Menus.css";
import "./Fichiers.css";

type Props = {
  serveur: Serveur;
  onChange: (s: Serveur) => void;
  onOnglet: (o: Onglet) => void;
  onErreur: (texte: string) => void;
  onInfo: (texte: string) => void;
};

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n’a pas marché.");
const GERES = ["ops.json", "whitelist.json", "banned-players.json", "banned-ips.json", "usercache.json"];
const TEXTES = /\.(txt|properties|ya?ml|json5?|toml|cfg|conf|ini|md|csv|sk|mcmeta|lang|bat|cmd|sh|xml|snbt|mcfunction)$/i;

function icone(e: { nom: string; dossier: boolean }) {
  if (e.dossier) return IconFolder;
  if (e.nom === "server.properties") return IconSettings;
  if (/\.png$/i.test(e.nom)) return IconPhoto;
  if (/\.(jar|zip|gz|tar)$/i.test(e.nom)) return IconFileZip;
  if (TEXTES.test(e.nom) || /\.log$/i.test(e.nom)) return IconFileText;
  return IconFile;
}

const date = (ms: number | null) => (ms ? new Date(ms).toLocaleString("fr-FR", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) : "");

// L’onglet Fichiers : le dossier du serveur à gauche, le fichier choisi à droite (lecture, modification, réglages faciles).
export default function Fichiers({ serveur, onChange, onOnglet, onErreur, onInfo }: Props) {
  const [dossier, setDossier] = useState("");
  const [elements, setElements] = useState<Element[] | null>(null);
  const [chemin, setChemin] = useState<string | null>("server.properties");
  const [fichier, setFichier] = useState<FichierLu | null>(null);
  const [brouillon, setBrouillon] = useState<string | null>(null);
  const [vueFacile, setVueFacile] = useState(true);
  const [survol, setSurvol] = useState(false);
  // Clic droit (ou « … ») sur un élément, et les fenêtres qui en découlent.
  const [menu, setMenu] = useState<{ x: number; y: number; cible: Cible } | null>(null);
  const [aRenommer, setARenommer] = useState<Cible | null>(null);
  const [aSupprimer, setASupprimer] = useState<Cible | null>(null);
  const [creerDossier, setCreerDossier] = useState(false);
  const allume = serveur.statut !== "arrete" && serveur.statut !== "plante";

  // Une réponse arrivée en retard (on a déjà changé de dossier ou de fichier) est ignorée :
  // sinon le contenu d’un fichier pourrait s’afficher, et être enregistré, sous le nom d’un autre.
  const dossierDemande = useRef(dossier);
  const cheminDemande = useRef<string | null>(chemin);
  const relire = (d = dossier) => {
    dossierDemande.current = d;
    return fichiersServeur(serveur.id, d).then(
      (l) => dossierDemande.current === d && setElements(l),
      (e) => onErreur(phrase(e)),
    );
  };
  useEffect(() => {
    setElements(null);
    relire(dossier);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id, dossier]);

  const ouvrirFichier = (c: string | null) => {
    setChemin(c);
    setBrouillon(null);
    setFichier(null);
    cheminDemande.current = c;
    if (c)
      lireFichier(serveur.id, c).then(
        (f) => cheminDemande.current === c && setFichier(f),
        (e) => onErreur(phrase(e)),
      );
  };
  useEffect(() => {
    ouvrirFichier(chemin);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id]);

  // Glisser des fichiers du PC dans la fenêtre : ils vont dans le dossier affiché.
  const dossierActuel = useRef(dossier);
  dossierActuel.current = dossier;
  useEffect(() => {
    let fin: (() => void) | null = null;
    let ferme = false;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over") setSurvol(true);
        else if (e.payload.type === "leave") setSurvol(false);
        else if (e.payload.type === "drop") {
          setSurvol(false);
          importer(e.payload.paths, dossierActuel.current);
        }
      })
      .then((arreter) => (ferme ? arreter() : (fin = arreter)))
      .catch(() => {});
    return () => {
      ferme = true;
      fin?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id]);

  const importer = (sources: string[], d: string) => {
    if (!sources.length) return;
    // Machine dédiée : les fichiers partent de ce PC, on montre l’envoi.
    const fin = serveur.machine ? ecouterTravail((qui, etape, pourcent) => qui === serveur.id && onInfo(`${etape}… ${pourcent} %`)) : () => {};
    importerFichiers(serveur.id, d, sources).then(
      (n) => {
        fin();
        onInfo(n > 1 ? `${n} éléments importés` : "Importé");
        relire(d);
      },
      (e) => {
        fin();
        onErreur(phrase(e));
      },
    );
  };

  // Le fichier ouvert est-il cet élément, ou dans ce dossier ?
  const touche = (c: string) => chemin !== null && (chemin === c || chemin.startsWith(c + "/"));

  const supprimer = (c: Cible) =>
    supprimerFichiers(serveur.id, [c.chemin]).then(
      () => {
        onInfo(`« ${c.nom} » supprimé`);
        if (touche(c.chemin)) ouvrirFichier(null);
        relire();
      },
      (e) => {
        onErreur(phrase(e));
        relire();
      },
    );

  const renommer = (c: Cible) => (nom: string) =>
    renommerFichier(serveur.id, c.chemin, nom).then((nouveau) => {
      onInfo("Renommé");
      if (touche(c.chemin) && chemin !== null) ouvrirFichier(nouveau + chemin.slice(c.chemin.length));
      relire();
    });

  const creer = (nom: string) =>
    nouveauDossier(serveur.id, dossier, nom).then(() => {
      onInfo("Dossier créé");
      relire();
    });

  const nomFichier = chemin?.split("/").pop() ?? "";
  const gere = GERES.includes(nomFichier);
  const journal = /\.log$/i.test(nomFichier);
  const proprietes = chemin === "server.properties";
  const lectureSeule = gere || journal;
  const monde = elements?.some((e) => e.nom === "level.dat") || /^world(_nether|_the_end)?(\/|$)/.test(dossier);
  const segments = dossier ? dossier.split("/") : [];

  const enregistrer = () => {
    if (chemin === null || brouillon === null) return;
    ecrireFichier(serveur.id, chemin, brouillon).then(
      () => {
        onInfo(`${nomFichier} enregistré`);
        ouvrirFichier(chemin);
      },
      (e) => onErreur(phrase(e)),
    );
  };

  const bandeau =
    gere && nomFichier !== "usercache.json"
      ? { texte: "Géré par BHOST, via l’onglet Joueurs.", bouton: "Ouvrir Joueurs", aller: () => onOnglet("joueurs") }
      : nomFichier === "latest.log"
        ? { texte: "Le journal en direct est dans la Console.", bouton: "Ouvrir la Console", aller: () => onOnglet("console") }
        : null;

  const texte = fichier?.genre === "texte" ? fichier.texte : "";
  const modifie = brouillon !== null && brouillon !== texte;

  return (
    <div className="fx">
      <section className={"surf fx-tree" + (survol ? " survol" : "")} aria-label="Fichiers du serveur">
        <div className="fx-top">
          {dossier && (
            <button type="button" className="icb" aria-label="Dossier parent" title="Dossier parent" onClick={() => setDossier(segments.slice(0, -1).join("/"))}>
              <IconArrowLeft size={15} stroke={2} aria-hidden />
            </button>
          )}
          <nav className="crumbs" aria-label="Chemin">
            <button type="button" className={dossier ? "" : "cur"} onClick={() => setDossier("")}>
              {serveur.nom}
            </button>
            {segments.map((s, i) => (
              <span key={i} className="cr">
                <IconChevronRight className="cr-sep" size={12} stroke={2} aria-hidden />
                <button type="button" className={i === segments.length - 1 ? "cur" : ""} onClick={() => setDossier(segments.slice(0, i + 1).join("/"))}>
                  {s}
                </button>
              </span>
            ))}
          </nav>
          <button type="button" className="icb" aria-label="Nouveau dossier" title="Nouveau dossier" onClick={() => setCreerDossier(true)}>
            <IconFolderPlus size={15} stroke={2} aria-hidden />
          </button>
          <button type="button" className="icb" aria-label="Importer des fichiers" title="Importer des fichiers" onClick={() => choisirFichiers().then((l) => importer(l, dossier))}>
            <IconUpload size={15} stroke={2} aria-hidden />
          </button>
          {!serveur.machine && (
            <button type="button" className="icb" aria-label="Ouvrir dans l’Explorateur" title="Ouvrir dans l’Explorateur" onClick={() => montrerFichier(serveur.id, dossier).catch((e) => onErreur(phrase(e)))}>
              <IconFolderOpen size={15} stroke={2} aria-hidden />
            </button>
          )}
        </div>
        {allume && monde && (
          <div className="fx-warn">
            <IconAlertTriangle size={14} stroke={2} aria-hidden />
            <span>Arrête le serveur avant de toucher au monde.</span>
          </div>
        )}
        <div className="fx-list" role="listbox" aria-label={dossier || serveur.nom}>
          {elements === null ? (
            <div className="empty">Lecture…</div>
          ) : elements.length === 0 ? (
            <div className="empty">Ce dossier est vide.</div>
          ) : (
            elements.map((e) => {
              const c = (dossier ? dossier + "/" : "") + e.nom;
              const choisi = !e.dossier && c === chemin;
              const Icone = icone(e);
              const cible: Cible = { chemin: c, nom: e.nom, dossier: e.dossier };
              const ouvrirMenu = (x: number, y: number) => setMenu({ x, y, cible });
              return (
                <div
                  key={e.nom}
                  className={"fl-ligne" + (choisi ? " sel" : "") + (menu?.cible.chemin === c ? " menu" : "")}
                  onContextMenu={(ev) => {
                    ev.preventDefault();
                    ouvrirMenu(ev.clientX, ev.clientY);
                  }}
                >
                  <button
                    type="button"
                    role="option"
                    aria-selected={choisi}
                    className={"fl" + (e.dossier ? " isdir" : "") + (choisi ? " sel" : "")}
                    onClick={() => (e.dossier ? setDossier(c) : ouvrirFichier(c))}
                    onKeyDown={(ev: KeyboardEvent) => {
                      if (ev.key === "Delete") setASupprimer(cible);
                      else if (ev.key === "F2") setARenommer(cible);
                    }}
                  >
                    <Icone className="fl-ic" size={16} stroke={1.8} aria-hidden />
                    <span className="fl-n">{e.nom}</span>
                    <span className="fl-m">{e.dossier ? `${e.taille} élément${e.taille > 1 ? "s" : ""}` : poids(e.taille)}</span>
                  </button>
                  <button
                    type="button"
                    className="icb fl-plus"
                    aria-label={`Actions pour ${e.nom}`}
                    title="Actions"
                    onClick={(ev: SourisReact) => {
                      const r = ev.currentTarget.getBoundingClientRect();
                      ouvrirMenu(r.right - 4, r.bottom + 4);
                    }}
                  >
                    <IconDots size={15} stroke={2} aria-hidden />
                  </button>
                </div>
              );
            })
          )}
        </div>
        <div className="fx-foot">
          {survol ? "Lâche pour importer ici" : `${elements?.length ?? 0} élément${(elements?.length ?? 0) > 1 ? "s" : ""} · dépose des fichiers ici`}
        </div>
      </section>

      <section className="surf fx-ed" aria-label={nomFichier || "Aucun fichier"}>
        {chemin === null ? (
          <div className="fx-none">
            <span className="fx-big">
              <IconFile size={30} stroke={1.6} aria-hidden />
            </span>
            <b>Choisis un fichier</b>
          </div>
        ) : (
          <>
            <div className="fx-eh">
              <span className="fx-fi">
                {(() => {
                  const I = icone({ nom: nomFichier, dossier: false });
                  return <I size={18} stroke={1.8} aria-hidden />;
                })()}
              </span>
              <div className="fx-nm">
                <div className="fx-fn">{nomFichier}</div>
                <div className="fx-fp">{[serveur.nom, ...chemin.split("/")].join(" › ")}</div>
              </div>
              {proprietes && (
                <Segment
                  etiquette="Affichage"
                  options={[
                    { id: "facile", nom: "Réglages faciles" },
                    { id: "brut", nom: "Texte brut" },
                  ]}
                  valeur={vueFacile ? "facile" : "brut"}
                  onChoisir={(v) => {
                    setVueFacile(v === "facile");
                    ouvrirFichier(chemin);
                  }}
                />
              )}
              {lectureSeule && (
                <span className="fx-ro">
                  <IconLock size={12} stroke={2} aria-hidden />
                  Lecture seule
                </span>
              )}
              {!serveur.machine && (
                <button type="button" className="icb" aria-label="Afficher dans l’Explorateur" title="Afficher dans l’Explorateur" onClick={() => montrerFichier(serveur.id, chemin).catch((e) => onErreur(phrase(e)))}>
                  <IconExternalLink size={15} stroke={2} aria-hidden />
                </button>
              )}
            </div>

            {bandeau && (
              <div className="fx-ban">
                <IconInfoCircle size={15} stroke={2} aria-hidden />
                <span>{bandeau.texte}</span>
                <button type="button" className="btn" onClick={bandeau.aller}>
                  {bandeau.bouton}
                </button>
              </div>
            )}

            {fichier === null ? (
              <div className="fx-none">
                <span className="spin" aria-hidden />
              </div>
            ) : proprietes && vueFacile ? (
              <ReglagesFaciles serveur={serveur} onChange={onChange} onErreur={onErreur} onInfo={onInfo} />
            ) : fichier.genre === "texte" ? (
              lectureSeule ? (
                <pre className="fx-code ro">{texte}</pre>
              ) : (
                <textarea
                  className="fx-code"
                  spellCheck={false}
                  aria-label={nomFichier}
                  value={brouillon ?? texte}
                  onChange={(e) => setBrouillon(e.target.value)}
                  onKeyDown={(e) => {
                    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
                      e.preventDefault();
                      enregistrer();
                    }
                  }}
                />
              )
            ) : fichier.genre === "image" ? (
              <div className="fx-none">
                <img className="fx-img" src={fichier.adresse} alt={nomFichier} />
                {nomFichier === "server-icon.png" && (
                  <>
                    <b>Icône du serveur</b>
                    <span>Image carrée, mise en 64 × 64.</span>
                    <ChoisirIcone id={serveur.id} onFini={() => ouvrirFichier(chemin)} onErreur={onErreur} onInfo={onInfo} />
                  </>
                )}
              </div>
            ) : (
              <div className="fx-none">
                <span className="fx-big">
                  <IconFileZip size={30} stroke={1.6} aria-hidden />
                </span>
                <b>{fichier.genre === "tropGros" ? "Trop gros pour être affiché ici" : infosBinaire(nomFichier, chemin, serveur).titre}</b>
                {!serveur.machine && <span>{fichier.genre === "tropGros" ? "Ouvre-le depuis l’Explorateur." : infosBinaire(nomFichier, chemin, serveur).texte}</span>}
                {!serveur.machine && (
                  <button type="button" className="btn" onClick={() => montrerFichier(serveur.id, chemin).catch((e) => onErreur(phrase(e)))}>
                    <IconFolderOpen size={14} stroke={2} aria-hidden />
                    Afficher dans l’Explorateur
                  </button>
                )}
              </div>
            )}

            <div className="fx-ef">
              <span className={modifie ? "warn" : ""}>
                {modifie
                  ? "Modifications non enregistrées"
                  : proprietes && vueFacile
                    ? allume
                      ? "Enregistré. Redémarre pour appliquer."
                      : "Enregistré automatiquement."
                    : fichier
                      ? `${poids(fichier.taille)} · modifié le ${date(fichier.modifie)}${fichier.genre === "texte" && !lectureSeule ? ". Appliqué au prochain démarrage." : ""}`
                      : ""}
              </span>
              {modifie && (
                <>
                  <button type="button" className="btn" onClick={() => setBrouillon(null)}>
                    Annuler
                  </button>
                  <button type="button" className="btn acc" onClick={enregistrer}>
                    Enregistrer
                  </button>
                </>
              )}
            </div>
          </>
        )}
      </section>

      {menu && (
        <MenuFichier
          x={menu.x}
          y={menu.y}
          cible={menu.cible}
          logiciel={menu.cible.chemin === serveur.jar}
          surCePc={!serveur.machine}
          onFermer={() => setMenu(null)}
          onOuvrir={() => (menu.cible.dossier ? setDossier(menu.cible.chemin) : ouvrirFichier(menu.cible.chemin))}
          onRenommer={() => setARenommer(menu.cible)}
          onMontrer={() => montrerFichier(serveur.id, menu.cible.chemin).catch((e) => onErreur(phrase(e)))}
          onSupprimer={() => setASupprimer(menu.cible)}
        />
      )}
      {aRenommer && (
        <Renommer
          nom={aRenommer.nom}
          titre={aRenommer.dossier ? "Renommer le dossier" : "Renommer le fichier"}
          longueur={120}
          onEnregistrer={renommer(aRenommer)}
          onFermer={() => setARenommer(null)}
        />
      )}
      {creerDossier && <Renommer nom="" titre="Nouveau dossier" bouton="Créer" longueur={120} onEnregistrer={creer} onFermer={() => setCreerDossier(false)} />}
      {aSupprimer && (
        <Confirmer
          titre={`Supprimer « ${aSupprimer.nom} » ?`}
          texte={
            (aSupprimer.dossier ? "Le dossier et tout ce qu’il contient. " : "") +
            (serveur.machine ? "Ce sera effacé pour de bon." : "Ça ira dans la Corbeille de Windows.") +
            (allume ? " Le serveur tourne : mieux vaut l’arrêter avant." : "")
          }
          bouton="Supprimer"
          onOui={() => supprimer(aSupprimer)}
          onFermer={() => setASupprimer(null)}
        />
      )}
    </div>
  );
}

type Cible = { chemin: string; nom: string; dossier: boolean };

const MARGE = 8; // le menu ne touche jamais le bord de la fenêtre

// Le menu du clic droit (ou du bouton « … ») sur un fichier ou un dossier.
function MenuFichier({
  x,
  y,
  cible,
  logiciel,
  surCePc,
  onFermer,
  onOuvrir,
  onRenommer,
  onMontrer,
  onSupprimer,
}: {
  x: number;
  y: number;
  cible: Cible;
  logiciel: boolean; // le .jar du serveur : ni renommé ni supprimé ici
  surCePc: boolean;
  onFermer: () => void;
  onOuvrir: () => void;
  onRenommer: () => void;
  onMontrer: () => void;
  onSupprimer: () => void;
}) {
  const menu = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState({ x, y });

  // S’il dépasse en bas ou à droite, on le décale pour qu’il reste dans la fenêtre.
  useLayoutEffect(() => {
    const el = menu.current;
    if (!el) return;
    setPlace({
      x: Math.max(MARGE, Math.min(x, window.innerWidth - el.offsetWidth - MARGE)),
      y: Math.max(MARGE, Math.min(y, window.innerHeight - el.offsetHeight - MARGE)),
    });
    el.querySelector<HTMLButtonElement>("button")?.focus();
  }, [x, y]);

  useEffect(() => {
    window.addEventListener("resize", onFermer);
    window.addEventListener("blur", onFermer);
    return () => {
      window.removeEventListener("resize", onFermer);
      window.removeEventListener("blur", onFermer);
    };
  }, [onFermer]);

  const touche = (e: KeyboardEvent) => {
    if (e.key === "Escape") return onFermer();
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const choix = [...(menu.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
    const i = choix.indexOf(document.activeElement as HTMLButtonElement);
    choix[(i + (e.key === "ArrowDown" ? 1 : choix.length - 1)) % choix.length]?.focus();
  };

  const choisir = (action: () => void) => () => {
    onFermer();
    action();
  };
  const bloque = logiciel ? "C’est le logiciel du serveur : sa version se change dans Paramètres." : undefined;

  return createPortal(
    <>
      <div
        className="ctx-catch"
        onClick={onFermer}
        onContextMenu={(e) => {
          e.preventDefault();
          onFermer();
        }}
      />
      <div ref={menu} className="g ctx fx-menu" data-lens="panel" role="menu" aria-label={cible.nom} style={{ left: place.x, top: place.y }} onKeyDown={touche}>
        <button type="button" role="menuitem" className="pm-item" onClick={choisir(onOuvrir)}>
          {cible.dossier ? <IconFolderOpen size={16} stroke={2} aria-hidden /> : <IconFileText size={16} stroke={2} aria-hidden />}
          Ouvrir
        </button>
        <button type="button" role="menuitem" className="pm-item" disabled={logiciel} title={bloque} onClick={choisir(onRenommer)}>
          <IconPencil size={16} stroke={2} aria-hidden />
          Renommer…
        </button>
        {surCePc && (
          <button type="button" role="menuitem" className="pm-item" onClick={choisir(onMontrer)}>
            <IconExternalLink size={16} stroke={2} aria-hidden />
            Afficher dans l’Explorateur
          </button>
        )}
        <div className="pm-sep" role="separator" />
        <button type="button" role="menuitem" className="pm-item danger" disabled={logiciel} title={bloque} onClick={choisir(onSupprimer)}>
          <IconTrash size={16} stroke={2} aria-hidden />
          Supprimer…
        </button>
      </div>
    </>,
    document.body,
  );
}

// Ce qu’on dit d’un fichier que BHOST n’ouvre pas.
function infosBinaire(nom: string, chemin: string, serveur: Serveur) {
  if (chemin === serveur.jar) return { titre: "Le logiciel du serveur", texte: `C’est ${serveur.logiciel} lui-même. Sa version se change dans Paramètres.` };
  if (/^(plugins|mods)\/[^/]+\.jar$/i.test(chemin)) return { titre: chemin.startsWith("mods") ? "Fichier d’un mod" : "Fichier d’un plugin", texte: "Remplace-le en important une autre version." };
  if (/\.log\.gz$/i.test(nom)) return { titre: "Ancien journal compressé", texte: "À ouvrir depuis l’Explorateur." };
  if (/\.(dat|mca|dat_old|nbt)$/i.test(nom)) return { titre: "Fichier du monde", texte: "Fichier interne de Minecraft." };
  return { titre: "Fichier non lisible ici", texte: "À ouvrir depuis l’Explorateur." };
}

function ChoisirIcone({ id, onFini, onErreur, onInfo }: { id: string; onFini: () => void; onErreur: (t: string) => void; onInfo: (t: string) => void }) {
  return (
    <button
      type="button"
      className="btn"
      onClick={async () => {
        const image = await choisirImage();
        if (!image) return;
        iconeServeur(id, image).then(
          () => {
            onInfo("Icône changée");
            onFini();
          },
          (e) => onErreur(phrase(e)),
        );
      }}
    >
      Choisir une image…
    </button>
  );
}

// ---- server.properties en « Réglages faciles » ----

const MODES = [
  { id: "survie", nom: "Survie" },
  { id: "creatif", nom: "Créatif" },
  { id: "aventure", nom: "Aventure" },
];
const DIFFICULTES = [
  { id: "paisible", nom: "Paisible" },
  { id: "facile", nom: "Facile" },
  { id: "normale", nom: "Normale" },
  { id: "difficile", nom: "Difficile" },
];

// « 1.18 » et après : distance de simulation.
function depuis118(version: string) {
  const n = version.split(".").map(Number);
  if (n.some(isNaN) || !version) return true;
  return n[0] > 1 || (n[0] === 1 && n[1] >= 18);
}

// Le texte d’un motd : les codes de couleur (§a…) et les échappements (é) en moins.
function motdLisible(motd: string) {
  return motd.replace(/\\u([0-9a-fA-F]{4})/g, (_, h) => String.fromCharCode(parseInt(h, 16))).replace(/§./g, "");
}

function ReglagesFaciles({ serveur, onChange, onErreur, onInfo }: Omit<Props, "onOnglet">) {
  const [p, setP] = useState<Record<string, string> | null>(null);
  const [partie, setPartie] = useState<Partie | null>(null);
  const [motd, setMotd] = useState("");
  const [port, setPort] = useState("");
  const [icone, setIcone] = useState<string | null>(null);
  const [iconePerso, setIconePerso] = useState(false);

  const lireIcone = (remettre = false) => {
    const lire = () => lireFichier(serveur.id, "server-icon.png").then((f) => setIcone(f.genre === "image" ? f.adresse : null), () => setIcone(null));
    iconeDuJeu(serveur.id, remettre).then(
      (perso) => {
        setIconePerso(perso);
        if (remettre) onInfo("Icône de BHOST remise");
        lire();
      },
      (e) => {
        onErreur(phrase(e));
        lire();
      },
    );
  };
  useEffect(() => {
    proprietesServeur(serveur.id).then((l) => {
      setP(l);
      setMotd(motdLisible(l["motd"] ?? ""));
      setPort(l["server-port"] ?? "25565");
    }, (e) => onErreur(phrase(e)));
    parametresServeur(serveur.id).then((i) => setPartie(i.partie), () => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id]);
  // L’icône dans Minecraft suit celle de BHOST (sauf image à soi).
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => lireIcone(), [serveur.id, serveur.couleur]);

  // Relit tout (server.properties et la partie) : chaque réglage voit ce que les autres ont changé.
  const relireTout = () => {
    proprietesServeur(serveur.id).then(setP, () => {});
    parametresServeur(serveur.id).then((i) => setPartie(i.partie), () => {});
  };
  const regler = (valeurs: Record<string, string>) => {
    if (!p) return;
    setP({ ...p, ...valeurs });
    reglerProprietes(serveur.id, valeurs).then(
      (l) => {
        setP(l);
        relireTout();
        // Port et joueurs max : l’Accueil et la barre latérale les montrent aussi.
        const port = Number(l["server-port"]);
        const joueursMax = Number(l["max-players"]);
        if ((port && port !== serveur.port) || (joueursMax && joueursMax !== serveur.joueursMax)) {
          onChange({ ...serveur, port: port || serveur.port, joueursMax: joueursMax || serveur.joueursMax });
        }
      },
      (e) => {
        onErreur(phrase(e));
        relireTout();
      },
    );
  };
  const changerPartie = (change: Partial<Partie>) => {
    if (!partie) return;
    const nouvelle = { ...partie, ...change };
    setPartie(nouvelle);
    reglerPartie(serveur.id, nouvelle).then(
      (s) => {
        onChange(s);
        relireTout();
      },
      (e) => {
        onErreur(phrase(e));
        relireTout();
      },
    );
  };

  if (!p || !partie) return <div className="fx-body"><div className="empty">Lecture…</div></div>;

  const vrai = (k: string, defaut = false) => (p[k] === undefined ? defaut : p[k] === "true");
  const nombreDe = (k: string, defaut: number) => {
    const n = parseInt(p[k] ?? "", 10);
    return Number.isFinite(n) ? n : defaut;
  };
  const interrupteur = (cle: string, nom: string, defaut: boolean, indice?: string, action?: (v: boolean) => void) => (
    <div className="row" key={cle}>
      <div className="lbl">
        <span>
          {nom}
          <code className="pk">{cle}</code>
        </span>
        {indice && <span className="t2">{indice}</span>}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={vrai(cle, defaut)}
        aria-label={nom}
        className={"sw" + (vrai(cle, defaut) ? " on" : "")}
        onClick={() => (action ? action(!vrai(cle, defaut)) : regler({ [cle]: String(!vrai(cle, defaut)) }))}
      >
        <i />
      </button>
    </div>
  );
  const compteur = (cle: string, nom: string, defaut: number, min: number, max: number, pas: number, unite: (n: number) => string) => (
    <div className="row" key={cle}>
      <span>
        {nom}
        <code className="pk">{cle}</code>
      </span>
      <Compteur valeur={nombreDe(cle, defaut)} min={min} max={max} pas={pas} unite={unite} nom={nom} onChoisir={(n) => regler({ [cle]: String(n) })} />
    </div>
  );
  const graine = p["level-seed"] || "";

  return (
    <div className="fx-body">
      <div className="grp-t">Accueil</div>
      <div className="fx-card">
        <div className="row">
          <label htmlFor="fx-motd">
            Message d’accueil
            <code className="pk">motd</code>
          </label>
          <input
            id="fx-motd"
            className="field fx-texte"
            type="text"
            maxLength={120}
            value={motd}
            spellCheck={false}
            onChange={(e) => setMotd(e.target.value)}
            onBlur={() => motd !== motdLisible(p["motd"] ?? "") && regler({ motd })}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </div>
        <div className="motd-row">
          <div className="motd" aria-label="Aperçu dans la liste des serveurs du jeu">
            {icone ? <img className="motd-ic" src={icone} alt="" /> : <span className="motd-ic vide">{serveur.nom.charAt(0).toUpperCase()}</span>}
            <div className="motd-t">
              <b>{serveur.nom}</b>
              <span>{motd || "A Minecraft Server"}</span>
            </div>
            <div className="motd-r">
              <span>
                {serveur.joueurs.length}/{p["max-players"] ?? 20}
              </span>
              <i />
              <i />
              <i />
              <i />
              <i />
            </div>
          </div>
        </div>
        <div className="row">
          <span>
            Icône du serveur
            <code className="pk">server-icon.png</code>
          </span>
          <div className="fx-icone">
            {icone ? <img className="motd-ic sm" src={icone} alt="" /> : <span className="motd-ic sm vide">{serveur.nom.charAt(0).toUpperCase()}</span>}
            {iconePerso && (
              <button type="button" className="btn" onClick={() => lireIcone(true)} title="Remettre la même icône que dans BHOST">
                Par défaut
              </button>
            )}
            <ChoisirIcone id={serveur.id} onFini={() => lireIcone()} onErreur={onErreur} onInfo={onInfo} />
          </div>
        </div>
      </div>

      <div className="grp-t">Accès</div>
      <div className="fx-card">
        {compteur("max-players", "Joueurs en même temps", 20, 1, 100, 1, String)}
        {interrupteur("white-list", "Liste blanche", false, undefined, (v) =>
          changerListeBlanche(serveur.id, v).then(() => setP({ ...p, "white-list": String(v) }), (e) => onErreur(phrase(e))),
        )}
        {interrupteur("online-mode", "Vérifier les comptes", true, "Empêche l’usurpation de pseudo.")}
        <div className="row">
          <label htmlFor="fx-port">
            Port
            <code className="pk">server-port</code>
          </label>
          <input
            id="fx-port"
            className="field fx-port tnum"
            type="text"
            inputMode="numeric"
            value={port}
            onChange={(e) => setPort(e.target.value.replace(/[^0-9]/g, "").slice(0, 5))}
            onBlur={() => port !== p["server-port"] && regler({ "server-port": port })}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </div>
      </div>

      <div className="grp-t">Partie</div>
      <div className="fx-card">
        <div className="row">
          <span>
            Mode de jeu
            <code className="pk">gamemode</code>
          </span>
          <Segment etiquette="Mode de jeu" options={MODES} valeur={partie.mode === "hardcore" ? "survie" : partie.mode} onChoisir={(mode) => changerPartie({ mode })} />
        </div>
        <div className="row">
          <span>
            Difficulté
            <code className="pk">difficulty</code>
          </span>
          <Segment
            etiquette="Difficulté"
            options={DIFFICULTES}
            valeur={partie.mode === "hardcore" ? "difficile" : partie.difficulte}
            off={partie.mode === "hardcore"}
            onChoisir={(difficulte) => changerPartie({ difficulte })}
          />
        </div>
        {interrupteur("hardcore", "Hardcore", false, undefined, (v) => changerPartie({ mode: v ? "hardcore" : "survie" }))}
        {interrupteur("pvp", "Combat entre joueurs", true)}
        {interrupteur("allow-flight", "Autoriser le vol", false, "Pour les mods qui font voler.")}
        {interrupteur("enable-command-block", "Blocs de commande", false)}
        {compteur("spawn-protection", "Protection du spawn", 16, 0, 64, 1, (n) => `${n} bloc${n > 1 ? "s" : ""}`)}
      </div>

      <div className="grp-t">Monde</div>
      <div className="fx-card">
        {compteur("view-distance", "Distance d’affichage", 10, 2, 32, 1, (n) => `${n} chunks`)}
        {depuis118(serveur.version) && compteur("simulation-distance", "Distance de simulation", 10, 2, 32, 1, (n) => `${n} chunks`)}
        <div className="row">
          <div className="lbl">
            <span>
              Graine du monde
              <code className="pk">level-seed</code>
            </span>
            <span className="t2 mono">{graine || "Aléatoire"}</span>
          </div>
          {graine && (
            <button
              type="button"
              className="btn"
              onClick={() =>
                navigator.clipboard.writeText(graine).then(
                  () => onInfo("Graine copiée"),
                  () => onErreur("Copie impossible."),
                )
              }
            >
              <IconCopy size={14} stroke={2} aria-hidden />
              Copier
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/* Un nombre avec − et + ; on peut aussi cliquer dessus et le taper. */
export function Compteur({ valeur, min, max, pas, unite, nom, onChoisir }: { valeur: number; min: number; max: number; pas: number; unite: (n: number) => string; nom: string; onChoisir: (n: number) => void }) {
  const [texte, setTexte] = useState<string | null>(null);
  const borner = (n: number) => Math.min(max, Math.max(min, n));
  const valider = () => {
    if (texte === null) return;
    const n = parseInt(texte, 10);
    setTexte(null);
    if (Number.isFinite(n) && borner(n) !== valeur) onChoisir(borner(n));
  };
  return (
    <div className="stepper">
      <button type="button" aria-label="Moins" disabled={valeur <= min} onClick={() => onChoisir(borner(valeur - pas))}>
        −
      </button>
      {texte === null ? (
        <button type="button" className="tnum fx-n" title="Taper un nombre" aria-label={nom} onClick={() => setTexte(String(valeur))}>
          <b key={valeur}>{unite(valeur)}</b>
        </button>
      ) : (
        <input
          className="tnum fx-n"
          autoFocus
          inputMode="numeric"
          aria-label={nom}
          value={texte}
          onFocus={(e) => e.currentTarget.select()}
          onChange={(e) => setTexte(e.target.value.replace(/[^0-9]/g, "").slice(0, 4))}
          onBlur={valider}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") setTexte(null);
          }}
        />
      )}
      <button type="button" aria-label="Plus" disabled={valeur >= max} onClick={() => onChoisir(borner(valeur + pas))}>
        +
      </button>
    </div>
  );
}
