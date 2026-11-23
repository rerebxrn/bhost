import { useEffect, useState } from "react";
import { IconPlus } from "@tabler/icons-react";
import Fenetre from "../components/Fenetre";
import Choix from "../components/Choix";
import { Compteur } from "./Fichiers";
import { quand } from "./Sauvegardes";
import {
  activerAutomatisation,
  automatisationsServeur,
  ecouterAutomatisations,
  enregistrerAutomatisation,
  supprimerAutomatisation,
  type ActionAuto,
  type Automatisation,
  type Declencheur,
  type VueAutomatisations,
} from "../api";
import { nomExtensions, type Serveur } from "../types";
import "./Automatisations.css";

type Props = {
  serveur: Serveur;
  onErreur: (texte: string) => void;
  onInfo: (texte: string) => void;
};

// « Alors… » : le nom dans le menu (et dans la liste).
const ACTIONS: Record<ActionAuto, string> = {
  start: "Démarrer le serveur",
  stop: "Arrêter le serveur",
  restart: "Redémarrer le serveur",
  backup: "Faire une sauvegarde",
  say: "Envoyer une annonce",
  cmd: "Lancer une commande",
  notify: "Me prévenir",
  update: "Tout mettre à jour",
};

// « Quand » : le nom dans le menu, une petite aide, les actions possibles (la première est choisie d’office).
const DECLENCHEURS: Record<Declencheur, { nom: string; aide: string; actions: ActionAuto[] }> = {
  time: { nom: "À une heure précise", aide: "", actions: ["restart", "stop", "start", "backup", "say", "cmd", "notify"] },
  crash: { nom: "Si le serveur plante", aide: "3 essais maximum.", actions: ["restart", "notify"] },
  empty: { nom: "Si personne ne joue", aide: "", actions: ["stop", "restart", "backup"] },
  join: { nom: "Quand un joueur arrive", aide: "{joueur} = son pseudo.", actions: ["say", "cmd", "notify"] },
  boot: { nom: "Quand le serveur démarre", aide: "", actions: ["cmd", "say", "notify"] },
  lag: { nom: "Si le serveur lag", aide: "Moins de 15 TPS pendant 1 min.", actions: ["notify", "say", "restart", "cmd"] },
  mem: { nom: "Si la mémoire est presque pleine", aide: "Plus de 90 % pendant 5 min.", actions: ["restart", "notify"] },
  wake: { nom: "Quand quelqu’un veut entrer", aide: "Le serveur dort. BHOST l’allume dès qu’un ami se connecte.", actions: ["start"] },
  upd: { nom: "Si une mise à jour sort", aide: "BHOST regarde toutes les 6 h.", actions: ["update", "notify"] },
};

const ORDRE: Declencheur[] = ["time", "crash", "empty", "join", "boot", "lag", "mem", "wake", "upd"];
const JOURS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const LETTRES = ["L", "M", "M", "J", "V", "S", "D"];
const MOIS = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
const REPETER = [
  { id: "once" as const, nom: "Une fois" },
  { id: "daily" as const, nom: "Chaque jour" },
  { id: "days" as const, nom: "Certains jours" },
  { id: "every" as const, nom: "Régulièrement" },
];
const AVANT = [
  { id: "1" as const, nom: "1 min avant" },
  { id: "5" as const, nom: "5 min avant" },
  { id: "15" as const, nom: "15 min avant" },
];

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n’a pas marché.");
const p2 = (n: number) => String(n).padStart(2, "0");
const majuscule = (x: string) => x.charAt(0).toUpperCase() + x.slice(1);
const minuscule = (x: string) => x.charAt(0).toLowerCase() + x.slice(1);
const jourSemaine = (d: Date) => (d.getDay() + 6) % 7; // 0 = lundi
const dateTexte = (d: Date) => `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`;

// Le jour « k » à partir d’aujourd’hui, à l’heure « 05:00 ».
function a(k: number, heure: string, maintenant: Date): Date {
  const d = new Date(maintenant);
  d.setDate(d.getDate() + k);
  const [h, m] = heure.split(":").map(Number);
  d.setHours(h || 0, m || 0, 0, 0);
  return d;
}

// Le jour et l’heure d’une automatisation « une fois ».
function dateDe(t: Automatisation): Date | null {
  const [y, mo, j] = t.date.split("-").map(Number);
  if (!y) return null;
  const [h, m] = t.time.split(":").map(Number);
  return new Date(y, mo - 1, j, h || 0, m || 0);
}

// La prochaine fois (comme dans automatisations.rs) : null si c’est passé.
function prochaine(t: Automatisation, maintenant: Date): Date | null {
  if (t.trig !== "time") return null;
  if (t.when === "every") {
    for (let k = 0; k <= 1; k++) {
      for (let h = 0; h < 24; h += Math.max(1, t.hours)) {
        const d = a(k, `${p2(h)}:00`, maintenant);
        if (d > maintenant) return d;
      }
    }
    return null;
  }
  if (t.when === "once") {
    const d = dateDe(t);
    return d && d > maintenant ? d : null;
  }
  for (let k = 0; k <= 8; k++) {
    const d = a(k, t.time, maintenant);
    if (d > maintenant && (t.when === "daily" || t.days.includes(jourSemaine(d)))) return d;
  }
  return null;
}

// « aujourd’hui », « demain », « vendredi 20 nov. ».
function jourTexte(d: Date, maintenant: Date): string {
  const k = Math.round((new Date(d).setHours(0, 0, 0, 0) - new Date(maintenant).setHours(0, 0, 0, 0)) / 86_400_000);
  if (k === 0) return "aujourd’hui";
  if (k === 1) return "demain";
  return `${JOURS[jourSemaine(d)]} ${d.getDate()} ${MOIS[d.getMonth()]}`;
}
const leTexte = (d: Date, maintenant: Date) => `${jourTexte(d, maintenant)} à ${p2(d.getHours())}:${p2(d.getMinutes())}`;

// « dans 12 min », « dans 3 h 05 », « dans 2 j ».
function dans(d: Date, maintenant: Date): string {
  const m = Math.max(1, Math.round((d.getTime() - maintenant.getTime()) / 60000));
  if (m < 60) return `dans ${m} min`;
  if (m < 1440) return `dans ${Math.floor(m / 60)} h ${p2(m % 60)}`;
  return `dans ${Math.round(m / 1440)} j`;
}

// « Le lundi, mercredi et vendredi ».
function joursTexte(jours: number[]): string {
  const n = [...jours].sort().map((i) => JOURS[i]);
  if (n.length === 7) return "Tous les jours";
  if (n.length === 1) return `Le ${n[0]}`;
  return `Le ${n.slice(0, -1).join(", ")} et ${n[n.length - 1]}`;
}

// « Tous les jours à 05:00 », « Toutes les 6 h », « Une seule fois, demain à 21:00 ».
function planning(t: Automatisation, maintenant: Date): string {
  if (t.when === "daily") return `Tous les jours à ${t.time}`;
  if (t.when === "days") return `${joursTexte(t.days)} à ${t.time}`;
  if (t.when === "every") return `Toutes les ${t.hours} h`;
  const d = dateDe(t);
  return d ? `Une seule fois, ${leTexte(d, maintenant)}` : "Une seule fois";
}

// « Si personne ne joue depuis 30 min ».
function declencheurTexte(t: Automatisation): string {
  if (t.trig === "empty") return `Si personne ne joue depuis ${t.mins} min`;
  if (t.trig === "join") return t.first ? "Quand un joueur arrive pour la 1re fois" : "Quand un joueur arrive";
  return DECLENCHEURS[t.trig].nom;
}

function titre(t: Automatisation): string {
  if (t.act === "say") return `Annonce : « ${t.msg || "…"} »`;
  if (t.act === "cmd") return `Commande : /${t.cmd || "…"}`;
  return ACTIONS[t.act];
}

const previent = (t: Automatisation) => t.trig === "time" && (t.act === "stop" || t.act === "restart");

const VIDE: Automatisation = {
  id: "",
  on: true,
  trig: "time",
  act: "restart",
  when: "daily",
  time: "05:00",
  days: [4],
  date: "",
  hours: 2,
  warn: true,
  warnMin: 5,
  msg: "",
  cmd: "",
  mins: 30,
  first: true,
  motd: "",
};

// L’onglet Automatisations : ce que BHOST fait tout seul pour ce serveur.
// Une liste calme, comme les alarmes de l’app Horloge : l’heure en gros, la phrase en dessous.
export default function Automatisations({ serveur, onErreur, onInfo }: Props) {
  const [vue, setVue] = useState<VueAutomatisations | null>(null);
  const [edition, setEdition] = useState<Automatisation | null>(null);
  const [maintenant, setMaintenant] = useState(() => new Date());

  useEffect(() => {
    const lire = () => automatisationsServeur(serveur.id).then(setVue, (e) => onErreur(phrase(e)));
    lire();
    return ecouterAutomatisations((id) => id === serveur.id && lire());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id]);

  // Les « dans 12 min » avancent tout seuls.
  useEffect(() => {
    const minuteur = window.setInterval(() => setMaintenant(new Date()), 20_000);
    return () => window.clearInterval(minuteur);
  }, []);

  const liste = vue?.liste ?? [];
  const aHeure = liste.filter((t) => t.trig === "time");
  const evenements = liste.filter((t) => t.trig !== "time");
  const suivante = aHeure
    .filter((t) => t.on)
    .map((t) => ({ t, d: prochaine(t, maintenant) }))
    .filter((x): x is { t: Automatisation; d: Date } => x.d !== null)
    .sort((x, y) => x.d.getTime() - y.d.getTime())[0];

  const basculer = (t: Automatisation) => activerAutomatisation(serveur.id, t.id, !t.on).then(setVue, (e) => onErreur(phrase(e)));
  const ouvrir = (t: Automatisation) => setEdition({ ...t, days: [...t.days] });

  const ligne = (t: Automatisation) => {
    let gros: string;
    let petit: string;
    let droite: string;
    if (t.trig === "time") {
      const d = prochaine(t, maintenant);
      const jour = dateDe(t);
      const quandTexte = t.when === "daily" ? "Chaque jour" : t.when === "days" ? joursTexte(t.days) : t.when === "once" ? (jour ? majuscule(jourTexte(jour, maintenant)) : "Une fois") : "";
      gros = t.when === "every" ? `Toutes les ${t.hours} h` : t.time;
      petit = (quandTexte ? `${quandTexte} · ` : "") + titre(t) + (previent(t) && t.warn ? ` · prévient ${t.warnMin} min avant` : "");
      droite = !t.on ? "En pause" : d ? dans(d, maintenant) : "Terminée";
    } else {
      gros = declencheurTexte(t);
      petit = titre(t);
      droite = t.on ? "" : "En pause";
    }
    const heure = t.trig === "time" && t.when !== "every";
    return (
      <div className={"au-row" + (t.on ? "" : " off")} key={t.id}>
        <button type="button" className="au-hit" onClick={() => ouvrir(t)}>
          <span className="au-t">
            <span className={"au-big" + (heure ? "" : " sm")}>{gros}</span>
            <span className="au-sub">{petit}</span>
          </span>
          {droite && <span className="au-nx tnum">{droite}</span>}
        </button>
        <button type="button" role="switch" aria-checked={t.on} aria-label={(t.on ? "Mettre en pause : " : "Activer : ") + titre(t)} className={"sw" + (t.on ? " on" : "")} onClick={() => basculer(t)}>
          <i />
        </button>
      </div>
    );
  };

  return (
    <div className="params au">
      <div className="au-top">
        <p className="au-resume">{vue === null ? "" : suivante ? `Prochaine : ${minuscule(titre(suivante.t))}, ${leTexte(suivante.d, maintenant)}.` : "Rien de prévu."}</p>
        <button type="button" className="btn acc" onClick={() => setEdition({ ...VIDE, days: [...VIDE.days] })}>
          <IconPlus size={14} stroke={2} aria-hidden />
          Nouvelle
        </button>
      </div>

      <div className="grp-t">À heure fixe</div>
      <section className="surf">
        {vue === null ? <div className="empty">Lecture…</div> : aHeure.length ? aHeure.map(ligne) : <div className="empty">Rien à heure fixe.</div>}
      </section>

      <div className="grp-t">Quand il se passe quelque chose</div>
      <section className="surf">{vue === null ? <div className="empty">Lecture…</div> : evenements.length ? evenements.map(ligne) : <div className="empty">Rien pour l’instant.</div>}</section>

      <div className="grp-t">Dernières actions</div>
      <section className="surf">
        {vue && vue.historique.length ? (
          vue.historique.slice(0, 8).map((h, i) => (
            <div className="au-h" key={h.quand + "-" + i}>
              <span className="au-hw">{h.quoi}</span>
              <span className="au-hq tnum">{quand(h.quand)}</span>
            </div>
          ))
        ) : (
          <div className="empty">Rien pour l’instant.</div>
        )}
      </section>

      <p className="au-foot">{serveur.machine ? "Marche tant que la machine est allumée." : "Marche tant que ton PC est allumé et BHOST ouvert."}</p>

      {edition && (
        <Edition
          serveur={serveur}
          depart={edition}
          tpsPossible={vue?.tpsPossible ?? false}
          majPossible={vue?.majPossible ?? false}
          onFermer={() => setEdition(null)}
          onFini={(v, texte) => {
            setVue(v);
            setEdition(null);
            onInfo(texte);
          }}
        />
      )}
    </div>
  );
}

type PropsEdition = {
  serveur: Serveur;
  depart: Automatisation;
  tpsPossible: boolean;
  majPossible: boolean;
  onFermer: () => void;
  onFini: (v: VueAutomatisations, texte: string) => void;
};

// « Nouvelle automatisation » : deux blocs, Quand et Alors, avec des menus façon Réglages de macOS.
// Une phrase en haut résume ce qui est réglé.
function Edition({ serveur, depart, tpsPossible, majPossible, onFermer, onFini }: PropsEdition) {
  const [t, setT] = useState<Automatisation>(depart);
  const [envoi, setEnvoi] = useState(false);
  const [erreur, setErreur] = useState("");
  const nouvelle = !depart.id;
  const maintenant = new Date();

  const changer = (patch: Partial<Automatisation>) =>
    setT((x) => {
      const n = { ...x, ...patch };
      // L’action doit aller avec le déclencheur : sinon on prend la première permise.
      if (!DECLENCHEURS[n.trig].actions.includes(n.act)) n.act = DECLENCHEURS[n.trig].actions[0];
      // « Une fois » sans jour : demain.
      if (n.when === "once" && !n.date) n.date = dateTexte(a(1, "00:00", new Date()));
      return n;
    });

  const D = DECLENCHEURS[t.trig];
  const declencheurs = ORDRE.filter((k) => (k !== "lag" || tpsPossible || t.trig === "lag") && (k !== "upd" || majPossible || t.trig === "upd")).map((k) => ({
    id: k,
    nom: DECLENCHEURS[k].nom,
  }));
  // « plugins », « mods », ou « mods et plugins » (Mohist, Youer, Arclight).
  const quoi = nomExtensions(serveur.logiciel).toLowerCase().replace(" & ", " et ");
  const actions = D.actions.map((k) => ({ id: k, nom: ACTIONS[k] }));
  const nomJour = (d: Date) => `${majuscule(JOURS[jourSemaine(d)])} ${d.getDate()} ${MOIS[d.getMonth()]}`;
  const jours = [0, 1, 2, 3, 4, 5, 6].map((k) => {
    const d = a(k, "00:00", maintenant);
    return { id: dateTexte(d), nom: k === 0 ? "Aujourd’hui" : k === 1 ? "Demain" : nomJour(d) };
  });
  // Une date plus lointaine (quand on modifie) reste dans le menu.
  const choisie = dateDe(t);
  if (t.when === "once" && choisie && !jours.some((j) => j.id === t.date)) jours.push({ id: t.date, nom: nomJour(choisie) });

  const resume = `${t.trig === "time" ? planning(t, maintenant) : declencheurTexte(t)} : ${minuscule(titre(t))}.`;
  const possible = (t.act !== "say" || t.msg.trim() !== "") && (t.act !== "cmd" || t.cmd.trim() !== "") && (t.trig !== "time" || t.when !== "days" || t.days.length > 0);

  const enregistrer = () => {
    if (!possible) return;
    setEnvoi(true);
    setErreur("");
    enregistrerAutomatisation(serveur.id, t).then(
      (v) => onFini(v, nouvelle ? "Automatisation ajoutée" : "Automatisation modifiée"),
      (e) => {
        setErreur(phrase(e));
        setEnvoi(false);
      },
    );
  };

  const supprimer = () => {
    setEnvoi(true);
    supprimerAutomatisation(serveur.id, t.id).then(
      (v) => onFini(v, "Automatisation supprimée"),
      (e) => {
        setErreur(phrase(e));
        setEnvoi(false);
      },
    );
  };

  return (
    <Fenetre titre={nouvelle ? "Nouvelle automatisation" : "Modifier l’automatisation"} occupe={envoi} classe="au-dlg" onFermer={onFermer}>
      <h4>{nouvelle ? "Nouvelle automatisation" : "Modifier l’automatisation"}</h4>
      <p className="au-phrase">{resume}</p>

      <div className="au-sec">Quand</div>
      <div className="au-box">
        <div className="au-r">
          <span>Déclencheur</span>
          <Choix etiquette="Déclencheur" options={declencheurs} valeur={t.trig} onChoisir={(trig) => changer({ trig })} />
        </div>
        {t.trig === "time" && (
          <>
            <div className="au-r">
              <span>Répéter</span>
              <Choix etiquette="Répéter" options={REPETER} valeur={t.when} onChoisir={(when) => changer({ when })} />
            </div>
            {t.when === "once" && (
              <div className="au-r">
                <span>Jour</span>
                <Choix etiquette="Jour" options={jours} valeur={t.date} onChoisir={(date) => changer({ date })} />
              </div>
            )}
            {t.when === "days" && (
              <div className="au-r">
                <span>Jours</span>
                <div className="au-days">
                  {LETTRES.map((l, i) => {
                    const on = t.days.includes(i);
                    return (
                      <button
                        key={i}
                        type="button"
                        aria-pressed={on}
                        aria-label={JOURS[i]}
                        className={"au-day" + (on ? " sel" : "")}
                        onClick={() => changer({ days: on ? t.days.filter((x) => x !== i) : [...t.days, i] })}
                      >
                        {l}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
            {t.when === "every" ? (
              <div className="au-r">
                <span>Toutes les</span>
                <Compteur valeur={t.hours} min={1} max={24} pas={1} unite={(n) => `${n} h`} nom="Toutes les combien d’heures" onChoisir={(hours) => changer({ hours })} />
              </div>
            ) : (
              <div className="au-r">
                <span>Heure</span>
                <input className="field au-time tnum" type="time" aria-label="Heure" value={t.time} onChange={(e) => changer({ time: e.target.value || "00:00" })} />
              </div>
            )}
          </>
        )}
        {t.trig === "empty" && (
          <div className="au-r">
            <span>Personne depuis</span>
            <Compteur valeur={t.mins} min={5} max={240} pas={5} unite={(n) => `${n} min`} nom="Minutes sans personne" onChoisir={(mins) => changer({ mins })} />
          </div>
        )}
        {t.trig === "join" && (
          <div className="au-r">
            <span id="au-first">Première venue seulement</span>
            <button type="button" role="switch" aria-checked={t.first} aria-labelledby="au-first" className={"sw" + (t.first ? " on" : "")} onClick={() => changer({ first: !t.first })}>
              <i />
            </button>
          </div>
        )}
        {t.trig === "wake" && (
          <div className="au-r col">
            <span>Message dans la liste du jeu</span>
            <input
              className="field"
              type="text"
              maxLength={120}
              placeholder="Le serveur dort. Rejoins pour le réveiller !"
              aria-label="Message pendant la veille"
              value={t.motd}
              onChange={(e) => changer({ motd: e.target.value })}
            />
          </div>
        )}
      </div>
      {D.aide && <p className="au-hint">{t.trig === "upd" ? `BHOST regarde les ${quoi} toutes les 6 h.` : D.aide}</p>}

      <div className="au-sec">Alors</div>
      <div className="au-box">
        <div className="au-r">
          <span>Action</span>
          <Choix etiquette="Action" options={actions} valeur={t.act} onChoisir={(act) => changer({ act })} />
        </div>
        {t.act === "say" && (
          <div className="au-r col">
            <span>Message</span>
            <input
              className="field"
              type="text"
              maxLength={200}
              placeholder={t.trig === "join" ? "Bienvenue {joueur} !" : "Ton message"}
              aria-label="Message"
              value={t.msg}
              onChange={(e) => changer({ msg: e.target.value })}
            />
          </div>
        )}
        {t.act === "cmd" && (
          <div className="au-r col">
            <span>Commande</span>
            <input
              className="field mono"
              type="text"
              maxLength={250}
              placeholder="gamerule keepInventory true"
              aria-label="Commande"
              value={t.cmd}
              onChange={(e) => changer({ cmd: e.target.value.replace(/^\//, "") })}
            />
          </div>
        )}
        {t.act === "notify" && <p className="au-note">Une notification Windows.</p>}
        {t.act === "update" && <p className="au-note">Serveur allumé : c’est fait à son prochain arrêt.</p>}
        {previent(t) && (
          <div className="au-r">
            <span id="au-warn">Prévenir les joueurs</span>
            <button type="button" role="switch" aria-checked={t.warn} aria-labelledby="au-warn" className={"sw" + (t.warn ? " on" : "")} onClick={() => changer({ warn: !t.warn })}>
              <i />
            </button>
          </div>
        )}
        {previent(t) && t.warn && (
          <div className="au-r">
            <span>Combien de temps avant</span>
            <Choix etiquette="Combien de temps avant" options={AVANT} valeur={String(t.warnMin) as "1" | "5" | "15"} onChoisir={(m) => changer({ warnMin: Number(m) })} />
          </div>
        )}
      </div>

      {erreur && (
        <p className="dlg-err au-err" role="alert">
          {erreur}
        </p>
      )}

      <div className="dlg-btns au-btns">
        {!nouvelle && (
          <button type="button" className="au-del" disabled={envoi} onClick={supprimer}>
            Supprimer
          </button>
        )}
        <button type="button" className="dlg-cancel" disabled={envoi} onClick={onFermer}>
          Annuler
        </button>
        <button type="button" className="dlg-ok" disabled={!possible || envoi} onClick={enregistrer}>
          Enregistrer
        </button>
      </div>
    </Fenetre>
  );
}
