import { useEffect, useState } from "react";
import { IconCopy, IconEye, IconEyeOff } from "@tabler/icons-react";
import IconeServeur, { couleurDe } from "../components/IconeServeur";
import { afficherCode } from "../code";
import Confirmer from "../components/Confirmer";
import { changerCodeMachine, etatSysteme, systemeMachine, type EtatSysteme } from "../api";
import { nombre, type Machine as MachineT, type Serveur } from "../types";
import "./Reglages.css";
import "./Machine.css";

type Props = {
  machine: MachineT;
  serveurs: Serveur[]; // ceux de cette machine
  onRenommer: () => void;
  onRetirer: () => void;
  onCopie: (texte: string) => void;
  onMessage: (texte: string) => void;
};

type Question = "code" | "redemarrer" | "eteindre" | "mettre-a-jour" | null;

// « il y a 3 h » (dernière recherche de mises à jour).
function ilYa(secondes?: number) {
  if (!secondes) return "";
  const m = Math.max(0, Math.round((Date.now() / 1000 - secondes) / 60));
  if (m < 2) return "à l’instant";
  if (m < 60) return `il y a ${m} min`;
  if (m < 48 * 60) return `il y a ${Math.round(m / 60)} h`;
  return `il y a ${Math.round(m / 1440)} jours`;
}

const pct = (a: number, b: number) => (b > 0 ? Math.min(100, Math.round((a / b) * 100)) : 0);
const classe = (p: number) => (p >= 90 ? " plein" : p >= 75 ? " haut" : "");

function Barre({ p, label }: { p: number; label: string }) {
  return (
    <span className={"sm-barre" + classe(p)} role="meter" aria-label={label} aria-valuenow={p} aria-valuemin={0} aria-valuemax={100}>
      <i style={{ width: `${p}%` }} />
    </span>
  );
}

// La page « Machine » d'une machine dédiée (clic sur son état, en haut de la barre de gauche).
export default function Machine({ machine: m, serveurs, onRenommer, onRetirer, onCopie, onMessage }: Props) {
  const [codeVu, setCodeVu] = useState(false);
  const [question, setQuestion] = useState<Question>(null);
  const [systeme, setSysteme] = useState<EtatSysteme | null>(null);
  const enLigne = m.etat === "en-ligne";

  // L'état du système : relu toutes les 5 s (plus souvent rien ne change ; pendant une recherche, il bouge).
  useEffect(() => {
    if (!enLigne) return;
    let fini = false;
    const lire = () => etatSysteme(m.id).then((e) => !fini && setSysteme(e), () => {});
    lire();
    const minuterie = window.setInterval(lire, 5000);
    return () => {
      fini = true;
      window.clearInterval(minuterie);
    };
  }, [m.id, enLigne]);

  const demander = (action: "redemarrer" | "eteindre" | "verifier" | "mettre-a-jour") =>
    systemeMachine(m.id, action).then(
      () => {
        if (action === "redemarrer") onMessage("Redémarrage… La machine revient dans une minute.");
        else if (action === "eteindre") onMessage("La machine s’éteint.");
        else setSysteme((e) => (e ? { ...e, enCours: action === "verifier" ? "verification" : "installation" } : e));
      },
      (e) => onMessage(typeof e === "string" ? e : "Ça n’a pas marché."),
    );
  const changerCode = () =>
    changerCodeMachine(m.id).then(
      () => {
        setCodeVu(true);
        onMessage("Nouveau code. Les autres appareils devront l’entrer.");
      },
      (e) => onMessage(typeof e === "string" ? e : "Ça n’a pas marché."),
    );

  const enCours = systeme?.enCours;
  const attente = systeme?.attente ?? 0;
  const etatMaj = !systeme
    ? "–"
    : enCours === "verification"
      ? "Recherche…"
      : enCours === "installation"
        ? "Installation…"
        : systeme.redemarrage
          ? "Redémarre pour finir la mise à jour."
          : attente > 0
            ? `${attente} mise${attente > 1 ? "s" : ""} à jour${systeme.verifie ? ` · ${ilYa(systeme.verifie)}` : ""}`
            : `À jour${systeme.verifie ? ` · ${ilYa(systeme.verifie)}` : ""}`;
  const t = m.temperature ?? null;
  return (
    <div className="page etroite reglages machine">
      <header className="head">
        <h1>{m.nom}</h1>
        <p className="ma-etat">
          <i className={"sm-point " + m.etat} aria-hidden />
          {enLigne ? "En ligne" : "Éteinte ou sans Internet"}
          {enLigne && m.ip && <span className="t2"> · {m.ip}</span>}
        </p>
      </header>

      <div className="params">
        {enLigne && m.disque && m.memoire && (
          <section className="surf strip ma-strip" aria-label="État de la machine">
            <div className="cell cell-disque">
              <div>
                <div className="k">Stockage</div>
                <div className="v">
                  {nombre(m.disque.utiliseGo)}
                  <small>sur {nombre(m.disque.totalGo)} Go</small>
                </div>
              </div>
              <Barre p={pct(m.disque.utiliseGo, m.disque.totalGo)} label="Stockage" />
            </div>
            <div className="cell cell-disque">
              <div>
                <div className="k">Mémoire réservée</div>
                <div className="v">
                  {nombre(m.memoire.reserveeGo)}
                  <small>sur {nombre(m.memoire.totalGo)} Go</small>
                </div>
              </div>
              <Barre p={pct(m.memoire.reserveeGo, m.memoire.totalGo)} label="Mémoire réservée" />
            </div>
            <div className="cell">
              <div>
                <div className="k">Processeur</div>
                <div className="v">
                  {nombre(m.processeur ?? 0)}
                  <small>%</small>
                </div>
              </div>
            </div>
            <div className="cell">
              <div>
                <div className="k">Température</div>
                <div className={"v" + (t !== null && t >= 80 ? " ma-chaud" : t !== null && t >= 70 ? " ma-tiede" : "")}>
                  {t === null ? "–" : nombre(t)}
                  <small>°C</small>
                </div>
              </div>
            </div>
          </section>
        )}

        <div className="grp-t">Serveurs</div>
        <section className="surf">
          {serveurs.map((s) => {
            const p = s.disqueMaxMo ? pct(s.disqueMo ?? 0, s.disqueMaxMo) : 0;
            const allume = s.statut !== "arrete" && s.statut !== "plante";
            return (
              <div className="row ma-srv" key={s.id}>
                <IconeServeur couleur={couleurDe(s)} taille={28} />
                <div className="lbl">
                  <span>{s.nom}</span>
                  <span className="t2">
                    {nombre(s.ramMo / 1024)} Go de mémoire · {allume ? "allumé" : "éteint"}
                  </span>
                </div>
                {s.disqueMaxMo ? (
                  <div className="ma-disque">
                    <span className="t2 tnum">
                      {nombre((s.disqueMo ?? 0) / 1024)} sur {nombre(s.disqueMaxMo / 1024)} Go
                    </span>
                    <Barre p={p} label={`Stockage de ${s.nom}`} />
                  </div>
                ) : null}
              </div>
            );
          })}
          {serveurs.length === 0 && (
            <div className="row">
              <span className="t2">Aucun serveur.</span>
            </div>
          )}
        </section>

        <div className="grp-t">Machine</div>
        <section className="surf">
          <div className="row">
            <div className="lbl">
              <span>Nom</span>
              <span className="t2">{m.nom}</span>
            </div>
            <button type="button" className="btn" onClick={onRenommer}>
              Renommer…
            </button>
          </div>
          {m.ip && (
            <div className="row">
              <div className="lbl">
                <span>À la maison</span>
                <span className="t2 tnum">{m.ip}</span>
              </div>
              <button type="button" className="btn" onClick={() => onCopie(m.ip!)}>
                <IconCopy size={14} stroke={2} aria-hidden />
                Copier
              </button>
            </div>
          )}
          <div className="row">
            <div className="lbl">
              <span>Code</span>
              <span className="t2">Il donne accès à tes serveurs.</span>
            </div>
            <div className="rg-btns">
              <button type="button" className="btn rg-code" aria-label={codeVu ? "Cacher le code" : "Afficher le code"} onClick={() => setCodeVu((v) => !v)}>
                {codeVu ? <IconEyeOff size={14} stroke={2} aria-hidden /> : <IconEye size={14} stroke={2} aria-hidden />}
                <span className="tnum">{codeVu ? afficherCode(m.code) : "••••-••••"}</span>
              </button>
              <button type="button" className="btn" disabled={!enLigne} onClick={() => setQuestion("code")}>
                Changer…
              </button>
            </div>
          </div>
          <div className="row">
            <div className="lbl">
              <span>BHOST OS{systeme?.version ? ` ${systeme.version}` : ""}</span>
              <span className="t2">{enLigne ? etatMaj : "–"}</span>
            </div>
            {attente > 0 && !enCours ? (
              <button type="button" className="btn" disabled={!enLigne} onClick={() => setQuestion("mettre-a-jour")}>
                Mettre à jour…
              </button>
            ) : (
              <button type="button" className="btn" disabled={!enLigne || !!enCours} onClick={() => demander("verifier")}>
                Rechercher
              </button>
            )}
          </div>
        </section>

        <div className="grp-t">Alimentation</div>
        <section className="surf">
          <div className="row">
            <div className="lbl">
              <span>Redémarrer</span>
              <span className="t2">Les serveurs s’arrêtent proprement, puis repartent.</span>
            </div>
            <button type="button" className="btn" disabled={!enLigne} onClick={() => setQuestion("redemarrer")}>
              Redémarrer…
            </button>
          </div>
          <div className="row">
            <div className="lbl">
              <span>Éteindre</span>
              <span className="t2">Il faudra la rallumer à la main.</span>
            </div>
            <button type="button" className="btn danger" disabled={!enLigne} onClick={() => setQuestion("eteindre")}>
              Éteindre…
            </button>
          </div>
          <div className="row">
            <div className="lbl">
              <span>Retirer de BHOST</span>
              <span className="t2">Ses serveurs continuent de tourner.</span>
            </div>
            <button type="button" className="btn" onClick={onRetirer}>
              Retirer…
            </button>
          </div>
        </section>
      </div>

      {question === "code" && (
        <Confirmer
          titre="Changer le code ?"
          texte="Ce PC garde l’accès. Les autres appareils devront entrer le nouveau code."
          bouton="Changer le code"
          onOui={changerCode}
          onFermer={() => setQuestion(null)}
        />
      )}
      {question === "redemarrer" && (
        <Confirmer
          titre={`Redémarrer ${m.nom} ?`}
          texte="Les serveurs s’arrêtent proprement. Ceux qui démarrent avec la machine repartent tout seuls."
          bouton="Redémarrer"
          onOui={() => demander("redemarrer")}
          onFermer={() => setQuestion(null)}
        />
      )}
      {question === "eteindre" && (
        <Confirmer
          titre={`Éteindre ${m.nom} ?`}
          texte="Les serveurs s’arrêtent proprement. Il faudra rallumer la machine à la main."
          bouton="Éteindre"
          onOui={() => demander("eteindre")}
          onFermer={() => setQuestion(null)}
        />
      )}
      {question === "mettre-a-jour" && (
        <Confirmer
          titre="Installer les mises à jour ?"
          texte="Les serveurs continuent de tourner. Quelques minutes."
          bouton="Installer"
          onOui={() => demander("mettre-a-jour")}
          onFermer={() => setQuestion(null)}
        />
      )}
    </div>
  );
}
