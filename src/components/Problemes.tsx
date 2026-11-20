import { useEffect, useState, type ReactNode } from "react";
import { IconAlertOctagon, IconCoffee, IconCpu, IconPlugX, IconWifiOff, IconX } from "@tabler/icons-react";
import {
  activerExtension,
  choisirJava,
  demarrerServeur,
  extensionsServeur,
  installerJava,
  reglerProprietes,
  reglerServeur,
  sauvegardesServeur,
  verifierInternet,
  type Probleme,
} from "../api";
import { nombre, nomLogiciel, type Onglet, type Serveur } from "../types";
import "./Problemes.css";

type Props = {
  serveur: Serveur;
  probleme?: Probleme;
  internet: boolean; // faux : la connexion est coupée
  onChange: (s: Serveur) => void; // réglage changé (port, mémoire, Java)
  onOnglet: (o: Onglet) => void;
  onInfo: (texte: string) => void;
  onErreur: (texte: string) => void;
};

// « il y a 3 min », « il y a 2 h », « il y a 4 j ».
function ilYa(quand: number) {
  const minutes = Math.floor((Date.now() - quand) / 60000);
  if (minutes < 1) return "à l'instant";
  if (minutes < 60) return `il y a ${minutes} min`;
  if (minutes < 48 * 60) return `il y a ${Math.floor(minutes / 60)} h`;
  return `il y a ${Math.floor(minutes / 1440)} j`;
}

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n'a pas marché.");

// Une carte : icône, titre, texte, et les boutons pour régler le problème.
function Carte(props: {
  ton: "warn" | "bad";
  icone: ReactNode;
  titre: string;
  texte: ReactNode;
  cause?: string;
  avancement?: { texte: string; pourcent: number | null };
  children?: ReactNode;
  lien?: { texte: string; action: () => void };
  onFermer?: () => void;
}) {
  return (
    <section className={"surf err " + props.ton} role="alert">
      <span className="err-ic" aria-hidden>
        {props.icone}
      </span>
      <div className="err-c">
        <b className="err-t">{props.titre}</b>
        <p className="err-x">{props.texte}</p>
        {props.cause && <p className="err-cause">{props.cause}</p>}
        {props.avancement ? (
          <div className="err-av">
            <span>{props.avancement.texte}</span>
            <div className="err-barre" role="progressbar" aria-valuenow={props.avancement.pourcent ?? undefined} aria-valuemin={0} aria-valuemax={100}>
              <i style={{ width: `${props.avancement.pourcent ?? 100}%` }} className={props.avancement.pourcent === null ? "plein" : ""} />
            </div>
          </div>
        ) : (
          (props.children || props.lien) && (
            <div className="err-a">
              {props.children}
              {props.lien && (
                <button type="button" className="err-lien" onClick={props.lien.action}>
                  {props.lien.texte}
                </button>
              )}
            </div>
          )
        )}
      </div>
      {props.onFermer && (
        <button type="button" className="err-fermer" title="Masquer" aria-label="Masquer" onClick={props.onFermer}>
          <IconX size={14} stroke={2} aria-hidden />
        </button>
      )}
    </section>
  );
}

// Les problèmes d'un serveur, en haut de son Accueil (maquette : « États d'erreur »).
export default function Problemes({ serveur, probleme, internet, onChange, onOnglet, onInfo, onErreur }: Props) {
  const [occupe, setOccupe] = useState(false);
  const [masque, setMasque] = useState<Probleme | null>(null);
  // Plantage : la cause probable (une extension en panne) et la dernière sauvegarde.
  const [cause, setCause] = useState<{ nom: string; fichier: string; panne: string } | null>(null);
  const [sauvegarde, setSauvegarde] = useState<number | null>(null);
  const id = serveur.id;

  useEffect(() => {
    setCause(null);
    setSauvegarde(null);
    if (probleme?.genre !== "crash") return;
    let vivant = true;
    extensionsServeur(id)
      .then((vue) => {
        const liste = vue.liste.filter((e) => e.actif && e.panne);
        // Une extension qui plante en jeu passe avant une qui n'a pas démarré.
        const e = liste.find((x) => x.panneEnJeu) ?? liste[0];
        if (vivant && e?.panne) setCause({ nom: e.nom, fichier: e.fichier, panne: e.panne });
      })
      .catch(() => {});
    sauvegardesServeur(id)
      .then((s) => vivant && setSauvegarde(s.liste.reduce<number | null>((m, x) => (m === null || x.quand > m ? x.quand : m), null)))
      .catch(() => {});
    return () => {
      vivant = false;
    };
  }, [id, probleme]);

  // Fait l'action, puis démarre le serveur.
  const faire = async (avant?: () => Promise<unknown>) => {
    onErreur("");
    setOccupe(true);
    try {
      if (avant) await avant();
      await demarrerServeur(id);
    } catch (e) {
      onErreur(phrase(e));
    } finally {
      setOccupe(false);
    }
  };

  const reglages = (changement: { ramMo?: number; javaChoix?: string }) =>
    reglerServeur(id, {
      ramMo: changement.ramMo ?? serveur.ramMo,
      demarrerAuto: !!serveur.demarrerAuto,
      javaChoix: changement.javaChoix ?? serveur.javaChoix ?? null,
      optimise: serveur.optimise ?? true,
      arguments: serveur.arguments ?? "",
    }).then(([s]) => onChange(s));

  const cartes: ReactNode[] = [];
  const montre = probleme && probleme !== masque ? probleme : undefined;

  if (montre?.genre === "java") {
    const v = montre.version;
    const telechargement = serveur.java && serveur.statut === "demarrage" ? serveur.java : null;
    cartes.push(
      <Carte
        key="java"
        ton="warn"
        icone={<IconCoffee size={18} stroke={2} />}
        titre={telechargement ? `Installation de Java ${v}` : `Java ${v} est introuvable`}
        texte={`${nomLogiciel(serveur)} a besoin de Java ${v} pour démarrer. Installation gratuite, en un clic.`}
        avancement={
          telechargement
            ? telechargement.pourcent >= 100
              ? { texte: "Installé. Démarrage du serveur…", pourcent: null }
              : { texte: `Téléchargement de Java ${v}… ${telechargement.pourcent} %`, pourcent: telechargement.pourcent }
            : undefined
        }
        onFermer={() => setMasque(montre)}
      >
        <button type="button" className="btn acc" disabled={occupe} onClick={() => faire(() => installerJava(id, v).then(() => {}))}>
          Installer Java {v}
        </button>
        <button
          type="button"
          className="btn"
          disabled={occupe}
          onClick={async () => {
            const chemin = await choisirJava();
            if (chemin) faire(() => reglages({ javaChoix: chemin }));
          }}
        >
          J'ai déjà Java…
        </button>
      </Carte>,
    );
  }

  if (montre?.genre === "port") {
    const { port, libre } = montre;
    cartes.push(
      <Carte
        key="port"
        ton="warn"
        icone={<IconPlugX size={18} stroke={2} />}
        titre={`Le port ${port} est déjà pris`}
        texte={montre.serveur ? `Ton serveur « ${montre.serveur} » l'utilise déjà.` : "Un autre programme l'utilise déjà sur ce PC."}
        onFermer={() => setMasque(montre)}
      >
        {libre !== null && (
          <button
            type="button"
            className="btn acc"
            disabled={occupe}
            onClick={() =>
              faire(async () => {
                await reglerProprietes(id, { "server-port": String(libre) });
                onChange({ ...serveur, port: libre });
                onInfo(`Port ${libre} choisi.`);
              })
            }
          >
            Utiliser le port {libre}
          </button>
        )}
        <button type="button" className={"btn" + (libre === null ? " acc" : "")} disabled={occupe} onClick={() => faire()}>
          Réessayer
        </button>
      </Carte>,
    );
  }

  if (montre?.genre === "crash") {
    const quand = sauvegarde ? ` Dernière sauvegarde : ${ilYa(sauvegarde)}.` : "";
    cartes.push(
      <Carte
        key="crash"
        ton="bad"
        icone={<IconAlertOctagon size={18} stroke={2} />}
        titre="Le serveur a planté"
        texte={`Il s'est arrêté tout seul.${quand}`}
        cause={cause ? `Cause probable : ${cause.nom}. ${cause.panne}` : undefined}
        lien={{ texte: "Voir la console", action: () => onOnglet("console") }}
        onFermer={() => setMasque(montre)}
      >
        {cause && (
          <button type="button" className="btn acc" disabled={occupe} onClick={() => faire(() => activerExtension(id, cause.fichier, false))}>
            Désactiver {cause.nom} et redémarrer
          </button>
        )}
        <button type="button" className={"btn" + (cause ? "" : " acc")} disabled={occupe} onClick={() => faire()}>
          {cause ? "Redémarrer quand même" : "Redémarrer"}
        </button>
      </Carte>,
    );
  }

  if (montre?.genre === "memoire") {
    const { go, libreGo } = montre;
    const plus = Math.round(go) + 2;
    const regler = { texte: "Régler la mémoire", action: () => onOnglet("parametres") };
    cartes.push(
      montre.demarrage ? (
        <Carte
          key="mem"
          ton="bad"
          icone={<IconCpu size={18} stroke={2} />}
          titre="Pas assez de mémoire pour démarrer"
          texte={`Il demande ${nombre(go)} Go, et ton PC n'en a que ${nombre(libreGo)} de libres.`}
          cause="Ferme des programmes, ou baisse sa mémoire."
          lien={regler}
          onFermer={() => setMasque(montre)}
        >
          <button type="button" className="btn acc" disabled={occupe} onClick={() => faire()}>
            Réessayer
          </button>
        </Carte>
      ) : (
        <Carte
          key="mem"
          ton="bad"
          icone={<IconCpu size={18} stroke={2} />}
          titre="Le serveur a manqué de mémoire"
          texte={`Il a utilisé les ${nombre(go)} Go alloués. ${nombre(libreGo)} Go libres sur ton PC.`}
          cause="Souvent : grosses fermes ou joueurs éparpillés."
          lien={regler}
          onFermer={() => setMasque(montre)}
        >
          {libreGo >= 2 && (
            <button type="button" className="btn acc" disabled={occupe} onClick={() => faire(() => reglages({ ramMo: plus * 1024 }))}>
              Passer à {plus} Go et redémarrer
            </button>
          )}
          <button type="button" className={"btn" + (libreGo >= 2 ? "" : " acc")} disabled={occupe} onClick={() => faire()}>
            Redémarrer
          </button>
        </Carte>
      ),
    );
  }

  // Plus de connexion : seulement pour un serveur ouvert aux amis et allumé.
  const allume = serveur.statut === "en-ligne" || serveur.statut === "demarrage";
  if (!internet && serveur.ouvertAmis && allume) {
    cartes.push(
      <Carte
        key="net"
        ton="warn"
        icone={<IconWifiOff size={18} stroke={2} />}
        titre="Plus de connexion Internet"
        texte="Tes amis ne peuvent plus rejoindre. Le serveur continue pour ce PC."
      >
        <button
          type="button"
          className="btn acc"
          disabled={occupe}
          onClick={async () => {
            setOccupe(true);
            const ok = await verifierInternet().catch(() => false);
            setOccupe(false);
            onInfo(ok ? "Connexion revenue." : "Toujours pas de connexion.");
          }}
        >
          {occupe ? "Vérification…" : "Réessayer maintenant"}
        </button>
      </Carte>,
    );
  }

  return cartes.length ? <div className="errs">{cartes}</div> : null;
}
