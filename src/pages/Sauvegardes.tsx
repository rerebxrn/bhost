import { useEffect, useState } from "react";
import {
  IconAlertTriangle,
  IconArrowBackUp,
  IconCircleCheck,
  IconClock,
  IconDeviceFloppy,
  IconDownload,
  IconFolder,
  IconPin,
  IconRefresh,
  IconTrash,
  IconWorld,
} from "@tabler/icons-react";
import Segment from "../components/Segment";
import Fenetre, { Avancement, poids, useTravail } from "../components/Fenetre";
import {
  choisirEnregistrement,
  epinglerSauvegarde,
  exporterSauvegarde,
  ouvrirSauvegardes,
  reglerSauvegardes,
  restaurerSauvegarde,
  sauvegarderMaintenant,
  sauvegardesServeur,
  supprimerSauvegarde,
  type ReglagesSauvegarde,
  type Sauvegarde,
  type Sauvegardes as Vue,
} from "../api";
import { nombre, type Serveur } from "../types";
import "./Sauvegardes.css";

type Props = {
  serveur: Serveur;
  onChange: (s: Serveur) => void;
  onErreur: (texte: string) => void;
  onInfo: (texte: string) => void;
};

const GENRES: Record<string, { nom: string; Icone: typeof IconClock }> = {
  auto: { nom: "Automatique", Icone: IconClock },
  manuelle: { nom: "Manuelle", Icone: IconDeviceFloppy },
  maj: { nom: "Avant mise à jour", Icone: IconRefresh },
  restauration: { nom: "Avant restauration", Icone: IconArrowBackUp },
  reinit: { nom: "Avant réinitialisation", Icone: IconWorld },
};

const FREQUENCES = [
  { id: "1h" as const, nom: "Toutes les heures" },
  { id: "6h" as const, nom: "Toutes les 6 h" },
  { id: "24h" as const, nom: "Chaque jour" },
];

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n'a pas marché.");

// « Aujourd’hui à 06:00 », « Hier à 21:47 », « Dim. 15 nov. à 14:12 ».
export function quand(ms: number, maintenant = Date.now()): string {
  const d = new Date(ms);
  const heure = d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  const jour = (t: number) => new Date(t).setHours(0, 0, 0, 0);
  const ecart = Math.round((jour(maintenant) - jour(ms)) / 86_400_000);
  if (ecart <= 0) return `Aujourd’hui à ${heure}`;
  if (ecart === 1) return `Hier à ${heure}`;
  const date = d.toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" });
  return `${date.charAt(0).toUpperCase()}${date.slice(1)} à ${heure}`;
}

// « 3 h 12 », « 38 min ».
function dans(ms: number): string {
  const minutes = Math.max(1, Math.round(ms / 60000));
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, "0")}`;
}

// L'onglet Sauvegardes : sauvegarde automatique, historique, restauration.
export default function Sauvegardes({ serveur, onChange, onErreur, onInfo }: Props) {
  const [vue, setVue] = useState<Vue | null>(null);
  const [envoi, setEnvoi] = useState(false);
  const [aRestaurer, setARestaurer] = useState<Sauvegarde | null>(null);
  const travail = useTravail(serveur.id, envoi);
  const allume = serveur.statut !== "arrete" && serveur.statut !== "plante";

  useEffect(() => {
    sauvegardesServeur(serveur.id).then(setVue, (e) => onErreur(phrase(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id, serveur.statut]);

  const regler = (change: Partial<ReglagesSauvegarde>) => {
    if (!vue) return;
    const reglages = { ...vue.reglages, ...change };
    setVue({ ...vue, reglages });
    reglerSauvegardes(serveur.id, reglages).then(setVue, (e) => onErreur(phrase(e)));
  };

  const maintenant = () => {
    setEnvoi(true);
    sauvegarderMaintenant(serveur.id)
      .then((v) => {
        setVue(v);
        onInfo("Sauvegarde terminée");
      }, (e) => onErreur(phrase(e)))
      .finally(() => setEnvoi(false));
  };

  const exporter = async (s: Sauvegarde) => {
    const cible = await choisirEnregistrement(`${serveur.nom} ${s.fichier}`);
    if (!cible) return;
    exporterSauvegarde(serveur.id, s.fichier, cible).then(() => onInfo("Sauvegarde exportée"), (e) => onErreur(phrase(e)));
  };

  const liste = vue?.liste ?? [];
  const total = liste.reduce((a, s) => a + s.taille, 0);
  const epinglees = liste.filter((s) => s.epinglee).length;
  const r = vue?.reglages;

  return (
    <div className="params bks">
      <div className="bk-top">
        <span className="t2">
          {liste.length
            ? `${liste.length} sauvegarde${liste.length > 1 ? "s" : ""} · ${poids(total)} · ${nombre(vue?.libreGo ?? 0)} Go libres`
            : "Aucune sauvegarde pour l’instant"}
        </span>
        <div className="bk-btns">
          <button type="button" className="btn" onClick={() => ouvrirSauvegardes(serveur.id).catch((e) => onErreur(phrase(e)))}>
            <IconFolder size={14} stroke={2} aria-hidden />
            Ouvrir le dossier
          </button>
          <button type="button" className="btn acc" disabled={envoi} onClick={maintenant}>
            <IconDeviceFloppy size={14} stroke={2} aria-hidden />
            Sauvegarder maintenant
          </button>
        </div>
      </div>

      {envoi && (
        <section className="surf bk-prog">
          <Avancement texte={travail?.etape ?? "Préparation"} pourcent={travail?.pourcent ?? null} />
        </section>
      )}

      <div className="grp-t">Automatique</div>
      <section className="surf">
        <div className="row">
          <div className="lbl">
            <span id="bk-auto">Sauvegarder automatiquement</span>
            <span className="t2">
              {r?.auto ? (vue?.prochaine ? `Prochaine dans ${dans(vue.prochaine - Date.now())}` : "Quand le serveur tourne.") : "Coupé."}
            </span>
          </div>
          <button type="button" role="switch" aria-checked={!!r?.auto} aria-labelledby="bk-auto" className={"sw" + (r?.auto ? " on" : "")} onClick={() => regler({ auto: !r?.auto })}>
            <i />
          </button>
        </div>
        <div className={"row" + (r?.auto ? "" : " off")}>
          <span>Fréquence</span>
          <Segment etiquette="Fréquence" options={FREQUENCES} valeur={r?.frequence ?? "6h"} off={!r?.auto} onChoisir={(frequence) => regler({ frequence })} />
        </div>
        <div className={"row" + (r?.auto ? "" : " off")}>
          <span>Garder</span>
          <div className="stepper">
            <button type="button" aria-label="Moins" disabled={!r?.auto} onClick={() => r && regler({ garder: Math.max(2, r.garder - 1) })}>
              −
            </button>
            <span className="tnum bk-garder">
              <b key={r?.garder}>{r?.garder ?? 10} dernières</b>
            </span>
            <button type="button" aria-label="Plus" disabled={!r?.auto} onClick={() => r && regler({ garder: Math.min(50, r.garder + 1) })}>
              +
            </button>
          </div>
        </div>
        <div className="row">
          <span id="bk-maj">Avant chaque changement de version</span>
          <button type="button" role="switch" aria-checked={!!r?.avantMaj} aria-labelledby="bk-maj" className={"sw" + (r?.avantMaj ? " on" : "")} onClick={() => regler({ avantMaj: !r?.avantMaj })}>
            <i />
          </button>
        </div>
      </section>

      <div className="grp-t">
        Historique{epinglees ? ` · ${epinglees} épinglée${epinglees > 1 ? "s" : ""}` : ""}
      </div>
      <section className="surf">
        {vue === null ? (
          <div className="empty">Lecture…</div>
        ) : liste.length === 0 ? (
          <div className="empty">Aucune sauvegarde pour l’instant.</div>
        ) : (
          liste.map((s) => {
            const g = GENRES[s.genre] ?? GENRES.manuelle;
            return (
              <div className="bk" key={s.fichier}>
                <span className={"bk-ic k-" + s.genre}>
                  <g.Icone size={16} stroke={2} aria-hidden />
                </span>
                <div className="mod-txt">
                  <b>
                    {quand(s.quand)}
                    {s.etiquette && <span className="bk-lab">{s.etiquette}</span>}
                  </b>
                  <span>{[g.nom, s.quoi === "monde" ? "Le monde" : "Tout le serveur", poids(s.taille)].join(" · ")}</span>
                </div>
                <button
                  type="button"
                  className={"icb pin" + (s.epinglee ? " on" : "")}
                  aria-pressed={s.epinglee}
                  aria-label={s.epinglee ? "Ne plus épingler" : "Épingler"}
                  title={s.epinglee ? "Ne plus épingler" : "Épingler (jamais effacée)"}
                  onClick={() => epinglerSauvegarde(serveur.id, s.fichier, !s.epinglee).then(setVue, (e) => onErreur(phrase(e)))}
                >
                  <IconPin size={15} stroke={2} aria-hidden />
                </button>
                <button type="button" className="icb" aria-label="Exporter en .zip" title="Exporter en .zip" onClick={() => exporter(s)}>
                  <IconDownload size={15} stroke={2} aria-hidden />
                </button>
                <button
                  type="button"
                  className="icb del"
                  aria-label="Supprimer"
                  title="Mettre à la Corbeille"
                  onClick={() =>
                    supprimerSauvegarde(serveur.id, s.fichier).then(
                      (v) => {
                        setVue(v);
                        onInfo("Sauvegarde mise à la Corbeille");
                      },
                      (e) => onErreur(phrase(e)),
                    )
                  }
                >
                  <IconTrash size={15} stroke={2} aria-hidden />
                </button>
                <button type="button" className="btn" onClick={() => setARestaurer(s)}>
                  Restaurer…
                </button>
              </div>
            );
          })
        )}
      </section>

      {aRestaurer && (
        <Restaurer
          serveur={serveur}
          sauvegarde={aRestaurer}
          allume={allume}
          onFermer={() => setARestaurer(null)}
          onFini={(s) => {
            setARestaurer(null);
            onChange(s);
            onInfo("C’est fait : tu peux redémarrer");
            sauvegardesServeur(serveur.id).then(setVue, () => {});
          }}
        />
      )}
    </div>
  );
}

type PropsRestaurer = {
  serveur: Serveur;
  sauvegarde: Sauvegarde;
  allume: boolean;
  onFermer: () => void;
  onFini: (s: Serveur) => void;
};

// « Restaurer… » : tout le serveur, ou seulement le monde. L'état actuel est sauvegardé avant.
function Restaurer({ serveur, sauvegarde, allume, onFermer, onFini }: PropsRestaurer) {
  const toutPossible = sauvegarde.quoi !== "monde";
  const [tout, setTout] = useState(toutPossible);
  const [envoi, setEnvoi] = useState(false);
  const [erreur, setErreur] = useState("");
  const travail = useTravail(serveur.id, envoi);
  const titre = quand(sauvegarde.quand).replace(/^Aujourd’hui/, "aujourd’hui").replace(/^Hier/, "hier");
  const options = [
    ...(toutPossible ? [{ tout: true, nom: "Tout le serveur", desc: "Monde, plugins, mods et réglages." }] : []),
    { tout: false, nom: "Seulement le monde", desc: "Garde tes plugins, mods et réglages actuels." },
  ];

  const valider = () => {
    setEnvoi(true);
    setErreur("");
    restaurerSauvegarde(serveur.id, sauvegarde.fichier, tout)
      .then(onFini)
      .catch((e) => {
        setErreur(phrase(e));
        setEnvoi(false);
      });
  };

  return (
    <Fenetre titre="Restaurer une sauvegarde" occupe={envoi} onFermer={onFermer}>
      <span className="dlg-ic">
        <IconArrowBackUp size={26} stroke={1.8} aria-hidden />
      </span>
      <h4>Revenir à « {titre} » ?</h4>
      <p className="dlg-txt">Ce qui a été fait depuis sera perdu.</p>
      {envoi ? (
        <Avancement texte={travail?.etape ?? "Préparation"} pourcent={travail?.pourcent ?? null} />
      ) : (
        <>
          <div className="rs-opts" role="radiogroup" aria-label="Quoi restaurer">
            {options.map((o) => (
              <button key={o.nom} type="button" role="radio" aria-checked={o.tout === tout} className={"rs-opt" + (o.tout === tout ? " sel" : "")} onClick={() => setTout(o.tout)}>
                <span className="radio" />
                <span className="rs-t">
                  <b>{o.nom}</b>
                  <span>{o.desc}</span>
                </span>
              </button>
            ))}
          </div>
          {allume ? (
            <div className="v-warn red rs-warn">
              <IconAlertTriangle size={15} stroke={2} aria-hidden />
              <span>Arrête le serveur d’abord.</span>
            </div>
          ) : (
            <div className="rs-safe">
              <IconCircleCheck size={15} stroke={2} aria-hidden />
              <span>Sauvegarde de l’état actuel avant.</span>
            </div>
          )}
        </>
      )}
      {erreur && (
        <p className="dlg-err" role="alert">
          {erreur}
        </p>
      )}
      <div className="dlg-btns">
        <button type="button" className="dlg-cancel" onClick={onFermer} disabled={envoi}>
          Annuler
        </button>
        <button type="button" className="dlg-ok" disabled={envoi || allume} onClick={valider}>
          Restaurer
        </button>
      </div>
    </Fenetre>
  );
}
