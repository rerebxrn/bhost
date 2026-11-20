import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  IconAlertTriangle,
  IconArrowLeft,
  IconArrowRight,
  IconCheck,
  IconDeviceDesktop,
  IconDeviceLaptop,
  IconDeviceMobile,
  IconKeyboard,
  IconQrcode,
  IconRefresh,
  IconUsb,
  IconWifi,
} from "@tabler/icons-react";
import Tabs from "../components/Tabs";
import { listerClesUsb, type CleUsb } from "../api";
import { nombre } from "../types";
import { Tuile } from "./PremierLancement";
import "./NouveauServeur.css";
import "./MachineDediee.css";

// Tutoriel « machine dédiée » : préparer un 2ᵉ PC avec BHOST OS, puis le relier à ce PC.
// La liste des clés USB marche déjà. L'écriture de la clé et la liaison viendront avec BHOST OS.

type Etape = "materiel" | "cle" | "demarrage" | "installation" | "liaison";
const ETAPES: Etape[] = ["materiel", "cle", "demarrage", "installation", "liaison"];
const NOMS: Record<Etape, string> = {
  materiel: "Matériel",
  cle: "Clé USB",
  demarrage: "Démarrage",
  installation: "Installation",
  liaison: "Liaison",
};
const TITRES: Record<Etape, [string, string]> = {
  materiel: ["Ce qu’il te faut", "Prévois un quart d’heure. BHOST te guide à chaque étape."],
  cle: ["Prépare la clé USB", "Branche la clé sur ce PC et choisis-la. BHOST télécharge le système et l’écrit dessus."],
  demarrage: ["Démarre l’autre PC sur la clé", ""],
  installation: ["Installe BHOST OS", "Sur l’autre PC, BHOST OS te guide. Ça prend environ 5 minutes."],
  liaison: ["Relie la machine", "L’autre PC affiche un code et un QR code."],
};
const TOUCHES: [string, string][] = [
  ["Dell", "F12"],
  ["HP", "F9"],
  ["Lenovo", "F12"],
  ["Asus", "F8"],
  ["Acer", "F12"],
  ["MSI", "F11"],
  ["Autre", "Échap ou F12"],
];
const PAS_ENCORE = "BHOST OS n’est pas encore disponible : cette étape marchera dans une prochaine version.";

export default function MachineDediee({ onQuitter }: { onQuitter: () => void }) {
  const [etape, setEtape] = useState<Etape>("materiel");
  const [atteinte, setAtteinte] = useState(0); // la plus loin des étapes déjà ouvertes
  const rang = ETAPES.indexOf(etape);

  const aller = (e: Etape) => {
    setEtape(e);
    setAtteinte((a) => Math.max(a, ETAPES.indexOf(e)));
  };

  const onglets = useMemo(() => ETAPES.map((e, i) => ({ id: e, nom: NOMS[e], off: i > atteinte })), [atteinte]);
  const [titre, sousTitre] = TITRES[etape];

  return (
    <div className="corps">
      <main className="main md-main">
        <Tabs onglets={onglets} actif={etape} onChoisir={(e) => ETAPES.indexOf(e) <= atteinte && aller(e)} etiquette="Étapes du tutoriel" />
        <div className="scroll">
          <div className="page wz md" key={etape}>
            <div className="wz-head">
              <span className="wz-kick">{`Étape ${rang + 1} sur ${ETAPES.length}`}</span>
              <h1>{titre}</h1>
            </div>
            <div className="md-grille">
              <div className="md-scene" aria-hidden>
                <Scene etape={etape} />
              </div>
              <div className="md-texte">
                {sousTitre && <p className="md-intro">{sousTitre}</p>}
                <Contenu etape={etape} />
              </div>
            </div>
          </div>
        </div>

        <div className="dock">
          <div className="g wz-nav" data-lens="bar">
            <button type="button" data-gel className="ab" onClick={onQuitter}>
              Annuler
            </button>
          </div>
          <div className="g wz-nav" data-lens="bar" role="group" aria-label="Navigation du tutoriel">
            {rang > 0 && (
              <button type="button" data-gel className="ab" onClick={() => aller(ETAPES[rang - 1])}>
                <IconArrowLeft size={15} stroke={2} aria-hidden />
                Retour
              </button>
            )}
            {rang < ETAPES.length - 1 && (
              <button type="button" data-gel className="ab go" onClick={() => aller(ETAPES[rang + 1])}>
                {etape === "materiel" ? "J’ai tout" : "Continuer"}
                <IconArrowRight size={15} stroke={2} aria-hidden />
              </button>
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

// ---------- Ce qu'on lit et ce qu'on fait, à droite ----------

function Contenu({ etape }: { etape: Etape }) {
  switch (etape) {
    case "materiel":
      return (
        <>
          <section className="surf">
            <Ligne titre="Un 2ᵉ PC" detail="Même vieux. 4 Go de mémoire ou plus." />
            <Ligne titre="Une clé USB" detail="8 Go ou plus." />
            <Ligne titre="Internet" detail="En Wi-Fi ou avec un câble." />
          </section>
          <p className="wz-note md-attention">
            <IconAlertTriangle size={15} stroke={2} aria-hidden />
            <span>Le PC et la clé seront effacés : sauvegarde ce qui compte avant.</span>
          </p>
        </>
      );
    case "cle":
      return <ChoixCle />;
    case "demarrage":
      return (
        <>
          <ol className="md-etapes">
            <li>Branche la clé sur l’autre PC, éteint.</li>
            <li>Allume-le et appuie plusieurs fois sur la touche de démarrage.</li>
            <li>Choisis la clé USB dans le menu.</li>
          </ol>
          <div className="wz-sec">La touche selon la marque</div>
          <div className="md-touches">
            {TOUCHES.map(([marque, touche]) => (
              <span key={marque}>
                {marque} <b>{touche}</b>
              </span>
            ))}
          </div>
          <p className="aide">Rien ne se passe ? Redémarre et essaie une autre touche.</p>
        </>
      );
    case "installation":
      return (
        <>
          <section className="surf">
            <Ligne titre="Choisis ton Wi-Fi" detail="Ou branche un câble réseau." />
            <Ligne titre="Choisis le disque" detail="Tout ce qu’il contient sera effacé." />
            <Ligne titre="Retire la clé quand on te le demande" detail="Le PC redémarre tout seul." />
          </section>
          <p className="aide">Quand l’autre PC affiche un code, passe à la suite.</p>
        </>
      );
    case "liaison":
      return <Liaison />;
  }
}

function Ligne({ titre, detail }: { titre: string; detail: string }) {
  return (
    <div className="row">
      <div className="lbl">
        <span>{titre}</span>
        <span className="t2">{detail}</span>
      </div>
    </div>
  );
}

function ChoixCle() {
  const [cles, setCles] = useState<CleUsb[] | null>(null);
  const [choix, setChoix] = useState("");
  const [note, setNote] = useState("");

  const chercher = () => {
    setNote("");
    listerClesUsb()
      .then((l) => {
        setCles(l);
        setChoix((c) => (l.some((k) => k.lecteur === c) ? c : (l[0]?.lecteur ?? "")));
      })
      .catch(() => setCles([]));
  };
  useEffect(chercher, []);

  return (
    <>
      <section className="surf" role="radiogroup" aria-label="Clé USB">
        {cles === null ? (
          <div className="row">
            <span className="t2">Recherche des clés USB…</span>
          </div>
        ) : cles.length === 0 ? (
          <div className="row">
            <div className="lbl">
              <span>Aucune clé USB</span>
              <span className="t2">Branche-la sur ce PC.</span>
            </div>
          </div>
        ) : (
          cles.map((k) => (
            <button
              key={k.lecteur}
              type="button"
              role="radio"
              aria-checked={choix === k.lecteur}
              className={"row md-cle" + (choix === k.lecteur ? " sel" : "")}
              onClick={() => setChoix(k.lecteur)}
            >
              <div className="lbl">
                <span>{k.nom}</span>
                <span className="t2">
                  {nombre(k.totalGo)} Go · {k.lecteur}
                </span>
              </div>
              <span className="md-rond" aria-hidden>
                <IconCheck size={13} stroke={3} />
              </span>
            </button>
          ))
        )}
      </section>
      <div className="md-actions">
        <button type="button" className="btn acc" disabled={!choix} onClick={() => setNote(PAS_ENCORE)}>
          Écrire la clé
        </button>
        <button type="button" className="btn" onClick={chercher}>
          <IconRefresh size={14} stroke={2} aria-hidden />
          Actualiser
        </button>
      </div>
      {note && (
        <p className="aide" role="status">
          {note}
        </p>
      )}
      <p className="aide">
        <button type="button" className="md-lien" onClick={() => setNote(PAS_ENCORE)}>
          Je préfère télécharger l’ISO et l’installer moi-même
        </button>
      </p>
    </>
  );
}

function Liaison() {
  const [code, setCode] = useState("");
  const [note, setNote] = useState("");
  const complet = code.length === 6;

  return (
    <>
      <div className="wz-sec">Sur ce PC, tape le code</div>
      <input
        className="md-code"
        type="text"
        inputMode="text"
        maxLength={6}
        spellCheck={false}
        autoComplete="off"
        aria-label="Code de liaison"
        placeholder="······"
        value={code}
        onChange={(e) => {
          setNote("");
          setCode(e.target.value.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 6));
        }}
      />
      <div className="md-actions">
        <button type="button" className="btn acc" disabled={!complet} onClick={() => setNote(PAS_ENCORE)}>
          Relier
        </button>
      </div>
      {note && (
        <p className="aide" role="status">
          {note}
        </p>
      )}
      <p className="wz-note">
        <IconDeviceMobile size={15} stroke={2} aria-hidden />
        <span>Pour piloter tes serveurs depuis ton téléphone, scanne le QR code avec l’appareil photo.</span>
      </p>
    </>
  );
}

// ---------- Les illustrations, à gauche (icônes Tabler sur un fond uni) ----------

function Pastille({ children, legende }: { children: ReactNode; legende?: string }) {
  return (
    <figure className="md-objet">
      <span className="md-pastille">{children}</span>
      {legende && <figcaption>{legende}</figcaption>}
    </figure>
  );
}

function Scene({ etape }: { etape: Etape }) {
  switch (etape) {
    case "materiel":
      return (
        <div className="md-pile">
          <Pastille>
            <IconDeviceDesktop size={46} stroke={1.6} />
          </Pastille>
          <div className="md-rangee">
            <Pastille>
              <IconUsb size={40} stroke={1.7} />
            </Pastille>
            <Pastille>
              <IconWifi size={40} stroke={1.7} />
            </Pastille>
          </div>
        </div>
      );
    case "cle":
      return (
        <div className="md-rangee md-lien-anime">
          <Pastille legende="Ce PC">
            <IconDeviceDesktop size={42} stroke={1.6} />
          </Pastille>
          <span className="md-flux" />
          <Pastille legende="Ta clé">
            <IconUsb size={40} stroke={1.7} />
          </Pastille>
        </div>
      );
    case "demarrage":
      return (
        <div className="md-pile">
          <div className="md-carte md-menu">
            <span className="md-carte-titre">Menu de démarrage</span>
            <span>Disque interne</span>
            <span className="sel">USB : clé BHOST</span>
            <span>Réseau</span>
          </div>
          <Pastille legende="Touche de démarrage">
            <IconKeyboard size={40} stroke={1.7} />
          </Pastille>
        </div>
      );
    case "installation":
      return (
        <div className="md-pile">
          <div className="md-carte md-install">
            <Tuile />
            <b>Installation de BHOST OS</b>
            <span>Wi-Fi « Maison » · disque de 256 Go</span>
            <span className="md-barre">
              <i />
            </span>
          </div>
          <span className="md-sous">Sur l’autre PC</span>
        </div>
      );
    case "liaison":
      return (
        <div className="md-pile md-liaison">
          <div className="md-objet">
            <div className="md-carte md-code-carte">
              <div>
                <span>Code de liaison</span>
                <b>K7P 2QX</b>
              </div>
              <IconQrcode size={64} stroke={1.4} />
            </div>
            <span className="md-sous">Sur l’autre PC</span>
          </div>
          <div className="md-rangee">
            <Pastille legende="Ce PC">
              <IconDeviceLaptop size={40} stroke={1.6} />
            </Pastille>
            <Pastille legende="Ton téléphone">
              <IconDeviceMobile size={40} stroke={1.6} />
            </Pastille>
          </div>
        </div>
      );
  }
}
