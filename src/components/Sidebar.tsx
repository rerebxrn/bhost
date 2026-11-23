import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { IconCheck, IconDeviceDesktop, IconPlus, IconSelector, IconServer2, IconSettings } from "@tabler/icons-react";
import IconeServeur, { couleurDe } from "./IconeServeur";
import { nombre, nomLogiciel, texteStatut, type Machine, type Serveur } from "../types";
import "./Menus.css";
import "./Choix.css";
import "./Sidebar.css";

type Props = {
  serveurs: Serveur[]; // seulement ceux de la machine choisie
  machines: Machine[]; // machines dédiées reliées (vide : pas de sélecteur)
  machineActive: string; // « ici » (ce PC) ou l’id d’une machine dédiée
  idActif: string | null; // null quand on n’est pas sur un serveur
  nouveauOuvert: boolean;
  reglagesOuverts: boolean;
  machineOuverte: boolean; // la page « Machine » (vue d’ensemble) est affichée
  onMachine: (id: string) => void;
  onAjouterMachine: () => void;
  onVueMachine: () => void;
  onChoisir: (id: string) => void;
  onMenu: (id: string, x: number, y: number) => void; // clic droit sur un serveur
  onCreer: () => void;
  onReglages: () => void;
};

const ETATS: Record<Machine["etat"], string> = {
  "en-ligne": "En ligne",
  "hors-ligne": "Hors ligne",
  connexion: "Connexion…",
};

// La barre de gauche montre une machine à la fois.
// Sans machine dédiée : rien ne change (« Mes serveurs », comme avant).
// Avec une machine dédiée : un sélecteur en haut ; pour une machine dédiée, son état (stockage, température…).
// Ce PC n’affiche ni température ni stockage.
export default function Sidebar(p: Props) {
  const machine = p.machines.find((m) => m.id === p.machineActive);
  const horsLigne = !!machine && machine.etat !== "en-ligne";
  const ligne = (s: Serveur) => (
    <button
      key={s.id}
      type="button"
      className={"srv" + (s.id === p.idActif ? " sel" : "") + (horsLigne ? " off" : "")}
      aria-current={s.id === p.idActif ? "page" : undefined}
      disabled={horsLigne}
      title={horsLigne ? "La machine est hors ligne." : undefined}
      onClick={() => p.onChoisir(s.id)}
      onContextMenu={(e) => {
        e.preventDefault();
        if (!horsLigne) p.onMenu(s.id, e.clientX, e.clientY);
      }}
    >
      <IconeServeur couleur={couleurDe(s)} />
      <span className="srv-txt">
        <b>{s.nom}</b>
        <span>{horsLigne ? nomLogiciel(s) : `${nomLogiciel(s)} · ${texteStatut(s)}`}</span>
      </span>
    </button>
  );

  return (
    <aside className="side">
      {p.machines.length > 0 && (
        <Selecteur machines={p.machines} active={p.machineActive} onChoisir={p.onMachine} onAjouter={p.onAjouterMachine} />
      )}
      {machine && <EtatMachine m={machine} ouverte={p.machineOuverte} onOuvrir={p.onVueMachine} />}

      <div className="sec-title">{machine ? "Serveurs" : "Mes serveurs"}</div>
      {p.serveurs.map((s) => ligne(s))}

      {!horsLigne && (
        <button
          type="button"
          className={"new" + (p.nouveauOuvert ? " sel" : "")}
          aria-current={p.nouveauOuvert ? "page" : undefined}
          onClick={p.onCreer}
        >
          <IconPlus size={16} stroke={2} aria-hidden />
          Créer un serveur
        </button>
      )}

      {p.machines.length === 0 && (
        <button type="button" className="side-set side-machine" onClick={p.onAjouterMachine}>
          <IconServer2 size={16} stroke={1.8} aria-hidden />
          Machine dédiée
          <IconPlus size={14} stroke={2} aria-hidden className="side-plus" />
        </button>
      )}
      <div className="side-foot">
        <button
          type="button"
          className={"side-set" + (p.reglagesOuverts ? " sel" : "")}
          aria-current={p.reglagesOuverts ? "page" : undefined}
          onClick={p.onReglages}
        >
          <IconSettings size={16} stroke={2} aria-hidden />
          Réglages
        </button>
      </div>
    </aside>
  );
}

// ---------- Le sélecteur de machine, en haut ----------

function Selecteur({
  machines,
  active,
  onChoisir,
  onAjouter,
}: {
  machines: Machine[];
  active: string;
  onChoisir: (id: string) => void;
  onAjouter: () => void;
}) {
  const bouton = useRef<HTMLButtonElement>(null);
  const [place, setPlace] = useState<{ x: number; y: number; w: number } | null>(null);
  const [ouvert, setOuvert] = useState(false);
  const m = machines.find((x) => x.id === active);

  useLayoutEffect(() => {
    if (!ouvert || !bouton.current) return;
    const b = bouton.current.getBoundingClientRect();
    setPlace({ x: b.left, y: b.bottom + 8, w: b.width });
  }, [ouvert]);

  const fermer = () => {
    setOuvert(false);
    setPlace(null);
  };

  useEffect(() => {
    if (!ouvert) return;
    const touche = (e: KeyboardEvent) => e.key === "Escape" && fermer();
    window.addEventListener("keydown", touche);
    return () => window.removeEventListener("keydown", touche);
  }, [ouvert]);

  const item = (id: string, nom: string, icone: ReactNode, etat?: Machine["etat"]) => (
    <button
      key={id}
      type="button"
      role="menuitemradio"
      aria-checked={id === active}
      className={"pm-item choix-item sm-item" + (id === active ? " sel" : "")}
      onClick={() => {
        onChoisir(id);
        fermer();
      }}
    >
      <i>
        <IconCheck size={13} stroke={2.4} aria-hidden />
      </i>
      {icone}
      <span className="sm-nom">{nom}</span>
      {etat && <span className={"sm-point " + etat} aria-label={ETATS[etat]} />}
    </button>
  );

  return (
    <>
      <button
        ref={bouton}
        type="button"
        className={"sel-machine" + (ouvert ? " on" : "")}
        aria-haspopup="menu"
        aria-expanded={ouvert}
        onClick={() => setOuvert(!ouvert)}
      >
        <span className="sm-ico">{m ? <IconServer2 size={17} stroke={1.8} /> : <IconDeviceDesktop size={17} stroke={1.8} />}</span>
        <span className="sm-txt">
          <b>{m ? m.nom : "Ce PC"}</b>
          {m && (
            <span>
              <i className={"sm-point " + m.etat} aria-hidden />
              {ETATS[m.etat]}
            </span>
          )}
        </span>
        <IconSelector size={14} stroke={2} aria-hidden className="sm-fleche" />
      </button>
      {ouvert &&
        createPortal(
          <>
            <div className="ctx-catch choix-catch" onMouseDown={fermer} />
            <div
              className="g ctx choix-menu sm-menu"
              role="menu"
              aria-label="Machine"
              data-lens="panel"
              style={place ? { left: place.x, top: place.y, width: place.w } : { visibility: "hidden" }}
            >
              {item("ici", "Ce PC", <IconDeviceDesktop size={15} stroke={1.8} aria-hidden />)}
              {machines.map((x) => item(x.id, x.nom, <IconServer2 size={15} stroke={1.8} aria-hidden />, x.etat))}
              <div className="pm-sep" />
              <button
                type="button"
                role="menuitem"
                className="pm-item choix-item sm-item"
                onClick={() => {
                  fermer();
                  onAjouter();
                }}
              >
                <i />
                <IconPlus size={15} stroke={1.8} aria-hidden />
                <span className="sm-nom">Ajouter une machine…</span>
              </button>
            </div>
          </>,
          document.body,
        )}
    </>
  );
}

// ---------- L’état d’une machine dédiée : stockage, mémoire, température ----------

const pct = (u: number, t: number) => (t > 0 ? Math.min(100, Math.round((u / t) * 100)) : 0);
const go = (n: number) => (n >= 1000 ? `${nombre(n / 1000)} To` : `${Math.round(n)} Go`);

// La température : grise, orange à 70 °C, rouge à 80 °C.
const niveauTemperature = (t: number) => (t >= 80 ? "chaud" : t >= 70 ? "tiede" : "");

function EtatMachine({ m, ouverte, onOuvrir }: { m: Machine; ouverte: boolean; onOuvrir: () => void }) {
  if (m.etat !== "en-ligne") {
    return (
      <button type="button" className={"sm-etat hors" + (ouverte ? " sel" : "")} onClick={onOuvrir}>
        <b>Éteinte ou sans Internet</b>
        <span>{m.vueIlYa ? `Vue ${ilYa(m.vueIlYa)}.` : "Allume-la pour retrouver tes serveurs."}</span>
      </button>
    );
  }
  const d = m.disque;
  const r = m.memoire;
  const t = m.temperature;
  return (
    <button type="button" className={"sm-etat" + (ouverte ? " sel" : "")} onClick={onOuvrir} aria-label={`Ouvrir la machine ${m.nom}`}>
      {d && <Jauge nom="Stockage" valeur={`${go(d.utiliseGo)} / ${go(d.totalGo)}`} pourcent={pct(d.utiliseGo, d.totalGo)} />}
      {r && <Jauge nom="Mémoire réservée" valeur={`${nombre(r.reserveeGo)} / ${nombre(r.totalGo)} Go`} pourcent={pct(r.reserveeGo, r.totalGo)} />}
      {(t != null || m.processeur != null) && (
        <span className="sm-ligne">
          <span>Processeur</span>
          <span className="sm-ligne-v">
            {t != null && <b className={"tnum " + niveauTemperature(t)}>{Math.round(t)} °C</b>}
            {t != null && m.processeur != null && <span aria-hidden>·</span>}
            {m.processeur != null && <b className="tnum">{Math.round(m.processeur)} %</b>}
          </span>
        </span>
      )}
    </button>
  );
}

function Jauge({ nom, valeur, pourcent }: { nom: string; valeur: string; pourcent: number }) {
  const niveau = pourcent >= 90 ? " plein" : pourcent >= 75 ? " haut" : "";
  return (
    <span className="sm-jauge">
      <span className="sm-jauge-t">
        <span>{nom}</span>
        <b className="tnum">{valeur}</b>
      </span>
      <span className={"sm-barre" + niveau} role="meter" aria-label={nom} aria-valuenow={pourcent} aria-valuemin={0} aria-valuemax={100}>
        <i style={{ width: `${pourcent}%` }} />
      </span>
    </span>
  );
}

function ilYa(ms: number) {
  const min = Math.round((Date.now() - ms) / 60000);
  if (min < 2) return "à l’instant";
  if (min < 60) return `il y a ${min} min`;
  const h = Math.round(min / 60);
  return h < 24 ? `il y a ${h} h` : `il y a ${Math.round(h / 24)} j`;
}
