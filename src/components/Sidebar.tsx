import { IconPlus, IconSettings } from "@tabler/icons-react";
import IconeServeur, { couleurDe } from "./IconeServeur";
import { nombre, nomLogiciel, texteStatut, type Machine, type Serveur } from "../types";
import "./Sidebar.css";

type Props = {
  serveurs: Serveur[]; // seulement ceux de la machine choisie
  machines: Machine[]; // machines dédiées reliées
  machineActive: string; // « ici » (ce PC) ou l’id d’une machine dédiée
  idActif: string | null; // null quand on n’est pas sur un serveur
  nouveauOuvert: boolean;
  reglagesOuverts: boolean;
  machineOuverte: boolean; // la page « Machine » (vue d’ensemble) est affichée
  onVueMachine: () => void;
  onChoisir: (id: string) => void;
  onMenu: (id: string, x: number, y: number) => void; // clic droit sur un serveur
  onCreer: () => void;
  onReglages: () => void;
};

// La barre de gauche montre une machine à la fois (choisie dans la barre de titre).
// Toujours la même barre (décidé le 22/11) : une machine dédiée ajoute seulement ses jauges en haut.
// En haut, la liste qui défile ; en bas, fixes, « Créer » et Réglages.
// Toutes les icônes sont centrées à 32 px, tous les textes partent de 56 px.
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
      <div className="side-liste">
        {machine && <EtatMachine m={machine} ouverte={p.machineOuverte} onOuvrir={p.onVueMachine} />}

        <div className="grp">
          <span className="grp-nm">Serveurs</span>
          {!horsLigne && (
            <button type="button" className="grp-b" aria-label="Créer un serveur" title="Créer un serveur" onClick={p.onCreer}>
              <IconPlus size={14} stroke={2} aria-hidden />
            </button>
          )}
        </div>
        {p.serveurs.map((s) => ligne(s))}
      </div>

      <div className="side-bas">
        {!horsLigne && (
          <button
            type="button"
            className={"new" + (p.nouveauOuvert ? " sel" : "")}
            aria-current={p.nouveauOuvert ? "page" : undefined}
            onClick={p.onCreer}
          >
            <span className="slot">
              <IconPlus size={16} stroke={2} aria-hidden />
            </span>
            Créer
          </button>
        )}
        <div className="side-foot">
          <button
            type="button"
            className={"side-set" + (p.reglagesOuverts ? " sel" : "")}
            aria-current={p.reglagesOuverts ? "page" : undefined}
            onClick={p.onReglages}
          >
            <span className="slot">
              <IconSettings size={16} stroke={2} aria-hidden />
            </span>
            Réglages
          </button>
        </div>
      </div>
    </aside>
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
