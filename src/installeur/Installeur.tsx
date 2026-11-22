// BHOST Setup : l'installeur de BHOST (setup/). La fenêtre n'est que la plaque de verre du premier lancement.
// Bienvenue → Dossier → Installation → Prêt. Derrière, l'installeur Windows de BHOST tourne sans fenêtre :
// les mises à jour et la désinstallation (Paramètres de Windows) marchent comme avant.
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { IconArrowRight, IconChevronLeft, IconCircleCheckFilled } from "@tabler/icons-react";
import { Tuile } from "../pages/PremierLancement";
import "../App.css";
import "../components/TitleBar.css";
import "../pages/Parametres.css";
import "../pages/Reglages.css";
import "../pages/PremierLancement.css";
import "../pages/MachineDediee.css";
import "./Installeur.css";

type Etape = "bienvenue" | "dossier" | "installation" | "pret" | "erreur";
type Infos = { dossier: string; lecteur: string; libreGo: number };

const fenetre = getCurrentWindow();
const phrase = (e: unknown) => (typeof e === "string" ? e : "L’installation a échoué. Réessaie.");

export default function Installeur() {
  const [etape, setEtape] = useState<Etape>("bienvenue");
  const [infos, setInfos] = useState<Infos | null>(null);
  const [pct, setPct] = useState(0);
  const [erreur, setErreur] = useState("");
  const enCours = useRef(false);

  useEffect(() => {
    invoke<Infos>("infos").then(setInfos).catch(() => {});
    const avancement = listen<number>("avancement", (e) => setPct(e.payload));
    // Pendant l'installation, on ne ferme pas (Alt+F4 compris).
    const fermeture = fenetre.onCloseRequested((e) => {
      if (enCours.current) e.preventDefault();
    });
    return () => {
      avancement.then((f) => f());
      fermeture.then((f) => f());
    };
  }, []);

  const changer = async () => {
    const choisi = await open({ directory: true, defaultPath: infos?.dossier }).catch(() => null);
    if (typeof choisi === "string") invoke<Infos>("infos", { dossier: choisi }).then(setInfos).catch(() => {});
  };

  const installer = () => {
    if (!infos) return;
    enCours.current = true;
    setPct(0);
    setEtape("installation");
    invoke("installer", { dossier: infos.dossier })
      .then(() => {
        setPct(100);
        setTimeout(() => setEtape("pret"), 400);
      })
      .catch((e) => {
        setErreur(phrase(e));
        setEtape("erreur");
      })
      .finally(() => {
        enCours.current = false;
      });
  };

  const ouvrir = () => {
    if (!infos) return;
    invoke("ouvrir", { dossier: infos.dossier }).catch((e) => {
      setErreur(phrase(e));
      setEtape("erreur");
    });
  };

  const [titre, sousTitre] = {
    bienvenue: ["Installer BHOST", "Héberge tes serveurs Minecraft sur ton PC."],
    dossier: ["Où installer BHOST ?", infos ? `${infos.libreGo} Go libres sur ${infos.lecteur}` : " "],
    installation: ["Installation…", "Quelques secondes."],
    pret: ["BHOST est prêt", "Tu le retrouves dans le menu Démarrer."],
    erreur: ["Installation impossible", erreur],
  }[etape];

  const bouton = {
    bienvenue: { texte: "Continuer", action: () => setEtape("dossier") },
    dossier: { texte: "Installer", action: installer },
    pret: { texte: "Ouvrir BHOST", action: ouvrir },
    erreur: { texte: "Réessayer", action: () => setEtape("dossier") },
    installation: null,
  }[etape];

  return (
    <div className="ins-fenetre pl" data-tauri-drag-region>
      <div className="wc-group ins-wc">
        <button type="button" className="wc" aria-label="Réduire" onClick={() => fenetre.minimize()}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M0 5.5h10" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
        <button
          type="button"
          className="wc close"
          aria-label="Fermer"
          disabled={etape === "installation"}
          onClick={() => fenetre.close()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M.5.5l9 9M9.5.5l-9 9" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
      </div>

      <section className="ins-panneau params" key={etape} data-tauri-drag-region>
        <header className="pl-tete pl-entree" data-tauri-drag-region>
          {etape === "pret" ? (
            <span className="ins-ok" aria-hidden>
              <Tuile />
              <IconCircleCheckFilled size={24} />
            </span>
          ) : (
            <Tuile />
          )}
          <h1>{titre}</h1>
          <p className={etape === "dossier" ? "tnum" : undefined}>{sousTitre}</p>
        </header>

        {etape === "dossier" && (
          <section className="surf ins-dossier pl-entree" style={{ animationDelay: "0.1s" }}>
            <div className="row">
              <div className="lbl">
                <span>Dossier</span>
                <span className="t2 rg-chemin" title={infos?.dossier}>
                  {infos?.dossier ?? "…"}
                </span>
              </div>
              <button type="button" className="btn" onClick={changer}>
                Changer…
              </button>
            </div>
          </section>
        )}

        {bouton === null ? (
          <div className="md-progres ins-progres pl-entree" role="status" style={{ animationDelay: "0.15s" }}>
            <span className="t2 tnum">{pct} %</span>
            <span className="md-barre fixe">
              <i style={{ transform: `scaleX(${Math.max(0.02, pct / 100)})` }} />
            </span>
          </div>
        ) : (
          <div className="pl-suite pl-entree" style={{ animationDelay: "0.15s" }}>
            <button type="button" className="btn acc pl-bouton" data-gel onClick={bouton.action} disabled={!infos}>
              {bouton.texte}
              <IconArrowRight size={17} stroke={2.2} aria-hidden />
            </button>
            {etape === "bienvenue" && <p className="pl-note">Gratuit et open source.</p>}
            {etape === "dossier" && (
              <p className="pl-note">
                <button type="button" className="pl-lien ins-retour" onClick={() => setEtape("bienvenue")}>
                  <IconChevronLeft size={14} stroke={2.2} aria-hidden />
                  Retour
                </button>
              </p>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
