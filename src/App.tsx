import { useCallback, useEffect, useRef, useState } from "react";
import { IconAlertCircle, IconCheck } from "@tabler/icons-react";
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
import AjouterMachine from "./components/CodeMachine";
import Confirmer from "./components/Confirmer";
import ManqueMemoire from "./components/ManqueMemoire";
import Reglages from "./pages/Reglages";
import NouveauServeur from "./pages/NouveauServeur";
import PremierLancement from "./pages/PremierLancement";
import MachineDediee from "./pages/MachineDediee";
import PageMachine from "./pages/Machine";
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
  machinesLiees,
  ecouterMachines,
  delierMachine,
  renommerMachine,
  estManqueMemoire,
  type ManqueMemoire as Manque,
  ecouterLag,
} from "./api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { listeOnglets, nomLogiciel, texteStatut, type Acces, type Machine, type Mesures, type Onglet, type Serveur } from "./types";

import "./App.css";

const POINTS = 40; // nombre de mesures gardées pour les mini-courbes (environ 80 secondes)
const MESURES_VIDES: Mesures = { processeur: [], memoireMo: [], joueurs: [], tps: [] };
const garder = (serie: number[], valeur: number) => [...serie, valeur].slice(-POINTS);
// Onglets en une seule colonne : la page est plus étroite et reste au centre.
const ETROITES: string[] = ["joueurs", "extensions", "sauvegardes", "automatisations", "parametres"];

// Le premier lancement (choix « ce PC » ou « machine dédiée ») n’est montré qu’une fois.
const CLE_ACCUEIL = "bhost.accueil";
// Réglages › « Revoir » met « revoir » : l’écran revient, même avec des serveurs.
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
    // Pas de stockage : l’écran reviendra au prochain lancement, ce n’est pas grave.
  }
};

// Ce qu’on affiche à droite de la barre latérale.
// « machine » : le tutoriel « Préparer une machine dédiée », ouvert depuis Réglages.
// « infos » : la page « Machine » d’une machine dédiée (stockage, température…).
type Vue = "serveur" | "nouveau" | "reglages" | "machine" | "infos";

export default function App() {
  // null : la liste n’est pas encore arrivée de la partie Rust.
  const [serveurs, setServeurs] = useState<Serveur[] | null>(null);
  const [idActif, setIdActif] = useState<string | null>(null);
  const [vue, setVue] = useState<Vue>("serveur");
  const [onglet, setOnglet] = useState<Onglet>("accueil");
  const [erreur, setErreur] = useState("");
  const [defile, setDefile] = useState(false); // la page a défilé : effet de bord sous les onglets
  const [accueilVu, setAccueilVu] = useState(accueilDejaVu);
  const [tutoMachine, setTutoMachine] = useState(false); // premier lancement : tutoriel « machine dédiée » ouvert
  const [machines, setMachines] = useState<Machine[]>([]); // machines dédiées reliées
  const [ajoutMachine, setAjoutMachine] = useState(false); // fenêtre « Ajouter une machine dédiée »
  const [aDelier, setADelier] = useState<Machine | null>(null);
  const [machineARenommer, setMachineARenommer] = useState<Machine | null>(null);
  const [machineActive, setMachineActive] = useState("ici"); // la machine affichée à gauche : « ici » = ce PC
  const theme = useTheme();
  useLiquidGlass(theme.reel, theme.teinte, theme.sombre);

  // Au lancement : on demande à Rust la liste des serveurs, avec leur état.
  useEffect(() => {
    listerServeurs()
      .then((liste) => {
        setServeurs(liste);
        setIdActif(liste[0]?.id ?? null);
        if (liste.length === 0) setVue("nouveau");
        // Déjà des serveurs (mise à jour depuis une ancienne version) : pas d’écran d’accueil.
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

  // Les machines dédiées reliées, et leurs changements (allumée, éteinte…).
  useEffect(() => {
    machinesLiees().then(setMachines, () => {});
    return ecouterMachines(setMachines);
  }, []);
  // Les mesures des machines (stockage, température…) : relues toutes les 5 s, tant qu’il y a une machine.
  const avecMachines = machines.length > 0;
  useEffect(() => {
    if (!avecMachines) return;
    const minuterie = window.setInterval(() => machinesLiees().then(setMachines, () => {}), 5000);
    return () => window.clearInterval(minuterie);
  }, [avecMachines]);
  // Une machine vient de s’allumer (ou de s’éteindre) : on relit la liste, ses serveurs arrivent (ou partent).
  const enLigne = machines
    .filter((m) => m.etat === "en-ligne")
    .map((m) => m.id)
    .sort()
    .join(",");
  const premierPassage = useRef(true);
  useEffect(() => {
    if (premierPassage.current) {
      premierPassage.current = false;
      return;
    }
    listerServeurs().then(setServeurs, () => {});
    // Une machine revenue ne renvoie son « Ouvert à tes amis » et ses problèmes que s’ils changent : on les redemande.
    etatsAcces().then((l) => setAcces(Object.fromEntries(l.map((a) => [a.id, a]))), () => {});
    problemesServeurs().then(setProblemes, () => {});
  }, [enLigne]);
  // La machine affichée est hors ligne : on montre sa page (pas l’assistant « Créer un serveur », qui échouerait).
  useEffect(() => {
    const m = machines.find((x) => x.id === machineActive);
    if (m && m.etat !== "en-ligne" && (vue === "serveur" || vue === "nouveau")) setVue("infos");
  }, [machines, machineActive, vue]);
  // Une machine vient d’être reliée : ses serveurs arrivent dans la liste.
  const machineAjoutee = (m: Machine) => {
    setMachines((l) => [...l.filter((x) => x.id !== m.id), m]);
    setMachineActive(m.id);
    listerServeurs().then(setServeurs, () => {});
    retenirAccueil();
    setAccueilVu(true);
    setTutoMachine(false);
    setVue((v) => (v === "machine" ? "reglages" : v));
  };
  const retirerMachine = (m: Machine) =>
    delierMachine(m.id).then(
      () => {
        setMachines((l) => l.filter((x) => x.id !== m.id));
        setServeurs((l) => l && l.filter((s) => s.machine !== m.id));
        setMachineActive((a) => (a === m.id ? "ici" : a));
        prevenir(`« ${m.nom} » retirée`);
      },
      // Depuis Réglages ou la page Machine, il n’y a pas d’en-tête d’onglet pour l’erreur : le petit message la montre.
      (e) => prevenir(typeof e === "string" ? e : "Ça n’a pas marché.", 4000, true),
    );

  // Rust nous prévient à chaque changement d’état d’un serveur.
  useEffect(
    () =>
      ecouterStatuts((id, statut, depuis) => {
        setServeurs((liste) => liste && liste.map((s) => (s.id === id ? { ...s, statut, depuis } : s)));
        // Plus en ligne : les ticks par seconde ne veulent plus rien dire.
        if (statut !== "en-ligne") setMesures((tout) => (tout[id]?.tps.length ? { ...tout, [id]: { ...tout[id], tps: [] } } : tout));
      }),
    [],
  );

  // Rust nous donne aussi la liste des joueurs connectés, dès qu’elle change.
  useEffect(
    () =>
      ecouterJoueurs((id, joueurs) =>
        setServeurs((liste) => liste && liste.map((s) => (s.id === id ? { ...s, joueurs } : s))),
      ),
    [],
  );

  // Ouvert à tes amis : l’adresse sur Internet de chaque serveur ouvert (ou pourquoi ça ne marche pas).
  const [acces, setAcces] = useState<Record<string, Acces>>({});
  useEffect(() => {
    const ranger = (liste: Acces[]) => setAcces(Object.fromEntries(liste.map((a) => [a.id, a])));
    etatsAcces().then(ranger, () => {});
    return ecouterAcces(ranger);
  }, []);

  // Les cartes d’erreur de l’Accueil (Java, port, plantage, mémoire), et la connexion Internet.
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

  // Le téléchargement de Java, avant le démarrage d’un serveur.
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

  // Les ticks par seconde du serveur affiché à l’accueil (un relevé toutes les dix secondes).
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

  // Quelqu’un vient d’essayer d’entrer sans être sur la liste blanche : on le montre tout de suite.
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
      (e) => setErreur(typeof e === "string" ? e : "Ça n’a pas marché."),
    );
  };

  const ajouter = (s: Serveur) => {
    setServeurs((liste) => [...(liste ?? []), s]);
    setIdActif(s.id);
    setOnglet("accueil");
    setVue("serveur");
  };

  // Petit message qui apparaît deux secondes en haut du contenu (« localhost » copié…).
  const [message, setMessage] = useState({ texte: "", visible: false, erreur: false });
  const minuterie = useRef(0);
  // duree = 0 : le message reste jusqu’au suivant (copie en cours…). erreur : icône d’alerte au lieu de la coche.
  const prevenir = (texte: string, duree = 2000, erreur = false) => {
    window.clearTimeout(minuterie.current);
    setMessage({ texte, visible: true, erreur });
    if (duree > 0) minuterie.current = window.setTimeout(() => setMessage((m) => ({ ...m, visible: false })), duree);
  };
  // « Pourquoi ça lag ? » : le rapport de spark s’ouvre dans le navigateur, même si on a changé de page.
  useEffect(
    () =>
      ecouterLag((_, lien, raison) => {
        if (lien) {
          openUrl(lien).catch(() => {});
          prevenir("Rapport ouvert dans ton navigateur", 3000);
        } else if (raison) setErreur(raison);
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );
  const cacherMessage = () => {
    window.clearTimeout(minuterie.current);
    setMessage((m) => ({ ...m, visible: false }));
  };

  // Clic droit › Dupliquer : la copie peut être longue (gros monde), on montre l’avancement.
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
        setErreur(typeof e === "string" ? e : "La copie n’a pas marché.");
      })
      .finally(arreter);
  };

  // Lance une action et affiche la phrase d’erreur de Rust si elle échoue.
  const agir = (action: Promise<void>) => {
    setErreur("");
    action.catch((e) => setErreur(typeof e === "string" ? e : "Ça n’a pas marché. Réessaie."));
  };

  // Démarrer. Sur une machine dédiée, la RAM peut manquer : on propose d’arrêter un autre serveur.
  const [manque, setManque] = useState<{ id: string; nom: string; manque: Manque } | null>(null);
  const demarrer = (s: Serveur) => {
    setErreur("");
    demarrerServeur(s.id).catch((e) =>
      estManqueMemoire(e) ? setManque({ id: s.id, nom: s.nom, manque: e }) : setErreur(typeof e === "string" ? e : "Ça n’a pas marché. Réessaie."),
    );
  };
  // Arrête `autre` proprement, attend qu’il soit vraiment arrêté (2 min au plus), puis démarre `id`.
  const arreterPuisDemarrer = (autre: string, id: string) =>
    new Promise<void>((fini, rate) => {
      let minuterie = 0;
      const desabonner = ecouterStatuts((x, statut) => {
        if (x !== autre || (statut !== "arrete" && statut !== "plante")) return;
        window.clearTimeout(minuterie);
        desabonner();
        demarrerServeur(id).then(fini, rate);
      });
      minuterie = window.setTimeout(() => {
        desabonner();
        rate("Le serveur met trop de temps à s’arrêter.");
      }, 120_000);
      arreterServeur(autre).catch((e) => {
        window.clearTimeout(minuterie);
        desabonner();
        rate(e);
      });
    }).catch((e) => setErreur(typeof e === "string" ? e : estManqueMemoire(e) ? "Toujours pas assez de mémoire." : "Ça n’a pas marché. Réessaie."));

  // Le menu du clic droit sur un serveur, et la fenêtre « Renommer » qu’il peut ouvrir.
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
  // Après la suppression (ou le retrait) : on montre le serveur d’à côté, ou l’assistant s’il n’en reste plus.
  const supprimer = (id: string, seulementRetirer: boolean) =>
    (seulementRetirer ? retirerServeur(id) : supprimerServeur(id)).then(() => {
      const reste = liste.filter((s) => s.id !== id);
      // Seulement ce serveur : ceux des autres machines restent dans la liste.
      setServeurs((l) => l && l.filter((s) => s.id !== id));
      if (idActif === id || !reste.some((s) => s.id === idActif)) {
        const i = liste.findIndex((s) => s.id === id);
        setIdActif(reste[Math.min(i, reste.length - 1)]?.id ?? null);
      }
      if (reste.length === 0) setVue("nouveau");
    });
  const renommer = (id: string, nom: string) =>
    renommerServeur(id, nom).then((s) => setServeurs((liste) => liste && liste.map((x) => (x.id === id ? { ...x, nom: s.nom } : x))));
  // Un serveur renvoyé par Rust après un réglage : on garde l’état du moment (statut, joueurs…) de l’interface.
  const mettreAJour = (s: Serveur) =>
    setServeurs(
      (liste) =>
        liste &&
        liste.map((x) =>
          // Tout ce que Rust renvoie (logiciel, version, jar, stockage max…), sauf l’état du moment que l’interface suit.
          x.id === s.id ? { ...s, machine: x.machine, statut: x.statut, depuis: x.depuis, joueurs: x.joueurs, java: x.java } : x,
        ),
    );

  // Seuls les serveurs de la machine choisie à gauche.
  const tous = serveurs ?? [];
  const liste = tous.filter((s) => (s.machine ?? "ici") === machineActive);
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
  const machineChoisie = machines.find((m) => m.id === machineActive); // absente : ce PC
  const surServeur = vue === "serveur" && serveur !== undefined;
  const onglets = serveur ? listeOnglets(serveur.logiciel) : [];
  const nomOnglet = onglets.find((o) => o.id === onglet)?.nom ?? "";
  const avecConsole = onglet === "accueil" || onglet === "console"; // la console remplit la page
  const avecExtensions = onglet === "extensions" && !!serveur && ["Paper", "Purpur", "Spigot", "Fabric", "Forge", "NeoForge", "Vanilla", "Youer", "Arclight", "Mohist"].includes(serveur.logiciel);
  // Premier lancement : rien à gauche, seulement le choix (on attend la liste pour ne rien faire clignoter).
  const accueil = !accueilVu && (serveurs === null || serveurs.length === 0 || revoirAccueil());

  const fenetreAjout = ajoutMachine && (
    <AjouterMachine
      onAjoutee={machineAjoutee}
      onPreparer={
        tutoMachine || vue === "machine"
          ? undefined
          : () => {
              setAjoutMachine(false);
              if (accueil) setTutoMachine(true);
              else setVue("machine");
            }
      }
      onFermer={() => setAjoutMachine(false)}
    />
  );

  if (accueil || vue === "machine") {
    const tuto = accueil ? tutoMachine : true;
    return (
      <div className={"fenetre" + (tuto ? " sans-cote" : "")}>
        <TitleBar />
        {serveurs !== null &&
          (tuto ? (
            <MachineDediee onQuitter={() => (accueil ? setTutoMachine(false) : setVue("reglages"))} onLiee={machineAjoutee} />
          ) : (
            <PremierLancement
              onLocal={() => {
                retenirAccueil();
                setAccueilVu(true);
              }}
              onMachine={() => setTutoMachine(true)}
              onDejaMachine={() => setAjoutMachine(true)}
            />
          ))}
        {fenetreAjout}
      </div>
    );
  }

  return (
    <div className="fenetre">
      <TitleBar />
      <div className="corps">
        <Sidebar
          serveurs={liste}
          machines={machines}
          machineActive={machineActive}
          machineOuverte={vue === "infos"}
          onMachine={(id) => {
            setMachineActive(id);
            // Machine hors ligne : ses serveurs sont grisés, on montre la machine.
            const eteinte = machines.some((m) => m.id === id && m.etat !== "en-ligne");
            const premier = eteinte ? undefined : tous.find((s) => (s.machine ?? "ici") === id);
            setIdActif(premier?.id ?? null);
            setVue(premier ? "serveur" : id === "ici" ? "nouveau" : "infos");
          }}
          onAjouterMachine={() => setAjoutMachine(true)}
          onVueMachine={() => setVue("infos")}
          idActif={surServeur ? serveur.id : null}
          nouveauOuvert={serveurs !== null && vue !== "reglages" && vue !== "infos" && !surServeur}
          reglagesOuverts={vue === "reglages"}
          onChoisir={ouvrir}
          onMenu={(id, x, y) => setMenu({ id, x, y })}
          onCreer={() => setVue("nouveau")}
          onReglages={() => setVue("reglages")}
        />

        <main className="main">
          {serveurs === null ? null : vue === "infos" ? (
            machineChoisie ? (
              <div className="scroll sans-barres">
                <PageMachine
                  machine={machineChoisie}
                  serveurs={liste}
                  onRenommer={() => setMachineARenommer(machineChoisie)}
                  onRetirer={() => setADelier(machineChoisie)}
                  onCopie={(texte) => navigator.clipboard?.writeText(texte).then(() => prevenir("Copié."), () => {})}
                  onMessage={(texte) => prevenir(texte, 3000)}
                />
              </div>
            ) : null
          ) : vue === "reglages" ? (
            <div className="scroll sans-barres">
              <Reglages machines={machines} onAjouterMachine={() => setAjoutMachine(true)} onRenommerMachine={setMachineARenommer} onDelierMachine={setADelier} />
            </div>
          ) : !surServeur ? (
            <NouveauServeur onCree={ajouter} onAnnuler={liste.length > 0 ? () => setVue("serveur") : undefined} machine={machineChoisie} />
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

              <div className={"edgebar" + (defile && !avecConsole ? " on" : "")} aria-hidden />
              <div ref={zoneScroll} className={"scroll" + (avecConsole ? " pleine" : "")} onScroll={(e) => setDefile(e.currentTarget.scrollTop > 4)}>
                {/* key : la page réapparaît en douceur quand on change d’onglet ou de serveur */}
                <div className={"page" + (ETROITES.includes(onglet) ? " etroite" : "")} key={serveur.id + onglet}>
                  <Filet>
                  {onglet === "accueil" ? (
                    <Accueil serveur={serveur} machine={machineChoisie} acces={acces[serveur.id]} mesures={mesures[serveur.id]} erreur={erreur} probleme={problemes[serveur.id]} internet={internet} onChange={mettreAJour} onOnglet={setOnglet} onCopie={prevenir} onErreur={setErreur} onManque={(m) => setManque({ id: serveur.id, nom: serveur.nom, manque: m })} />
                  ) : avecExtensions ? (
                    // L’onglet Plugins / Mods / Datapacks a son propre en-tête (bouton « Ajouter », page « Ajouter »).
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
                onDemarrer={() => demarrer(serveur)}
                onArreter={() => agir(arreterServeur(serveur.id))}
                onRedemarrer={() => agir(redemarrerServeur(serveur.id))}
                onForcer={() => agir(forcerArret(serveur.id))}
              />
            </>
          )}
          {/* Le petit message, sur toutes les pages (Réglages et page Machine comprises). */}
          <div
            className={
              "g toast" +
              (message.visible ? " show" : "") +
              (message.erreur ? " erreur" : "") +
              (surServeur ? (avecConsole ? " on-console" : "") : " sans-dock")
            }
            data-lens="btn"
            role="status"
            aria-live="polite"
          >
            {message.erreur ? <IconAlertCircle size={15} stroke={2.2} aria-hidden /> : <IconCheck size={15} stroke={2.4} aria-hidden />}
            <span>{message.texte}</span>
          </div>
          {demande && (
            <DemandePop
              nom={demande.nom}
              serveur={tous.find((s) => s.id === demande.id)?.nom ?? ""}
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
          onDossier={
            serveurMenu.machine
              ? undefined
              : () => {
                  ouvrir(serveurMenu.id);
                  agir(ouvrirDossier(serveurMenu.id));
                }
          }
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
          machine={!!serveurASupprimer.machine}
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
      {fenetreAjout}
      {manque && (
        <ManqueMemoire
          nom={manque.nom}
          manque={manque.manque}
          onArreterEtDemarrer={(autre) => arreterPuisDemarrer(autre, manque.id)}
          onFermer={() => setManque(null)}
        />
      )}
      {machineARenommer && (
        <Renommer
          titre="Renommer la machine"
          nom={machineARenommer.nom}
          onEnregistrer={(nom) =>
            renommerMachine(machineARenommer.id, nom).then(() =>
              setMachines((l) => l.map((x) => (x.id === machineARenommer.id ? { ...x, nom } : x))),
            )
          }
          onFermer={() => setMachineARenommer(null)}
        />
      )}
      {aDelier && (
        <Confirmer
          titre={`Retirer « ${aDelier.nom} » ?`}
          texte="Ses serveurs continuent de tourner dessus. Tu pourras la rajouter avec son code."
          bouton="Retirer"
          onOui={() => retirerMachine(aDelier)}
          onFermer={() => setADelier(null)}
        />
      )}
    </div>
  );
}
