import { useEffect, useState } from "react";
import { IconCopy, IconUserPlus } from "@tabler/icons-react";
import Console from "./Console";
import Courbe from "../components/Courbe";
import Tete from "../components/Tete";
import Inviter from "../components/Inviter";
import MondeCasse from "../components/MondeCasse";
import Problemes from "../components/Problemes";
import { regarderTps, type Probleme } from "../api";
import { adresseLocale, decimale, duree, nombre, nomLogiciel, texteStatut, type Acces, type Mesures, type Onglet, type Serveur } from "../types";

type Props = {
  serveur: Serveur;
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
export default function Accueil({ serveur, acces, mesures, erreur, probleme, internet, onChange, onOnglet, onCopie, onErreur }: Props) {
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

  const depuis = serveur.depuis ? duree(serveur.depuis, maintenant) : "";
  const local = adresseLocale(serveur);

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
  const texteAdresse = publique
    ? publique
    : !serveur.ouvertAmis
      ? "Fermé · local uniquement"
      : pourAmis?.etat === "erreur"
        ? "Fermé · voir Paramètres"
        : "Ouverture de ta box…";
  const copierPublique = () => {
    if (publique) navigator.clipboard.writeText(publique).then(() => onCopie(`« ${publique} » copié`));
  };

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
          <button type="button" className="me-addr" title="Pour rejoindre depuis ce PC" onClick={copier}>
            Toi, sur ce PC : <code>{local}</code>
          </button>
          <label htmlFor="addr">Adresse</label>
          <input
            id="addr"
            className={"field" + (publique ? "" : " is-off")}
            type="text"
            readOnly
            value={texteAdresse}
            title={pourAmis?.etat === "erreur" ? pourAmis.message : undefined}
          />
          <button type="button" className="btn acc" disabled={!publique} onClick={copierPublique}>
            <IconCopy size={14} stroke={2} aria-hidden />
            Copier
          </button>
        </div>
      </header>

      <Problemes serveur={serveur} probleme={probleme} internet={internet} onChange={onChange} onOnglet={onOnglet} onInfo={onCopie} onErreur={onErreur} />
      <MondeCasse id={serveur.id} onInfo={onCopie} onErreur={onErreur} />

      <section className="surf strip" aria-label="Indicateurs">
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
              {tps !== null && <small>sur 20</small>}
            </div>
          </div>
          <Courbe valeurs={tps === null ? [] : (mesures?.tps ?? [])} max={20} />
        </div>
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
