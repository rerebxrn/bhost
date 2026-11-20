import { useEffect, useRef, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { IconAlertTriangle, IconArrowUpRight, IconCheck, IconChevronLeft, IconChevronRight, IconLink, IconPlus, IconRefresh, IconSearch, IconTrash } from "@tabler/icons-react";
import Choix from "../components/Choix";
import Segment from "../components/Segment";
import Fenetre from "../components/Fenetre";
import Confirmer from "../components/Confirmer";
import MondeCasse from "../components/MondeCasse";
import {
  activerExtension,
  chercherExtensions,
  desinstallerExtension,
  ecouterExtensions,
  extensionsServeur,
  installerExtension,
  mettreAJourExtensions,
  redemarrerServeur,
  type Extension,
  type GenreExtensions,
  type Projet,
  type Tri,
  type VueExtensions,
} from "../api";
import { nomExtensions, nomLogiciel, texteStatut, type Serveur } from "../types";
import "../components/Menus.css";
import "./Extensions.css";

type Props = {
  serveur: Serveur;
  erreur: string;
  onErreur: (texte: string) => void;
  onInfo: (texte: string) => void;
};

type Genre = VueExtensions["genre"];

// Le nom d'un plugin, mod ou datapack du catalogue : un clic ouvre sa page web dans le navigateur.
function NomLien({ nom, projet, fleche }: { nom: string; projet: string | null; fleche?: boolean }) {
  if (!projet || !/^[A-Za-z0-9]{8}$/.test(projet)) return <>{nom}</>;
  const adresse = `https://modrinth.com/project/${projet}`;
  return (
    <a
      className={"ext-nom" + (fleche ? " fleche" : "")}
      href={adresse}
      title={`Voir la page de ${nom}`}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        openUrl(adresse).catch(() => {});
      }}
      onKeyDown={(e) => e.stopPropagation()}
    >
      {nom}
      {fleche && <IconArrowUpRight size={16} stroke={2.2} aria-hidden />}
    </a>
  );
}

// Les catégories du catalogue, par genre (celles qui existent vraiment).
const CATEGORIES: Record<Genre, { id: string; nom: string }[]> = {
  mods: [
    { id: "", nom: "Toutes les catégories" },
    { id: "optimization", nom: "Performances" },
    { id: "decoration", nom: "Construction" },
    { id: "technology", nom: "Technique" },
    { id: "adventure", nom: "Aventure" },
    { id: "utility", nom: "Utilitaires" },
    { id: "library", nom: "Bibliothèques" },
  ],
  plugins: [
    { id: "", nom: "Toutes les catégories" },
    { id: "management", nom: "Administration" },
    { id: "utility", nom: "Utilitaires" },
    { id: "economy", nom: "Économie" },
    { id: "minigame", nom: "Mini-jeux" },
    { id: "social", nom: "Social" },
    { id: "worldgen", nom: "Monde" },
  ],
  datapacks: [
    { id: "", nom: "Toutes les catégories" },
    { id: "worldgen", nom: "Génération du monde" },
    { id: "adventure", nom: "Aventure" },
    { id: "game-mechanics", nom: "Gameplay" },
    { id: "utility", nom: "Utilitaires" },
  ],
};

// L'ordre des résultats. « Pertinence » n'a de sens qu'avec un mot cherché.
const TRIS: { id: Tri; nom: string; titre: string }[] = [
  { id: "pertinence", nom: "Pertinence", titre: "Résultats" },
  { id: "telecharges", nom: "Plus téléchargés", titre: "Les plus téléchargés" },
  { id: "moins", nom: "Moins téléchargés", titre: "Les moins téléchargés" },
  { id: "suivis", nom: "Plus suivis", titre: "Les plus suivis" },
  { id: "recents", nom: "Plus récents", titre: "Les plus récents" },
  { id: "maj", nom: "Mis à jour récemment", titre: "Mis à jour récemment" },
];

const UNITES: Record<Genre, [string, string]> = {
  mods: ["mod", "mods"],
  plugins: ["plugin", "plugins"],
  datapacks: ["datapack", "datapacks"],
};

// Combien de résultats arrivent à chaque page (comme le catalogue).
const PAGE = 20;

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n'a pas marché.");

// « 12 M », « 3,4 k ».
function compte(n: number): string {
  const f = (x: number) => x.toLocaleString("fr-FR", { maximumFractionDigits: x < 10 ? 1 : 0 });
  if (n >= 1_000_000) return `${f(n / 1_000_000)} M`;
  if (n >= 1_000) return `${f(n / 1_000)} k`;
  return String(n);
}

// La tuile : la vraie icône du projet ; sinon (hors ligne, pas d'icône) sa couleur et son initiale.
function Tuile({ nom, couleur, icone, taille = 36 }: { nom: string; couleur: number | null; icone?: string | null; taille?: number }) {
  const [casse, setCasse] = useState(false);
  const rayon = Math.round(taille * 0.24);
  if (icone && !casse) {
    return (
      <img
        className="ext-ic img"
        src={icone}
        alt=""
        width={taille}
        height={taille}
        loading="lazy"
        decoding="async"
        draggable={false}
        style={{ width: taille, height: taille, flexBasis: taille, borderRadius: rayon }}
        onError={() => setCasse(true)}
      />
    );
  }
  const lettre = nom.replace(/^[^A-Za-z0-9]+/, "").charAt(0).toUpperCase() || "?";
  return (
    <span
      className={"ext-ic" + (couleur === null ? " neutre" : "")}
      style={{
        width: taille,
        height: taille,
        flexBasis: taille,
        borderRadius: rayon,
        fontSize: Math.round(taille * 0.42),
        ...(couleur === null ? {} : { background: `#${couleur.toString(16).padStart(6, "0")}` }),
      }}
      aria-hidden
    >
      {lettre}
    </span>
  );
}

function Interrupteur({ actif, nom, off, onClick }: { actif: boolean; nom: string; off?: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={actif}
      aria-label={(actif ? "Couper " : "Activer ") + nom}
      className={"sw" + (actif ? " on" : "")}
      disabled={off}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      <i />
    </button>
  );
}

// L'onglet Plugins / Mods / Datapacks (maquette A, choisie par Thomas le 18/11) :
// la liste calme de ce qui est installé (Mise à jour · Actifs · Coupés), une fiche par extension,
// et une page à part « Ajouter des … » pour le catalogue.
export default function Extensions({ serveur, erreur, onErreur, onInfo }: Props) {
  const [vue, setVue] = useState<VueExtensions | null>(null);
  const [illisible, setIllisible] = useState(false); // la liste n'a pas pu être lue
  const [horsLigne, setHorsLigne] = useState(false); // le catalogue n'a pas répondu
  const [essai, setEssai] = useState(0); // « Réessayer » relance la recherche
  const [ajouter, setAjouter] = useState(false);
  const [fiche, setFiche] = useState<string | null>(null); // fichier ouvert
  const [occupe, setOccupe] = useState<string | null>(null); // fichier ou projet en cours (ou « tout »)
  const [texte, setTexte] = useState("");
  const [categorie, setCategorie] = useState("");
  const [genreChoisi, setGenreChoisi] = useState<GenreExtensions | null>(null); // null : le genre principal
  const [triChoisi, setTriChoisi] = useState<Tri | null>(null); // null : selon la recherche
  const [resultats, setResultats] = useState<Projet[] | null>(null);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [suite, setSuite] = useState(false);
  const [cherche, setCherche] = useState(false);
  const [plus, setPlus] = useState(false); // « Afficher plus » en cours
  const demande = useRef(0);
  // Avec un mot cherché : la pertinence d'abord. Sans mot : la pertinence n'a pas de sens.
  const avecTexte = texte.trim() !== "";
  const tri: Tri = triChoisi === null || (triChoisi === "pertinence" && !avecTexte) ? (avecTexte ? "pertinence" : "telecharges") : triChoisi;

  // Un autre serveur : on revient au genre principal (plugins ou mods).
  useEffect(() => {
    setGenreChoisi(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id, serveur.logiciel, serveur.version]);

  useEffect(() => {
    setVue(null);
    setAjouter(false);
    setCategorie("");
    setResultats(null);
    lire();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id, serveur.logiciel, serveur.version, genreChoisi]);

  function lire() {
    setIllisible(false);
    extensionsServeur(serveur.id, genreChoisi).then(setVue, (e) => {
      setIllisible(true);
      onErreur(phrase(e));
    });
  }

  // Le serveur redémarre : la note « Redémarre pour appliquer » disparaît.
  const statut = useRef(serveur.statut);
  useEffect(() => {
    if (statut.current === serveur.statut) return;
    statut.current = serveur.statut;
    if (serveur.statut === "en-ligne" || serveur.statut === "arrete") extensionsServeur(serveur.id, genreChoisi).then(setVue, () => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.statut]);

  // Un plugin n'a pas démarré (vu dans la console) : on relit la liste, un peu après la dernière ligne.
  useEffect(() => {
    let minuterie = 0;
    const arreter = ecouterExtensions((id) => {
      if (id !== serveur.id) return;
      window.clearTimeout(minuterie);
      minuterie = window.setTimeout(() => extensionsServeur(serveur.id, genreChoisi).then(setVue, () => {}), 800);
    });
    return () => {
      window.clearTimeout(minuterie);
      arreter();
    };
  }, [serveur.id, genreChoisi]);

  // Ajouter : on cherche un peu après la dernière lettre tapée (la 1re page).
  useEffect(() => {
    if (!ajouter) return;
    const numero = ++demande.current;
    setCherche(true);
    setPlus(false);
    const minuterie = window.setTimeout(() => {
      chercherExtensions(serveur.id, texte, categorie || null, tri, 0, genreChoisi)
        .then(
          (r) => {
            if (numero !== demande.current) return;
            setHorsLigne(false);
            setResultats(r.projets);
            setTotal(r.total);
            setSuite(r.suite);
            setPage(0);
          },
          (e) => {
            if (numero !== demande.current) return;
            setResultats([]);
            setTotal(0);
            setSuite(false);
            setHorsLigne(true);
            onErreur(phrase(e));
          },
        )
        .finally(() => numero === demande.current && setCherche(false));
    }, 350);
    return () => window.clearTimeout(minuterie);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ajouter, texte, categorie, tri, serveur.id, essai, genreChoisi]);

  // « Afficher plus » : la page suivante, ajoutée au bout de la liste.
  const afficherPlus = () => {
    const numero = demande.current;
    setPlus(true);
    chercherExtensions(serveur.id, texte, categorie || null, tri, page + 1, genreChoisi)
      .then(
        (r) => {
          if (numero !== demande.current) return;
          setResultats((avant) => {
            const deja = new Set((avant ?? []).map((p) => p.id));
            return [...(avant ?? []), ...r.projets.filter((p) => !deja.has(p.id))];
          });
          setTotal(r.total);
          setSuite(r.suite);
          setPage(page + 1);
        },
        (e) => numero === demande.current && onErreur(phrase(e)),
      )
      .finally(() => numero === demande.current && setPlus(false));
  };

  const genre: Genre =
    genreChoisi ?? vue?.genre ?? (serveur.logiciel === "Vanilla" ? "datapacks" : ["Fabric", "Forge", "NeoForge", "Youer", "Arclight", "Mohist"].includes(serveur.logiciel) ? "mods" : "plugins");
  // Plusieurs genres (plugins + datapacks) : le sélecteur en haut, comme dans l'onglet Joueurs.
  const [genres, setGenres] = useState<Genre[]>([]);
  const [nombres, setNombres] = useState<VueExtensions["nombres"]>({});
  useEffect(() => {
    if (!vue) return;
    setGenres(vue.genres);
    setNombres(vue.nombres);
  }, [vue]);
  useEffect(() => {
    setGenres([]);
  }, [serveur.id]);
  const avecSelecteur = genres.length > 1;
  const [unite, unites] = UNITES[genre];
  const titre = nomExtensions(serveur.logiciel);
  const version = nomLogiciel(serveur);
  const liste = vue?.liste ?? [];
  const majs = liste.filter((x) => x.maj);
  const actifs = liste.filter((x) => x.actif && !x.maj);
  const coupes = liste.filter((x) => !x.actif && !x.maj);
  const installes = new Set(liste.map((x) => x.projet).filter(Boolean));
  const ouverte = fiche ? liste.find((x) => x.fichier === fiche) : undefined;

  const agir = (cle: string, action: Promise<VueExtensions>, message?: string) => {
    onErreur("");
    setOccupe(cle);
    action
      .then(
        (v) => {
          setVue(v);
          if (message) onInfo(message);
        },
        (e) => onErreur(phrase(e)),
      )
      .finally(() => setOccupe(null));
  };
  // Un datapack qui a créé le terrain du monde : on demande avant de le couper ou de le retirer.
  const [aConfirmer, setAConfirmer] = useState<{ x: Extension; retirer: boolean } | null>(null);
  const couper = (x: Extension, forcer = false) => agir(x.fichier, activerExtension(serveur.id, x.fichier, !x.actif, genreChoisi, forcer));
  const demander = (x: Extension, retirer: boolean) => {
    setFiche(null);
    setAConfirmer({ x, retirer });
  };
  const basculer = (x: Extension) => (x.actif && x.monde ? demander(x, false) : couper(x));
  // Une mise à jour peut ne rien changer : la version plus récente demande un Java plus récent que celui du serveur.
  const majFaite = (cle: string, fichiers: string[], reussi: (faits: number) => string) => {
    onErreur("");
    setOccupe(cle);
    mettreAJourExtensions(serveur.id, fichiers, genreChoisi)
      .then(
        ({ vue: v, faits }) => {
          setVue(v);
          onInfo(faits > 0 ? reussi(faits) : "Rien de plus récent qui marche ici");
        },
        (e) => onErreur(phrase(e)),
      )
      .finally(() => setOccupe(null));
  };
  const mettreAJour = (x: Extension) => majFaite(x.fichier, [x.fichier], () => `${x.nom} mis à jour`);
  const retirer = (x: Extension, forcer = false) => {
    setFiche(null);
    agir(x.fichier, desinstallerExtension(serveur.id, x.fichier, genreChoisi, forcer), `${x.nom} désinstallé`);
  };
  const desinstaller = (x: Extension) => (x.monde ? demander(x, true) : retirer(x));

  const installer = (p: Projet) => {
    onErreur("");
    setOccupe(p.id);
    installerExtension(serveur.id, p.id, genreChoisi)
      .then(
        ({ vue: v, avec, manque }) => {
          setVue(v);
          onInfo(`${p.nom} installé` + (avec.length ? ` avec ${avec.join(", ")}` : ""));
          if (manque.length) onErreur(`Il lui faut aussi ${manque.join(", ")}, introuvable pour ${nomLogiciel(serveur)}.`);
        },
        (e) => onErreur(phrase(e)),
      )
      .finally(() => setOccupe(null));
  };

  const boutonAjouter = (
    <button type="button" className="ext-pilule" data-gel onClick={() => setAjouter(true)}>
      <IconPlus size={15} stroke={2.4} aria-hidden />
      Ajouter des {unites}
    </button>
  );

  const entete = (
    <header className="head ligne">
      <div>
        {ajouter && (
          <button type="button" className="ext-retour" onClick={() => setAjouter(false)}>
            <IconChevronLeft size={16} stroke={2} aria-hidden />
            {titre}
          </button>
        )}
        <h1>{ajouter ? `Ajouter des ${unites}` : titre}</h1>
        <div className="meta">
          {ajouter ? (
            <span>Seulement ceux qui marchent avec {version}</span>
          ) : (
            <>
              <span className={"dot " + serveur.statut} aria-hidden />
              <b>{texteStatut(serveur)}</b>
              <span>
                · {version}
                {vue && !avecSelecteur && ` · ${liste.length} installé${liste.length > 1 ? "s" : ""}`}
              </span>
            </>
          )}
        </div>
        {erreur && (
          <p className="alerte" role="alert">
            {erreur}
          </p>
        )}
      </div>
      {!ajouter && !avecSelecteur && boutonAjouter}
    </header>
  );

  // Plugins | Datapacks (ou Mods | Datapacks), avec le bouton « Ajouter » à droite.
  const NOMS_GENRES: Record<Genre, string> = { plugins: "Plugins", mods: "Mods", datapacks: "Datapacks" };
  const selecteur = avecSelecteur && (
    <>
      <div className="pl-top ext-top">
        <Segment
          etiquette="Genre"
          options={genres.map((g) => ({ id: g, nom: NOMS_GENRES[g], n: nombres[g] ?? 0 }))}
          valeur={genre}
          onChoisir={(g) => setGenreChoisi(g)}
        />
        {boutonAjouter}
      </div>
      {genre === "datapacks" && vue?.monde && <p className="ext-hors">Dans le monde « {vue.monde} ».</p>}
    </>
  );

  const ligne = (x: Extension) => {
    const ici = occupe === x.fichier || occupe === "tout";
    return (
      <div
        className={"ext-ligne" + (x.actif ? "" : " off")}
        key={x.fichier}
        role="button"
        tabIndex={0}
        onClick={() => setFiche(x.fichier)}
        onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), setFiche(x.fichier))}
      >
        <Tuile nom={x.nom} couleur={x.couleur} icone={x.icone} />
        <div className="ext-tx">
          <b>
            <NomLien nom={x.nom} projet={x.projet} />
            {x.version && !x.maj && <span className="ext-ver">{x.version}</span>}
          </b>
          {x.maj ? (
            <span>
              {x.version ?? "?"} → {x.maj}
            </span>
          ) : (
            x.description && <span>{x.description}</span>
          )}
          {x.pour && (
            <span className="ext-badge">
              <IconLink size={12} stroke={2} aria-hidden />
              Pour {x.pour}
            </span>
          )}
          {(x.conflit ?? x.panne ?? x.alerte) && (
            <span className="ext-badge rouge">
              <IconAlertTriangle size={12} stroke={2} aria-hidden />
              {x.conflit ?? x.panne ?? x.alerte}
            </span>
          )}
        </div>
        {x.maj ? (
          <button
            type="button"
            className="ext-pilule clair petit"
            disabled={ici}
            onClick={(e) => {
              e.stopPropagation();
              mettreAJour(x);
            }}
          >
            {ici ? "Mise à jour…" : "Mettre à jour"}
          </button>
        ) : (
          !x.dossier && <Interrupteur actif={x.actif} nom={x.nom} off={ici} onClick={() => basculer(x)} />
        )}
        <IconChevronRight className="ext-chev" size={16} stroke={2} aria-hidden />
      </div>
    );
  };

  const groupe = (nom: string, elements: Extension[], droite?: ReactNode) =>
    elements.length > 0 && (
      <div className="ext-groupe">
        <div className="ext-gt">
          <span>{nom}</span>
          {droite}
        </div>
        <section className="surf">{elements.map(ligne)}</section>
      </div>
    );

  if (ajouter) {
    const titreTri = TRIS.find((t) => t.id === tri)?.titre ?? "Résultats";
    const tris = TRIS.filter((t) => avecTexte || t.id !== "pertinence");
    return (
      <div className="params extensions ext-pousse" key="ajouter">
        {entete}
        <div className="ext-barre">
          <label className="ext-recherche">
            <IconSearch size={14} stroke={2} aria-hidden />
            <input type="search" value={texte} placeholder="Rechercher" aria-label="Rechercher" spellCheck={false} autoFocus onChange={(e) => setTexte(e.target.value)} />
          </label>
          <Choix etiquette="Catégorie" options={CATEGORIES[genre]} valeur={categorie} onChoisir={setCategorie} />
          <Choix etiquette="Trier" options={tris} valeur={tri} onChoisir={setTriChoisi} />
        </div>
        <div className="ext-groupe">
          <div className="ext-gt">
            <span>{titreTri}</span>
            {resultats !== null && total > 0 && (
              <span className="ext-total">
                {total.toLocaleString("fr-FR")} {total > 1 ? unites : unite}
              </span>
            )}
          </div>
          {resultats === null ? (
            <section className="surf">
              <div className="empty">Recherche…</div>
            </section>
          ) : resultats.length === 0 ? (
            <section className="surf">
              {horsLigne ? (
                <div className="empty ext-reessai">
                  <span>Catalogue injoignable.</span>
                  <button type="button" className="ext-pilule clair petit" onClick={() => setEssai((n) => n + 1)}>
                    Réessayer
                  </button>
                </div>
              ) : (
                <div className="empty">{avecTexte ? "Rien trouvé. Essaie un autre mot." : "Rien pour l’instant dans cette catégorie."}</div>
              )}
            </section>
          ) : (
            <div className={"ext-cartes" + (cherche ? " attente" : "")}>
              {resultats.map((p, i) => (
                <div className="ext-carte" key={p.id} style={{ animationDelay: `${Math.min(i % PAGE, 10) * 25}ms` }}>
                  <Tuile nom={p.nom} couleur={p.couleur} icone={p.icone} taille={48} />
                  <div className="ext-tx">
                    <b>
                      <NomLien nom={p.nom} projet={p.id} />
                    </b>
                    <p>{p.description}</p>
                    <small>
                      {compte(p.telechargements)} téléchargement{p.telechargements > 1 ? "s" : ""}
                      {p.auteur && <> · {p.auteur}</>}
                    </small>
                  </div>
                  {installes.has(p.id) ? (
                    <span className="ext-installe">
                      <IconCheck size={14} stroke={2.4} aria-hidden />
                      Installé
                    </span>
                  ) : (
                    <button type="button" className="ext-pilule clair petit" disabled={occupe !== null} onClick={() => installer(p)}>
                      {occupe === p.id ? "Installation…" : "Installer"}
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
          {resultats !== null && resultats.length > 0 && suite && (
            <div className="ext-plus">
              <button type="button" className="ext-pilule clair" disabled={plus || cherche} onClick={afficherPlus}>
                {plus ? "Chargement…" : "Afficher plus"}
              </button>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="params extensions" key="liste">
      {entete}
      {selecteur}

      <MondeCasse id={serveur.id} manquants={vue?.manquants ?? []} onInfo={onInfo} onErreur={onErreur} />

      {vue?.redemarrer && (
        <div className="ext-note">
          <IconRefresh size={16} stroke={2} aria-hidden />
          <span>Redémarre le serveur pour appliquer.</span>
          <button type="button" className="ext-pilule petit" onClick={() => redemarrerServeur(serveur.id).catch((e) => onErreur(phrase(e)))}>
            Redémarrer
          </button>
        </div>
      )}
      {vue?.horsLigne && <p className="ext-hors">Catalogue injoignable : pas de mises à jour pour l’instant.</p>}

      {vue === null ? (
        <section className="surf">
          {illisible ? (
            <div className="empty ext-reessai">
              <span>Impossible de lire les {unites}.</span>
              <button type="button" className="ext-pilule clair petit" onClick={lire}>
                Réessayer
              </button>
            </div>
          ) : (
            <div className="empty">Lecture…</div>
          )}
        </section>
      ) : liste.length === 0 ? (
        <section className="surf ext-vide">
          <b>Aucun {unite} pour l’instant.</b>
          <span>Ajoute-en depuis le catalogue : seulement ceux qui marchent avec {version}.</span>
          <button type="button" className="ext-pilule clair" onClick={() => setAjouter(true)}>
            <IconPlus size={15} stroke={2.4} aria-hidden />
            Ajouter des {unites}
          </button>
        </section>
      ) : (
        <>
          {groupe(
            majs.length > 1 ? "Mises à jour" : "Mise à jour",
            majs,
            majs.length > 1 && (
              <button type="button" className="ext-lien" disabled={occupe !== null} onClick={() => majFaite("tout", [], (n) => `${n} mis à jour`)}>
                {occupe === "tout" ? "Mise à jour…" : "Tout mettre à jour"}
              </button>
            ),
          )}
          {groupe(`Actifs · ${actifs.length}`, actifs)}
          {groupe(`Coupés · ${coupes.length}`, coupes)}
        </>
      )}

      {ouverte && (
        <Fenetre titre={ouverte.nom} classe="ext-fiche" onFermer={() => setFiche(null)}>
          <div className="ext-fh">
            <Tuile nom={ouverte.nom} couleur={ouverte.couleur} icone={ouverte.icone} taille={56} />
            <div>
              <b>
                <NomLien nom={ouverte.nom} projet={ouverte.projet} fleche />
              </b>
              <span>
                {ouverte.version ? `Version ${ouverte.version} · ` : ""}
                {ouverte.actif ? "actif" : "coupé"}
              </span>
            </div>
            {!ouverte.dossier && <Interrupteur actif={ouverte.actif} nom={ouverte.nom} off={occupe !== null} onClick={() => basculer(ouverte)} />}
          </div>
          {ouverte.description && <p className="ext-desc">{ouverte.description}</p>}
          <div className="ext-kv">
            {ouverte.conflit && (
              <div>
                <span>Attention</span>
                <span className="rouge">{ouverte.conflit}</span>
              </div>
            )}
            {ouverte.panne && (
              <div>
                <span>{ouverte.panneEnJeu ? "En jeu" : "Démarrage"}</span>
                <span className="rouge">{ouverte.panne}</span>
              </div>
            )}
            {ouverte.monde && (
              <div>
                <span>Monde</span>
                <span>A créé son terrain : à garder</span>
              </div>
            )}
            {ouverte.maj && (
              <div>
                <span>Mise à jour</span>
                <span>{ouverte.maj} disponible</span>
              </div>
            )}
            <div>
              <span>Fonctionne avec</span>
              <span className={ouverte.alerte ? "rouge" : ""}>
                {ouverte.alerte ?? (!ouverte.projet ? "Inconnu du catalogue" : ouverte.panne ? `Prévu pour ${version}` : `${version} ✓`)}
              </span>
            </div>
            {ouverte.pour && (
              <div>
                <span>Installé pour</span>
                <span>{ouverte.pour}</span>
              </div>
            )}
            <div>
              <span>Fichier</span>
              <span className="mono">{ouverte.fichier}</span>
            </div>
          </div>
          <div className="ext-fb">
            <button type="button" className="ext-danger" disabled={occupe !== null} onClick={() => desinstaller(ouverte)}>
              <IconTrash size={15} stroke={2} aria-hidden />
              Désinstaller
            </button>
            <span />
            <button type="button" className="btn" onClick={() => setFiche(null)}>
              Fermer
            </button>
            {ouverte.maj && (
              <button type="button" className="btn acc" disabled={occupe !== null} onClick={() => mettreAJour(ouverte)}>
                {occupe === ouverte.fichier ? "Mise à jour…" : "Mettre à jour"}
              </button>
            )}
          </div>
        </Fenetre>
      )}

      {aConfirmer && (
        <Confirmer
          titre={`${aConfirmer.retirer ? "Désinstaller" : "Couper"} « ${aConfirmer.x.nom} » ?`}
          texte="Ce datapack a créé le terrain de ce monde. Sans lui, le monde ne se chargera plus."
          bouton={aConfirmer.retirer ? "Désinstaller" : "Couper"}
          onOui={() => (aConfirmer.retirer ? retirer(aConfirmer.x, true) : couper(aConfirmer.x, true))}
          onFermer={() => setAConfirmer(null)}
        />
      )}
    </div>
  );
}
