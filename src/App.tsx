import { useCallback, useEffect, useRef, useState } from "react";
import { IconCheck } from "@tabler/icons-react";
import TitleBar from "./components/TitleBar";
import Sidebar from "./components/Sidebar";
import Tabs from "./components/Tabs";
import ActionBar from "./components/ActionBar";
import CommandBar from "./components/CommandBar";
import MenuServeur from "./components/MenuServeur";
import Filet from "./components/Filet";
import Renommer from "./components/Renommer";
import Apparence from "./components/Apparence";
import Supprimer from "./components/Supprimer";
import DemandePop from "./components/DemandePop";
import Reglages from "./pages/Reglages";
import NouveauServeur from "./pages/NouveauServeur";
import PremierLancement from "./pages/PremierLancement";
import MachineDediee from "./pages/MachineDediee";
import Console from "./pages/Console";
import Accueil from "./pages/Accueil";
import Parametres from "./pages/Parametres";
import Joueurs from "./pages/Joueurs";
import Fichiers from "./pages/Fichiers";
import Sauvegardes from "./pages/Sauvegardes";
import Automatisations from "./pages/Automatisations";
import Extensions from "./pages/Extensions";
import { useLiquidGlass } from "./glass/useLiquidGlass";
import { useTheme } from "./theme/ThemeProvider";
import {
  couleurServeur,
  dupliquerServeur,
  ecouterAcces,
  ecouterCopie,
  ecouterDemandes,
  repondreDemande,
  etatsAcces,
  supprimerServeur,
  retirerServeur,
  arreterServeur,
  demarrerServeur,
  ecouterJava,
  ecouterProblemes,
  ecouterInternet,
  problemesServeurs,
  etatInternet,
  type Probleme,
  ecouterJoueurs,
  ecouterMesures,
  ecouterStatuts,
  ecouterTps,
  envoyerCommande,
  forcerArret,
  listerServeurs,
  ouvrirDossier,
  redemarrerServeur,
  renommerServeur,
} from "./api";
import { listeOnglets, nomLogiciel, texteStatut, type Acces, type Mesures, type Onglet, type Serveur } from "./types";

import "./App.css";

const POINTS = 40; // nombre de mesures gardées pour les mini-courbes (environ 80 secondes)
const MESURES_VIDES: Mesures = { processeur: [], memoireMo: [], joueurs: [], tps: [] };
const garder = (serie: number[], valeur: number) => [...serie, valeur].slice(-POINTS);
// Onglets en une seule colonne : la page est plus étroite et reste au centre.
const ETROITES: string[] = ["joueurs", "extensions", "sauvegardes", "parametres"];

// Le premier lancement (choix « ce PC » ou « machine dédiée ») n'est montré qu'une fois.
const CLE_ACCUEIL = "bhost.accueil";
// Réglages › « Revoir » met « revoir » : l'écran revient, même avec des serveurs.
const revoirAccueil = () => {
  try {
    return localStorage.getItem(CLE_ACCUEIL) === "revoir";
  } catch {
    return false;
  }
};
const accueilDejaVu = () => {
  try {
    return localStorage.getItem(CLE_ACCUEIL) !== null && !revoirAccueil();
  } catch {
    return true;
  }
};
const retenirAccueil = () => {
  try {
    localStorage.setItem(CLE_ACCUEIL, "local");
  } catch {
    // Pas de stockage : l'écran reviendra au prochain lancement, ce n'est pas grave.
  }
};

// Ce qu'on affiche à droite de la barre latérale.
type Vue = "serveur" | "nouveau" | "reglages";

export default function App() {
  // null : la liste n'est pas encore arrivée de la partie Rust.
  const [serveurs, setServeurs] = useState<Serveur[] | null>(null);
  const [idActif, setIdActif] = useState<string | null>(null);
  const [vue, setVue] = useState<Vue>("serveur");
  const [onglet, setOnglet] = useState<Onglet>("accueil");
  const [erreur, setErreur] = useState("");
  const [defile, setDefile] = useState(false); // la page a défilé : effet de bord sous les onglets
  const [accueilVu, setAccueilVu] = useState(accueilDejaVu);
  const [tutoMachine, setTutoMachine] = useState(false); // premier lancement : tutoriel « machine dédiée » ouvert
  const theme = useTheme();
  useLiquidGlass(theme.reel, theme.teinte, theme.sombre);

  // Au lancement : on demande à Rust la liste des serveurs, avec leur état.
  useEffect(() => {
    listerServeurs()
      .then((liste) => {
        setServeurs(liste);
        setIdActif(liste[0]?.id ?? null);
        if (liste.length === 0) setVue("nouveau");
        // Déjà des serveurs (mise à jour depuis une ancienne version) : pas d'écran d'accueil.
        else if (!revoirAccueil()) {
          retenirAccueil();
          setAccueilVu(true);
        }
      })
      .catch(() => {
        setServeurs([]);
        setVue("nouveau");
      });
  }, []);

  // Rust nous prévient à chaque changement d'état d'un serveur.
  useEffect(
    () =>
      ecouterStatuts((id, statut, depuis) => {
        setServeurs((liste) => liste && liste.map((s) => (s.id === id ? { ...s, statut, depuis } : s)));
        // Plus en ligne : les ticks par seconde ne veulent plus rien dire.
        if (statut !== "en-ligne") setMesures((tout) => (tout[id]?.tps.length ? { ...tout, [id]: { ...tout[id], tps: [] } } : tout));
      }),
    [],
  );

  // Rust nous donne aussi la liste des joueurs connectés, dès qu'elle change.
  useEffect(
    () =>
      ecouterJoueurs((id, joueurs) =>
        setServeurs((liste) => liste && liste.map((s) => (s.id === id ? { ...s, joueurs } : s))),
      ),
    [],
  );

  // Ouvert à tes amis : l'adresse sur Internet de chaque serveur ouvert (ou pourquoi ça ne marche pas).
  const [acces, setAcces] = useState<Record<string, Acces>>({});
  useEffect(() => {
    const ranger = (liste: Acces[]) => setAcces(Object.fromEntries(liste.map((a) => [a.id, a])));
    etatsAcces().then(ranger, () => {});
    return ecouterAcces(ranger);
  }, []);

  // Les cartes d'erreur de l'Accueil (Java, port, plantage, mémoire), et la connexion Internet.
  const [problemes, setProblemes] = useState<Record<string, Probleme>>({});
  const [internet, setInternet] = useState(true);
  useEffect(() => {
    problemesServeurs().then(setProblemes, () => {});
    etatInternet().then(setInternet, () => {});
    const a = ecouterProblemes((id, p) =>
      setProblemes((tout) => {
        const suite = { ...tout };
        if (p) suite[id] = p;
        else delete suite[id];
        return suite;
      }),
    );
    const b = ecouterInternet(setInternet);
    return () => {
      a();
      b();
    };
  }, []);

  // Le téléchargement de Java, avant le démarrage d'un serveur.
  useEffect(
    () =>
      ecouterJava((id, version, pourcent) =>
        setServeurs(
          (liste) =>
            liste &&
            liste.map((s) => (s.id === id ? { ...s, java: pourcent === null ? undefined : { version, pourcent } } : s)),
        ),
      ),
    [],
  );

  // Les mesures de chaque serveur allumé. On garde les dernières pour tracer les mini-courbes.
  const [mesures, setMesures] = useState<Record<string, Mesures>>({});
  const joueursDe = useRef<Record<string, number>>({});
  joueursDe.current = Object.fromEntries((serveurs ?? []).map((s) => [s.id, s.joueurs.length]));
  useEffect(
    () =>
      ecouterMesures((id, processeur, memoireMo) =>
        setMesures((tout) => {
          const avant = tout[id] ?? MESURES_VIDES;
          return {
            ...tout,
            [id]: {
              ...avant,
              processeur: garder(avant.processeur, processeur),
              memoireMo: garder(avant.memoireMo, memoireMo),
              joueurs: garder(avant.joueurs, joueursDe.current[id] ?? 0),
            },
          };
        }),
      ),
    [],
  );

  // Les ticks par seconde du serveur affiché à l'accueil (un relevé toutes les dix secondes).
  useEffect(
    () =>
      ecouterTps((id, tps) =>
        setMesures((tout) => {
          const avant = tout[id] ?? MESURES_VIDES;
          return { ...tout, [id]: { ...avant, tps: garder(avant.tps, tps) } };
        }),
      ),
    [],
  );

  // Quelqu'un vient d'essayer d'entrer sans être sur la liste blanche : on le montre tout de suite.
  const [demande, setDemande] = useState<{ id: string; nom: string } | null>(null);
  const [demandeVue, setDemandeVue] = useState(false);
  const demandeActuelle = useRef(demande);
  demandeActuelle.current = demande;
  useEffect(
    () =>
      ecouterDemandes((id, demandes, nouvelle) => {
        const d = demandeActuelle.current;
        if (nouvelle) {
          setDemande({ id, nom: nouvelle.nom });
          setDemandeVue(true);
        } else if (d && d.id === id && !demandes.some((x) => x.nom === d.nom)) {
          // Réponse donnée ailleurs (onglet Demandes, Inviter) : le message part.
          setDemandeVue(false);
        }
      }),
    [],
  );
  const repondre = (accepter: boolean) => {
    if (!demande) return Promise.resolve();
    const { id, nom } = demande;
    return repondreDemande(id, nom, accepter).then(
      () => {
        setDemandeVue(false);
        prevenir(accepter ? `${nom} peut entrer` : `Demande de ${nom} refusée`);
      },
      (e) => setErreur(typeof e === "string" ? e : "Ça n'a pas marché."),
    );
  };

  const ajouter = (s: Serveur) => {
    setServeurs((liste) => [...(liste ?? []), s]);
    setIdActif(s.id);
    setOnglet("accueil");
    setVue("serveur");
  };

  // Petit message qui apparaît deux secondes en haut du contenu (« localhost » copié…).
  const [message, setMessage] = useState({ texte: "", visible: false });
  const minuterie = useRef(0);
  // duree = 0 : le message reste jusqu'au suivant (copie en cours…).
  const prevenir = (texte: string, duree = 2000) => {
    window.clearTimeout(minuterie.current);
    setMessage({ texte, visible: true });
    if (duree > 0) minuterie.current = window.setTimeout(() => setMessage((m) => ({ ...m, visible: false })), duree);
  };
  const cacherMessage = () => {
    window.clearTimeout(minuterie.current);
    setMessage((m) => ({ ...m, visible: false }));
  };

  // Clic droit › Dupliquer : la copie peut être longue (gros monde), on montre l'avancement.
  const dupliquer = (source: Serveur) => {
    ouvrir(source.id);
    prevenir(`Copie de « ${source.nom} »…`, 0);
    const arreter = ecouterCopie((id, pourcent) => id === source.id && prevenir(`Copie de « ${source.nom} »… ${pourcent} %`, 0));
    dupliquerServeur(source.id)
      .then((copie) => {
        ajouter(copie);
        prevenir(`« ${source.nom} » dupliqué`);
      })
      .catch((e) => {
        cacherMessage();
        setErreur(typeof e === "string" ? e : "La copie n'a pas marché.");
      })
      .finally(arreter);
  };

  // Lance une action et affiche la phrase d'erreur de Rust si elle échoue.
  const agir = (action: Promise<void>) => {
    setErreur("");
    action.catch((e) => setErreur(typeof e === "string" ? e : "Ça n'a pas marché. Réessaie."));
  };

  // Le menu du clic droit sur un serveur, et la fenêtre « Renommer » qu'il peut ouvrir.
  const [menu, setMenu] = useState<{ id: string; x: number; y: number } | null>(null);
  const [aRenommer, setARenommer] = useState<string | null>(null);
  const [aDecorer, setADecorer] = useState<string | null>(null);
  const [aSupprimer, setASupprimer] = useState<{ id: string; garder: boolean } | null>(null);
  const fermerMenu = useCallback(() => setMenu(null), []);
  const ouvrir = (id: string) => {
    setIdActif(id);
    setErreur("");
    setVue("serveur");
  };
  const decorer = (id: string, couleur: string) =>
    couleurServeur(id, couleur).then((s) =>
      setServeurs((liste) => liste && liste.map((x) => (x.id === id ? { ...x, couleur: s.couleur } : x))),
    );
  // Après la suppression (ou le retrait) : on montre le serveur d'à côté, ou l'assistant s'il n'en reste plus.
  const supprimer = (id: string, seulementRetirer: boolean) =>
    (seulementRetirer ? retirerServeur(id) : supprimerServeur(id)).then(() => {
      const reste = liste.filter((s) => s.id !== id);
      setServeurs(reste);
      if (idActif === id || !reste.some((s) => s.id === idActif)) {
        const i = liste.findIndex((s) => s.id === id);
        setIdActif(reste[Math.min(i, reste.length - 1)]?.id ?? null);
      }
      if (reste.length === 0) setVue("nouveau");
    });
  const renommer = (id: string, nom: string) =>
    renommerServeur(id, nom).then((s) => setServeurs((liste) => liste && liste.map((x) => (x.id === id ? { ...x, nom: s.nom } : x))));
  // Un serveur renvoyé par Rust après un réglage : on garde l'état du moment (statut, joueurs…) de l'interface.
  const mettreAJour = (s: Serveur) =>
    setServeurs(
      (liste) =>
        liste &&
        liste.map((x) =>
          x.id === s.id
            ? { ...x, nom: s.nom, ramMo: s.ramMo, joueursMax: s.joueursMax, port: s.port, couleur: s.couleur, javaChoix: s.javaChoix, demarrerAuto: s.demarrerAuto, ouvertAmis: s.ouvertAmis, optimise: s.optimise, arguments: s.arguments }
            : x,
        ),
    );

  const liste = serveurs ?? [];
  // Nouvel onglet ou autre serveur : on remonte en haut de la page.
  const zoneScroll = useRef<HTMLDivElement>(null);
  const cleOnglet = (idActif ?? "") + onglet + vue;
  useEffect(() => {
    zoneScroll.current?.scrollTo({ top: 0 });
    setDefile(false);
  }, [cleOnglet]);
  const serveurMenu = menu && liste.find((s) => s.id === menu.id);
  const serveurARenommer = aRenommer && liste.find((s) => s.id === aRenommer);
  const serveurADecorer = aDecorer && liste.find((s) => s.id === aDecorer);
  const serveurASupprimer = aSupprimer && liste.find((s) => s.id === aSupprimer.id);
  const serveur = liste.find((s) => s.id === idActif) ?? liste[0];
  const surServeur = vue === "serveur" && serveur !== undefined;
  const onglets = serveur ? listeOnglets(serveur.logiciel) : [];
  const nomOnglet = onglets.find((o) => o.id === onglet)?.nom ?? "";
  const avecConsole = onglet === "accueil" || onglet === "console"; // la console remplit la page
  const avecExtensions = onglet === "extensions" && !!serveur && ["Paper", "Purpur", "Spigot", "Fabric", "Forge", "NeoForge", "Vanilla", "Youer", "Arclight", "Mohist"].includes(serveur.logiciel);
  const sousTitre = vue === "reglages" ? "Réglages" : surServeur ? serveur.nom : "Nouveau serveur";
  // Premier lancement : rien à gauche, seulement le choix (on attend la liste pour ne rien faire clignoter).
  const accueil = !accueilVu && (serveurs === null || serveurs.length === 0 || revoirAccueil());

  if (accueil) {
    return (
      <div className={"fenetre" + (tutoMachine ? " sans-cote" : "")}>
        <TitleBar sousTitre={tutoMachine ? "Machine dédiée" : undefined} />
        {serveurs !== null &&
          (tutoMachine ? (
            <MachineDediee onQuitter={() => setTutoMachine(false)} />
          ) : (
            <PremierLancement
              onLocal={() => {
                retenirAccueil();
                setAccueilVu(true);
              }}
              onMachine={() => setTutoMachine(true)}
            />
          ))}
      </div>
    );
  }

  return (
    <div className="fenetre">
      <TitleBar sousTitre={serveurs === null ? undefined : sousTitre} />
      <div className="corps">
        <Sidebar
          serveurs={liste}
          idActif={surServeur ? serveur.id : null}
          nouveauOuvert={serveurs !== null && vue !== "reglages" && !surServeur}
          reglagesOuverts={vue === "reglages"}
          onChoisir={ouvrir}
          onMenu={(id, x, y) => setMenu({ id, x, y })}
          onCreer={() => setVue("nouveau")}
          onReglages={() => setVue("reglages")}
        />

        <main className="main">
          {serveurs === null ? null : vue === "reglages" ? (
            <div className="scroll sans-barres">
              <Reglages />
            </div>
          ) : !surServeur ? (
            <NouveauServeur onCree={ajouter} onAnnuler={liste.length > 0 ? () => setVue("serveur") : undefined} />
          ) : (
            <>
              <Tabs
                onglets={onglets}
                actif={onglet}
                onChoisir={(o) => {
                  setErreur("");
                  setOnglet(o);
                }}
              />

              <div
                className={"g toast" + (message.visible ? " show" : "") + (avecConsole ? " on-console" : "")}
                data-lens="btn" role="status" aria-live="polite">
                <IconCheck size={15} stroke={2.4} aria-hidden />
                <span>{message.texte}</span>
              </div>

              <div className={"edgebar" + (defile && !avecConsole ? " on" : "")} aria-hidden />
              <div ref={zoneScroll} className={"scroll" + (avecConsole ? " pleine" : "")} onScroll={(e) => setDefile(e.currentTarget.scrollTop > 4)}>
                {/* key : la page réapparaît en douceur quand on change d'onglet ou de serveur */}
                <div className={"page" + (ETROITES.includes(onglet) ? " etroite" : "")} key={serveur.id + onglet}>
                  <Filet>
                  {onglet === "accueil" ? (
                    <Accueil serveur={serveur} acces={acces[serveur.id]} mesures={mesures[serveur.id]} erreur={erreur} probleme={problemes[serveur.id]} internet={internet} onChange={mettreAJour} onOnglet={setOnglet} onCopie={prevenir} onErreur={setErreur} />
                  ) : avecExtensions ? (
                    // L'onglet Plugins / Mods / Datapacks a son propre en-tête (bouton « Ajouter », page « Ajouter »).
                    <Extensions serveur={serveur} erreur={erreur} onErreur={setErreur} onInfo={prevenir} />
                  ) : (
                    <>
                      <header className="head">
                        <h1>{nomOnglet}</h1>
                        <div className="meta">
                          <span className={"dot " + serveur.statut} aria-hidden />
                          <b>{texteStatut(serveur)}</b>
                          <span>· {nomLogiciel(serveur)}</span>
                        </div>
                        {erreur && (
                          <p className="alerte" role="alert">
                            {erreur}
                          </p>
                        )}
                      </header>
                      {onglet === "console" ? (
                        <Console id={serveur.id} />
                      ) : onglet === "joueurs" ? (
                        <Joueurs serveur={serveur} acces={acces[serveur.id]} onErreur={setErreur} onInfo={prevenir} />
                      ) : onglet === "fichiers" ? (
                        <Fichiers serveur={serveur} onChange={mettreAJour} onOnglet={setOnglet} onErreur={setErreur} onInfo={prevenir} />
                      ) : onglet === "sauvegardes" ? (
                        <Sauvegardes serveur={serveur} onChange={mettreAJour} onErreur={setErreur} onInfo={prevenir} />
                      ) : onglet === "automatisations" ? (
                        <Automatisations serveur={serveur} onErreur={setErreur} onInfo={prevenir} />
                      ) : onglet === "parametres" ? (
                        <Parametres
                          serveur={serveur}
                          acces={acces[serveur.id]}
                          onChange={mettreAJour}
                          onRenommer={(nom) => renommer(serveur.id, nom)}
                          onRetirer={() => setASupprimer({ id: serveur.id, garder: true })}
                          onSupprimer={() => setASupprimer({ id: serveur.id, garder: false })}
                          onErreur={setErreur}
                          onInfo={prevenir}
                        />
                      ) : (
                        <div className="soon">
                          <b>{nomOnglet}</b>
                          Bientôt.
                        </div>
                      )}
                    </>
                  )}
                  </Filet>
                </div>
              </div>

              <ActionBar
                statut={serveur.statut}
                surConsole={avecConsole}
                avecPanneau={onglet === "accueil"}
                commande={
                  avecConsole ? (
                    <CommandBar
                      actif={serveur.statut === "en-ligne" || serveur.statut === "demarrage"}
                      onEnvoyer={(commande) => agir(envoyerCommande(serveur.id, commande))}
                    />
                  ) : undefined
                }
                onDemarrer={() => agir(demarrerServeur(serveur.id))}
                onArreter={() => agir(arreterServeur(serveur.id))}
                onRedemarrer={() => agir(redemarrerServeur(serveur.id))}
                onForcer={() => agir(forcerArret(serveur.id))}
              />
            </>
          )}
          {demande && (
            <DemandePop
              nom={demande.nom}
              serveur={liste.find((s) => s.id === demande.id)?.nom ?? ""}
              ouvert={demandeVue}
              onAccepter={() => repondre(true)}
              onRefuser={() => repondre(false)}
              onFermer={() => setDemandeVue(false)}
            />
          )}
        </main>
      </div>

      {serveurMenu && menu && (
        <MenuServeur
          x={menu.x}
          y={menu.y}
          nom={serveurMenu.nom}
          onFermer={fermerMenu}
          onOuvrir={() => ouvrir(serveurMenu.id)}
          onRenommer={() => setARenommer(serveurMenu.id)}
          onApparence={() => setADecorer(serveurMenu.id)}
          onDupliquer={() => dupliquer(serveurMenu)}
          onRetirer={() => setASupprimer({ id: serveurMenu.id, garder: true })}
          onSupprimer={() => setASupprimer({ id: serveurMenu.id, garder: false })}
          allume={serveurMenu.statut !== "arrete" && serveurMenu.statut !== "plante"}
          onDossier={() => {
            ouvrir(serveurMenu.id);
            agir(ouvrirDossier(serveurMenu.id));
          }}
        />
      )}
      {serveurADecorer && (
        <Apparence
          serveur={serveurADecorer}
          onEnregistrer={(couleur) => decorer(serveurADecorer.id, couleur)}
          onFermer={() => setADecorer(null)}
        />
      )}
      {serveurASupprimer && aSupprimer && (
        <Supprimer
          nom={serveurASupprimer.nom}
          garder={aSupprimer.garder}
          onSupprimer={() => supprimer(serveurASupprimer.id, aSupprimer.garder)}
          onFermer={() => setASupprimer(null)}
        />
      )}
      {serveurARenommer && (
        <Renommer
          nom={serveurARenommer.nom}
          onEnregistrer={(nom) => renommer(serveurARenommer.id, nom)}
          onFermer={() => setARenommer(null)}
        />
      )}
    </div>
  );
}
