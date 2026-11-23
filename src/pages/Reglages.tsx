import { useEffect, useState, type KeyboardEvent } from "react";
import { IconEye, IconEyeOff, IconFolder, IconPlus } from "@tabler/icons-react";
import Curseur from "../components/Curseur";
import {
  changerDossierServeurs,
  chercherMiseAJour,
  choisirDossier,
  installerMiseAJour,
  ouvrirDossierServeurs,
  reglagesBhost,
  reglerBhost,
  suivreMiseAJour,
  type ChangementReglages,
  type VueReglages,
} from "../api";
import { useTheme } from "../theme/ThemeProvider";
import type { Fond, Mode } from "../theme/palette";
import { nombre, type Machine } from "../types";
import { afficherCode } from "../code";
import "./Parametres.css";
import "./Reglages.css";

// Le thème (Azur ou Ambre), puis le mode (comme Apparence dans Réglages de macOS).
const FONDS: { id: Fond; nom: string }[] = [
  { id: "azur", nom: "Azur" },
  { id: "ambre", nom: "Ambre" },
];

const MODES: { id: Mode; nom: string }[] = [
  { id: "clair", nom: "Clair" },
  { id: "sombre", nom: "Sombre" },
  { id: "systeme", nom: "Auto" },
];

// Flèches dans un groupe de vignettes : comme de vrais boutons radio
// (Tab entre dans le groupe, les flèches choisissent).
function fleches<T extends string>(ids: T[], actuel: T, choisir: (id: T) => void) {
  return (e: KeyboardEvent<HTMLDivElement>) => {
    const pas: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };
    if (!pas[e.key]) return;
    e.preventDefault();
    const i = (ids.indexOf(actuel) + pas[e.key] + ids.length) % ids.length;
    choisir(ids[i]);
    (e.currentTarget.children[i] as HTMLElement | undefined)?.focus();
  };
}

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n’a pas marché.");

// « 3,8 Go », « 640 Mo ».
function taille(octets: number): string {
  const mo = octets / 1_048_576;
  return mo >= 1024 ? `${nombre(mo / 1024)} Go` : `${Math.round(mo)} Mo`;
}

// Où en est la recherche de mise à jour.
type Maj =
  | { etape: "rien" }
  | { etape: "recherche" }
  | { etape: "a-jour" }
  | { etape: "dispo"; version: string }
  | { etape: "telechargement"; version: string; pourcent: number }
  | { etape: "erreur"; texte: string };

// Réglages de BHOST (bouton en bas de la barre latérale), d’après la maquette.
type Props = {
  machines: Machine[];
  onAjouterMachine: () => void;
  onRenommerMachine: (m: Machine) => void;
  onDelierMachine: (m: Machine) => void;
};

const ETATS_MACHINE: Record<Machine["etat"], string> = {
  "en-ligne": "En ligne",
  "hors-ligne": "Hors ligne",
  connexion: "Connexion…",
};

export default function Reglages({ machines, onAjouterMachine, onRenommerMachine, onDelierMachine }: Props) {
  const theme = useTheme();
  const [vue, setVue] = useState<VueReglages | null>(null);
  const [pseudo, setPseudo] = useState("");
  const [erreur, setErreur] = useState("");
  const [maj, setMaj] = useState<Maj>({ etape: "rien" });
  const [codeVu, setCodeVu] = useState<string | null>(null); // la machine dont on montre le code

  const recevoir = (v: VueReglages) => {
    setVue(v);
    setPseudo(v.pseudo);
  };
  useEffect(() => {
    reglagesBhost().then(recevoir, (e) => setErreur(phrase(e)));
  }, []);

  // L’avancement du téléchargement de la mise à jour.
  useEffect(
    () =>
      suivreMiseAJour((pourcent) =>
        setMaj((m) => (m.etape === "telechargement" ? { ...m, pourcent } : m)),
      ),
    [],
  );

  const chercher = () => {
    setMaj({ etape: "recherche" });
    chercherMiseAJour().then(
      (v) => setMaj(v.etat === "dispo" && v.version ? { etape: "dispo", version: v.version } : { etape: "a-jour" }),
      (e) => setMaj({ etape: "erreur", texte: phrase(e) }),
    );
  };

  const installer = (version: string) => {
    setMaj({ etape: "telechargement", version, pourcent: 0 });
    // Si tout va bien, BHOST se ferme et l’installeur le rouvre : on ne revient pas ici.
    installerMiseAJour().catch((e) => setMaj({ etape: "erreur", texte: phrase(e) }));
  };

  // La ligne sous « Version 0.1.0 ».
  const etatMaj = (() => {
    switch (maj.etape) {
      case "recherche":
        return "Recherche…";
      case "a-jour":
        return "Tu as la dernière version.";
      case "dispo":
        return `Version ${maj.version} disponible. Tes serveurs s’arrêtent proprement.`;
      case "telechargement":
        return maj.pourcent >= 100 ? "Arrêt des serveurs…" : `Téléchargement… ${maj.pourcent} %`;
      case "erreur":
        return maj.texte;
      default:
        return "Cherchée aussi à chaque lancement.";
    }
  })();

  const regler = (changement: ChangementReglages) => {
    setErreur("");
    // L’interrupteur bouge tout de suite ; Rust confirme juste après.
    setVue((v) => v && { ...v, ...changement });
    reglerBhost(changement).then(recevoir, (e) => {
      setErreur(phrase(e));
      reglagesBhost().then(recevoir, () => {});
    });
  };

  const garderPseudo = () => {
    if (vue && pseudo.trim() !== vue.pseudo) regler({ pseudo: pseudo.trim() });
  };

  const changerDossier = async () => {
    const dossier = await choisirDossier("Où ranger tes serveurs ?");
    if (!dossier) return;
    changerDossierServeurs(dossier).then(
      () => reglagesBhost().then(recevoir),
      (e) => setErreur(phrase(e)),
    );
  };

  const interrupteur = (id: string, nom: string, aide: string, actif: boolean, changer: () => void) => (
    <div className="row">
      <div className="lbl">
        <span id={id}>{nom}</span>
        <span className="t2">{aide}</span>
      </div>
      <button type="button" role="switch" aria-checked={actif} aria-labelledby={id} className={"sw" + (actif ? " on" : "")} onClick={changer}>
        <i />
      </button>
    </div>
  );

  const teinte = Math.round(theme.teinte * 100);

  return (
    <div className="page etroite reglages">
      <header className="head">
        <h1>Réglages</h1>
        {erreur && (
          <p className="alerte" role="alert">
            {erreur}
          </p>
        )}
      </header>

      <div className="params">
        <div className="grp-t">Général</div>
        <section className="surf">
          {interrupteur(
            "rg-windows",
            "Lancer avec Windows",
            vue?.garder ? "BHOST démarre en bas à droite." : "BHOST s’ouvre quand tu allumes le PC.",
            !!vue?.lancerWindows,
            () => regler({ lancerWindows: !vue?.lancerWindows }),
          )}
          {interrupteur("rg-garder", "Garder les serveurs allumés", "Fermer la fenêtre range BHOST en bas à droite.", !!vue?.garder, () =>
            regler({ garder: !vue?.garder }),
          )}
          {interrupteur("rg-propre", "Arrêter proprement", "Quand Windows s’éteint, les mondes sont enregistrés.", !!vue?.proprement, () =>
            regler({ proprement: !vue?.proprement }),
          )}
          {interrupteur("rg-veille", "Empêcher la veille", "Tant qu’un serveur tourne.", !!vue?.veille, () => regler({ veille: !vue?.veille }))}
          {interrupteur("rg-notifs", "Notifications", "Demandes et automatisations.", !!vue?.notifications, () =>
            regler({ notifications: !vue?.notifications }),
          )}
        </section>

        <div className="grp-t">Toi</div>
        <section className="surf">
          <div className="row">
            <div className="lbl">
              <label htmlFor="rg-pseudo">Ton pseudo Minecraft</label>
              <span className="t2">OP sur chaque nouveau serveur.</span>
            </div>
            <input
              id="rg-pseudo"
              className="field texte rg-pseudo"
              type="text"
              maxLength={17}
              value={pseudo}
              placeholder="Steve"
              spellCheck={false}
              disabled={!vue}
              onChange={(e) => setPseudo(e.target.value)}
              onBlur={garderPseudo}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") setPseudo(vue?.pseudo ?? "");
              }}
            />
          </div>
        </section>

        <div className="grp-t">Stockage</div>
        <section className="surf">
          <div className="row">
            <div className="lbl">
              <span>Dossier des serveurs</span>
              <span className="t2 rg-chemin" title={vue?.dossier}>
                {vue?.dossier ?? "…"}
              </span>
              {vue && (
                <span className="t2">
                  {vue.serveurs} serveur{vue.serveurs > 1 ? "s" : ""} · {taille(vue.octets)}
                </span>
              )}
            </div>
            <div className="rg-btns">
              <button type="button" className="btn" onClick={() => ouvrirDossierServeurs().catch((e) => setErreur(phrase(e)))}>
                <IconFolder size={14} stroke={2} aria-hidden />
                Ouvrir
              </button>
              <button type="button" className="btn" onClick={changerDossier}>
                Changer…
              </button>
            </div>
          </div>
        </section>

        <div className="grp-t">Machines dédiées</div>
        <section className="surf">
          {machines.map((m) => (
            <div className="row" key={m.id}>
              <div className="lbl">
                <span>{m.nom}</span>
                <span className="t2">
                  <span className={"rg-etat " + m.etat} aria-hidden />
                  {ETATS_MACHINE[m.etat]} · {m.serveurs} serveur{m.serveurs > 1 ? "s" : ""}
                </span>
              </div>
              <div className="rg-btns">
                <button
                  type="button"
                  className="btn rg-code"
                  aria-label={codeVu === m.id ? "Cacher le code" : "Afficher le code"}
                  onClick={() => setCodeVu((v) => (v === m.id ? null : m.id))}
                >
                  {codeVu === m.id ? <IconEyeOff size={14} stroke={2} aria-hidden /> : <IconEye size={14} stroke={2} aria-hidden />}
                  <span className="tnum">{codeVu === m.id ? afficherCode(m.code) : "••••-••••"}</span>
                </button>
                <button type="button" className="btn" onClick={() => onRenommerMachine(m)}>
                  Renommer…
                </button>
                <button type="button" className="btn" onClick={() => onDelierMachine(m)}>
                  Retirer…
                </button>
              </div>
            </div>
          ))}
          <div className="row">
            <div className="lbl">
              <span>{machines.length === 0 ? "Aucune machine" : "Une autre machine"}</span>
              <span className="t2">Un 2ᵉ PC qui héberge tes serveurs jour et nuit.</span>
            </div>
            <button type="button" className="btn" onClick={onAjouterMachine}>
              <IconPlus size={14} stroke={2} aria-hidden />
              Ajouter…
            </button>
          </div>
        </section>

        <div className="grp-t">Apparence</div>
        <section className="surf">
          <div className="row">
            <span>Thème</span>
            <div
              className="appear"
              role="radiogroup"
              aria-label="Thème"
              onKeyDown={fleches(FONDS.map((f) => f.id), theme.fond, (fond) => theme.changer({ fond }))}
            >
              {FONDS.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  role="radio"
                  aria-checked={theme.fond === f.id}
                  tabIndex={theme.fond === f.id ? 0 : -1}
                  className={"ap" + (theme.fond === f.id ? " sel" : "")}
                  onClick={() => theme.changer({ fond: f.id })}
                >
                  {/* Toujours la version claire : c’est là que les deux thèmes se distinguent le mieux */}
                  <i className={"v-" + f.id + "-clair"}>
                    <span />
                  </i>
                  {f.nom}
                </button>
              ))}
            </div>
          </div>
          <div className="row">
            <span>Mode</span>
            <div
              className="appear"
              role="radiogroup"
              aria-label="Mode"
              onKeyDown={fleches(MODES.map((m) => m.id), theme.mode, (mode) => theme.changer({ mode }))}
            >
              {MODES.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  role="radio"
                  aria-checked={theme.mode === m.id}
                  tabIndex={theme.mode === m.id ? 0 : -1}
                  className={"ap" + (theme.mode === m.id ? " sel" : "")}
                  onClick={() => theme.changer({ mode: m.id })}
                >
                  {/* Le thème choisi dans ce mode (Auto : moitié clair, moitié sombre) */}
                  <i className={"v-" + theme.fond + "-" + m.id}>
                    <span />
                  </i>
                  {m.nom}
                </button>
              ))}
            </div>
          </div>
          <div className="row">
            <div className="lbl">
              <span>Liquid Glass</span>
              <span className="t2 tnum">{teinte} %</span>
            </div>
            <div className="rg-verre">
              <span>Transparent</span>
              <Curseur min={0} max={100} valeur={teinte} unite="%" etiquette="Opacité du verre" onChange={(v) => theme.changer({ teinte: v / 100 })} />
              <span>Teinté</span>
            </div>
          </div>
          {interrupteur("rg-reel", "Réfraction réelle", "Le verre déforme ce qui est derrière.", theme.reel, () => theme.changer({ reel: !theme.reel }))}
        </section>

        <div className="grp-t">BHOST</div>
        <section className="surf">
          <div className="row">
            <div className="lbl">
              <span>Version {vue?.version ?? ""}</span>
              <span className={"t2" + (maj.etape === "erreur" ? " rg-maj-err" : "")} role="status" aria-live="polite">
                {etatMaj}
              </span>
            </div>
            {maj.etape === "dispo" ? (
              <button type="button" className="btn acc" onClick={() => installer(maj.version)}>
                Installer
              </button>
            ) : (
              <button type="button" className="btn" disabled={maj.etape === "recherche" || maj.etape === "telechargement"} onClick={chercher}>
                Rechercher les mises à jour
              </button>
            )}
          </div>
          <div className="row">
            <div className="lbl">
              <span>Écran de bienvenue</span>
              <span className="t2">Revoir le choix entre ce PC et une machine dédiée.</span>
            </div>
            <button type="button" className="btn" onClick={revoirBienvenue}>
              Revoir
            </button>
          </div>
        </section>

        {/* Obligatoire (règles d’utilisation de Minecraft) : en anglais, comme le texte officiel. */}
        <p className="rg-mention">NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.</p>
      </div>
    </div>
  );
}

// L’appli se recharge et remontre l’écran du premier lancement (voir App.tsx).
function revoirBienvenue() {
  try {
    localStorage.setItem("bhost.accueil", "revoir");
  } catch {
    return;
  }
  window.location.reload();
}
