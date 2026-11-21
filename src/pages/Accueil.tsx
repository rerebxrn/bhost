import { useEffect, useState } from "react";
import { IconUserPlus } from "@tabler/icons-react";
import Console from "./Console";
import Courbe from "../components/Courbe";
import Tete from "../components/Tete";
import Inviter from "../components/Inviter";
import MondeCasse from "../components/MondeCasse";
import Problemes from "../components/Problemes";
import Adresse from "../components/Adresse";
import { analysesLag, pourquoiLag, regarderTps, type Probleme } from "../api";
import { adresseLocale, decimale, duree, nombre, nomLogiciel, texteStatut, type Acces, type Machine, type Mesures, type Onglet, type Serveur } from "../types";

type Props = {
  serveur: Serveur;
  machine?: Machine; // la machine dédiée qui l'héberge ; absente = ce PC
  acces?: Acces; // Ouvert à tes amis : absent = fermé
  mesures?: Mesures; // absent tant que le serveur n'a pas tourné
  erreur: string;
  probleme?: Probleme; // la carte d'erreur (Java, port, plantage, mémoire)
  internet: boolean; // faux : plus de connexion
  onChange: (s: Serveur) => void;
  onOnglet: (o: Onglet) => void;
  onCopie: (message: string) => void;
  onErreur: (texte: string) => void;
};

// L'accueil d'un serveur : son état, son adresse, et sa console qui prend la place.
export default function Accueil({ serveur, machine, acces, mesures, erreur, probleme, internet, onChange, onOnglet, onCopie, onErreur }: Props) {
  const [inviter, setInviter] = useState<DOMRect | null>(null);
  // L'heure, relue toutes les 30 secondes, pour « En ligne depuis 3 h 42 ».
  const [maintenant, setMaintenant] = useState(Date.now());
  useEffect(() => {
    const battement = window.setInterval(() => setMaintenant(Date.now()), 30000);
    return () => window.clearInterval(battement);
  }, []);

  // Tant que cette page est affichée, Rust demande au serveur ses ticks par seconde.
  useEffect(() => {
    regarderTps(serveur.id, true).catch(() => {});
    return () => {
      regarderTps(serveur.id, false).catch(() => {});
    };
  }, [serveur.id]);

  // « Pourquoi ça lag ? » : spark analyse 30 s, puis BHOST ouvre son rapport (App.tsx, nouvelle « lag »).
  const [finLag, setFinLag] = useState(() => analysesLag.get(serveur.id) ?? 0);
  const [, setSeconde] = useState(0);
  useEffect(() => setFinLag(analysesLag.get(serveur.id) ?? 0), [serveur.id]);
  const resteLag = Math.max(0, Math.ceil((finLag - Date.now()) / 1000));
  const analyseLag = finLag > 0 && analysesLag.has(serveur.id);
  useEffect(() => {
    if (!analyseLag) return;
    const battement = window.setInterval(() => {
      setSeconde((n) => n + 1);
      if (!analysesLag.has(serveur.id)) setFinLag(0);
    }, 1000);
    return () => window.clearInterval(battement);
  }, [analyseLag, serveur.id]);
  const lancerLag = () => {
    pourquoiLag(serveur.id).then(
      ({ secondes }) => {
        if (secondes === 0) {
          onCopie("spark est installé. Redémarre le serveur, puis réessaie.");
          return;
        }
        const fin = Date.now() + secondes * 1000;
        analysesLag.set(serveur.id, fin);
        setFinLag(fin);
      },
      (e) => onErreur(typeof e === "string" ? e : "Ça n'a pas marché."),
    );
  };

  const depuis = serveur.depuis ? duree(serveur.depuis, maintenant) : "";
  // Sur ce PC : « localhost ». Sur une machine dédiée : son adresse dans la maison.
  const local = machine?.ip ? (serveur.port === 25565 ? machine.ip : `${machine.ip}:${serveur.port}`) : adresseLocale(serveur);
  const disque = machine && serveur.disqueMaxMo ? { mo: serveur.disqueMo ?? 0, max: serveur.disqueMaxMo } : null;
  const pctDisque = disque ? Math.min(100, Math.round((disque.mo / disque.max) * 100)) : 0;

  // Les chiffres du bandeau. Serveur éteint : des tirets, pas de vieux chiffres.
  const allume = serveur.statut !== "arrete" && serveur.statut !== "plante";
  const derniere = (serie?: number[]) => (allume && serie?.length ? serie[serie.length - 1] : null);
  const memoire = derniere(mesures?.memoireMo);
  const processeur = derniere(mesures?.processeur);
  const tps = serveur.statut === "en-ligne" ? derniere(mesures?.tps) : null;
  const courbe = (serie?: number[]) => (allume ? (serie ?? []) : []);

  const copier = () => {
    navigator.clipboard.writeText(local).then(() => onCopie(`« ${local} » copié`));
  };

  // L'adresse pour les amis (Paramètres › Ouvert à tes amis).
  const pourAmis = serveur.ouvertAmis ? acces : undefined;
  const publique = pourAmis?.etat === "ouvert" ? pourAmis.adresse : undefined;
  return (
    <>
      <header className="head ligne">
        <div>
          <h1>{serveur.nom}</h1>
          <div className="meta">
            <span className={"dot " + serveur.statut} aria-hidden />
            <b>
              {texteStatut(serveur)}
              {depuis && ` depuis ${depuis}`} · {publique ? "ouvert à tes amis" : "local"}
            </b>
            <span>· {nomLogiciel(serveur)}</span>
          </div>
          {erreur && (
            <p className="alerte" role="alert">
              {erreur}
            </p>
          )}
        </div>
        <div className="addr">
          <button type="button" className="me-addr" title={machine ? "Pour rejoindre depuis la maison" : "Pour rejoindre depuis ce PC"} onClick={copier}>
            {machine ? "À la maison" : "Toi, sur ce PC"} : <code>{local}</code>
          </button>
          <label htmlFor="addr">Adresse</label>
          <Adresse serveur={serveur} acces={acces} onInfo={onCopie} onErreur={onErreur} />
        </div>
      </header>

      <Problemes serveur={serveur} probleme={probleme} internet={internet} onChange={onChange} onOnglet={onOnglet} onInfo={onCopie} onErreur={onErreur} />
      <MondeCasse id={serveur.id} onInfo={onCopie} onErreur={onErreur} />

      <section className={"surf strip" + (disque ? " cinq" : "")} aria-label="Indicateurs">
        <div className="cell">
          <div>
            <div className="k">Joueurs</div>
            <div className="v">
              {allume ? serveur.joueurs.length : "–"}
              <small>sur {serveur.joueursMax}</small>
            </div>
          </div>
          <Courbe valeurs={courbe(mesures?.joueurs)} max={Math.max(serveur.joueursMax, 1)} />
        </div>
        <div className="cell">
          <div>
            <div className="k">Mémoire</div>
            <div className="v">
              {memoire === null ? "–" : nombre(memoire / 1024)}
              <small>sur {nombre(serveur.ramMo / 1024)} Go</small>
            </div>
          </div>
          <Courbe valeurs={courbe(mesures?.memoireMo)} max={serveur.ramMo} />
        </div>
        <div className="cell">
          <div>
            <div className="k">Processeur</div>
            <div className="v">
              {processeur === null ? "–" : Math.round(processeur)}
              <small>%</small>
            </div>
          </div>
          <Courbe valeurs={courbe(mesures?.processeur)} max={100} />
        </div>
        <div className="cell">
          <div>
            <div className="k">Ticks par seconde</div>
            <div className="v">
              {tps === null ? "–" : decimale(tps)}
              {serveur.statut === "en-ligne" ? (
                <button type="button" className="cell-lag" disabled={analyseLag} onClick={lancerLag}>
                  {analyseLag ? (resteLag > 0 ? `Analyse… ${resteLag} s` : "Rapport…") : "Pourquoi ça lag ?"}
                </button>
              ) : (
                tps !== null && <small>sur 20</small>
              )}
            </div>
          </div>
          <Courbe valeurs={tps === null ? [] : (mesures?.tps ?? [])} max={20} />
        </div>
        {disque && (
          // Le stockage bouge lentement : une barre plutôt qu'une courbe.
          <div className="cell cell-disque">
            <div>
              <div className="k">Stockage</div>
              <div className="v">
                {nombre(disque.mo / 1024)}
                <small>sur {nombre(disque.max / 1024)} Go</small>
              </div>
            </div>
            <span
              className={"sm-barre cell-barre" + (pctDisque >= 90 ? " plein" : pctDisque >= 75 ? " haut" : "")}
              role="meter"
              aria-label="Stockage"
              aria-valuenow={pctDisque}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <i style={{ width: `${pctDisque}%` }} />
            </span>
          </div>
        )}
      </section>

      <div className="work">
        <Console id={serveur.id} />
        <section className="surf players" aria-label="Joueurs connectés">
          <div className="p-h">
            <b>Joueurs</b>
            <span className="p-h-d">
              <span className="tnum">
                {serveur.joueurs.length} sur {serveur.joueursMax}
              </span>
              <button type="button" className="pill-acc teinte" data-gel onClick={(e) => setInviter(e.currentTarget.getBoundingClientRect())}>
                <IconUserPlus size={13} stroke={2.2} aria-hidden />
                Inviter
              </button>
            </span>
          </div>
          {serveur.joueurs.length === 0 ? (
            <div className="empty">Aucun joueur connecté.</div>
          ) : (
            serveur.joueurs.map((nom) => (
              <div className="li" key={nom}>
                <Tete nom={nom} />
                <b>{nom}</b>
              </div>
            ))
          )}
        </section>
      </div>
      {inviter && (
        <Inviter serveur={serveur} acces={acces} ancre={inviter} onFermer={() => setInviter(null)} onInfo={onCopie} onErreur={onErreur} />
      )}
    </>
  );
}
