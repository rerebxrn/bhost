import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
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
import { choisirIso, ecouterCle, ecrireCle, lierMachine, listerClesUsb, wifiDuPc, type AvancementCle, type CleUsb } from "../api";
import { openUrl } from "@tauri-apps/plugin-opener";
import Confirmer from "../components/Confirmer";
import { ChampCode } from "../components/CodeMachine";
import { codeComplet } from "../code";
import type { Machine } from "../types";
import { nombre } from "../types";
import { Tuile } from "./PremierLancement";
import "./NouveauServeur.css";
import "./MachineDediee.css";

// Tutoriel « machine dédiée » : préparer un 2ᵉ PC avec BHOST OS, puis le relier à ce PC.
// Clé USB : BHOST télécharge BHOST OS et l’écrit sur la clé, avec le Wi-Fi (src-tauri/src/cle.rs).

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
  cle: ["Prépare la clé USB", "Branche la clé sur ce PC et choisis-la. BHOST télécharge le système et l’écrit dessus, avec ton Wi-Fi."],
  demarrage: ["Démarre l’autre PC sur la clé", ""],
  installation: ["Installe BHOST OS", "Sur l’autre PC, BHOST OS te guide. Ça prend environ 5 minutes."],
  liaison: ["Relie la machine", "L’autre PC affiche un code de 8 caractères et un QR code."],
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

export default function MachineDediee({ onQuitter, onLiee }: { onQuitter: () => void; onLiee: (m: Machine) => void }) {
  const [etape, setEtape] = useState<Etape>("materiel");
  const [defile, setDefile] = useState(false); // la page a défilé : effet de bord sous les étapes
  const [atteinte, setAtteinte] = useState(0); // la plus loin des étapes déjà ouvertes
  // Étape « Clé USB » : le bouton du bas écrit la clé tant qu’elle n’est pas prête, puis devient « Continuer ».
  const [cle, setCle] = useState<EtatCle>({ prete: false, peutEcrire: false, enCours: false });
  const [demandeEcriture, setDemandeEcriture] = useState(0);
  const ecrireAvant = etape === "cle" && !cle.prete;
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
        <div className={"edgebar" + (defile ? " on" : "")} aria-hidden />
        <div className="scroll" onScroll={(e) => setDefile(e.currentTarget.scrollTop > 4)}>
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
                <Contenu
                  etape={etape}
                  onLiee={onLiee}
                  demandeEcriture={demandeEcriture}
                  clePrete={cle.prete}
                  onCle={setCle}
                  onPasser={() => {
                    setCle((c) => ({ ...c, prete: true }));
                    aller(ETAPES[rang + 1]);
                  }}
                />
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
              <button type="button" data-gel className="ab" disabled={etape === "cle" && cle.enCours} onClick={() => aller(ETAPES[rang - 1])}>
                <IconArrowLeft size={15} stroke={2} aria-hidden />
                Retour
              </button>
            )}
            {ecrireAvant ? (
              <button type="button" data-gel className="ab go" disabled={!cle.peutEcrire || cle.enCours} onClick={() => setDemandeEcriture((n) => n + 1)}>
                {cle.enCours ? "Écriture…" : "Écrire la clé"}
                <IconUsb size={15} stroke={2} aria-hidden />
              </button>
            ) : (
              rang < ETAPES.length - 1 && (
                <button type="button" data-gel className="ab go" onClick={() => aller(ETAPES[rang + 1])}>
                  {etape === "materiel" ? "J’ai tout" : "Continuer"}
                  <IconArrowRight size={15} stroke={2} aria-hidden />
                </button>
              )
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

// ---------- Ce qu’on lit et ce qu’on fait, à droite ----------

type EtatCle = { prete: boolean; peutEcrire: boolean; enCours: boolean };

function Contenu({
  etape,
  onLiee,
  demandeEcriture,
  clePrete,
  onCle,
  onPasser,
}: {
  etape: Etape;
  onLiee: (m: Machine) => void;
  demandeEcriture: number;
  clePrete: boolean;
  onCle: (e: EtatCle) => void;
  onPasser: () => void;
}) {
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
      return <ChoixCle demande={demandeEcriture} deja={clePrete} onEtat={onCle} onPasser={onPasser} />;
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
      return <Liaison onLiee={onLiee} />;
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

const PAGE_ISO = "https://github.com/rerebxrn/bhost/releases/latest";

function ChoixCle({ demande, deja, onEtat, onPasser }: { demande: number; deja: boolean; onEtat: (e: EtatCle) => void; onPasser: () => void }) {
  const [cles, setCles] = useState<CleUsb[] | null>(null);
  const [choix, setChoix] = useState<number | null>(null);
  const [note, setNote] = useState("");
  const [wifiNom, setWifiNom] = useState("");
  const [wifiMdp, setWifiMdp] = useState("");
  const [iso, setIso] = useState<string | null>(null);
  const [confirmer, setConfirmer] = useState(false);
  const [avance, setAvance] = useState<AvancementCle | null>(null);
  const [fini, setFini] = useState(deja); // la clé est déjà prête (retour sur cette étape)

  const chercher = () => {
    setNote("");
    listerClesUsb()
      .then((l) => {
        setCles(l);
        setChoix((c) => (l.some((k) => k.disque === c) ? c : (l[0]?.disque ?? null)));
      })
      .catch(() => setCles([]));
  };
  useEffect(chercher, []);
  // Le Wi-Fi de ce PC, pré-rempli (le mot de passe seulement si Windows le donne).
  useEffect(() => {
    wifiDuPc().then((w) => {
      setWifiNom((n) => n || w.nom);
      setWifiMdp((m) => m || w.motdepasse);
    }, () => {});
  }, []);
  useEffect(() => ecouterCle(setAvance), []);

  const cle = cles?.find((k) => k.disque === choix);
  const enCours = avance !== null;
  // Le bouton du bas (« Écrire la clé ») suit l’état d’ici ; un clic dessus ouvre la confirmation.
  useEffect(() => onEtat({ prete: fini, peutEcrire: choix !== null, enCours }), [fini, choix, enCours, onEtat]);
  const premiereDemande = useRef(demande);
  useEffect(() => {
    if (demande !== premiereDemande.current && choix !== null) setConfirmer(true);
  }, [demande]); // eslint-disable-line react-hooks/exhaustive-deps
  const ecrire = () => {
    if (choix === null) return;
    setNote("");
    setFini(false);
    setAvance({ etape: iso ? "autorisation" : "telechargement", pct: 0 });
    ecrireCle(choix, iso, wifiNom, wifiMdp).then(
      () => {
        setAvance(null);
        setFini(true);
      },
      (e) => {
        setAvance(null);
        setNote(typeof e === "string" ? e : "Ça n’a pas marché.");
      },
    );
  };
  const texteAvance = !avance
    ? ""
    : avance.etape === "telechargement"
      ? `Téléchargement… ${avance.pct} %`
      : avance.etape === "autorisation"
        ? "Accepte la fenêtre de Windows."
        : avance.etape === "verification"
          ? `Vérification… ${avance.pct} %`
          : `Écriture… ${avance.pct} %`;

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
              key={k.disque}
              type="button"
              role="radio"
              aria-checked={choix === k.disque}
              disabled={enCours}
              className={"row md-cle" + (choix === k.disque ? " sel" : "")}
              onClick={() => setChoix(k.disque)}
            >
              <div className="lbl">
                <span>{k.nom}</span>
                <span className="t2">
                  {nombre(k.totalGo)} Go{k.lecteur ? ` · ${k.lecteur}` : ""}
                </span>
              </div>
              <span className="md-rond" aria-hidden>
                <IconCheck size={13} stroke={3} />
              </span>
            </button>
          ))
        )}
      </section>

      <div className="wz-sec">Wi-Fi de la machine</div>
      <section className="surf">
        <div className="row">
          <label htmlFor="md-wifi-nom">Nom</label>
          <input id="md-wifi-nom" className="field texte" type="text" maxLength={32} value={wifiNom} spellCheck={false} disabled={enCours} placeholder="Câble réseau" onChange={(e) => setWifiNom(e.target.value)} />
        </div>
        <div className="row">
          <label htmlFor="md-wifi-mdp">Mot de passe</label>
          <input id="md-wifi-mdp" className="field texte" type="password" maxLength={63} value={wifiMdp} spellCheck={false} disabled={enCours || !wifiNom.trim()} onChange={(e) => setWifiMdp(e.target.value)} />
        </div>
      </section>
      <p className="aide">Vide : la machine utilise un câble réseau.</p>

      <div className="md-actions">
        <button type="button" className="btn" disabled={enCours} onClick={chercher}>
          <IconRefresh size={14} stroke={2} aria-hidden />
          Actualiser
        </button>
      </div>
      {enCours && (
        <div className="md-progres" role="status">
          <span className="t2 tnum">{texteAvance}</span>
          <span className="md-barre fixe">
            <i style={{ transform: `scaleX(${Math.max(0.02, (avance?.pct ?? 0) / 100)})` }} />
          </span>
        </div>
      )}
      {fini && (
        <p className="aide" role="status">
          Clé prête. Branche-la sur la machine, puis clique sur Continuer.
        </p>
      )}
      {note && (
        <p className="aide md-erreur" role="alert">
          {note}
        </p>
      )}
      <p className="aide">
        {iso ? (
          <>
            {`ISO : ${iso.split(/[\\/]/).pop()} · `}
            <button type="button" className="md-lien" disabled={enCours} onClick={() => setIso(null)}>
              Télécharger plutôt
            </button>
          </>
        ) : (
          <>
            <button type="button" className="md-lien" disabled={enCours} onClick={() => choisirIso().then((f) => f && setIso(f), () => {})}>
              Utiliser un fichier ISO
            </button>
            {" · "}
            <button type="button" className="md-lien" onClick={() => openUrl(PAGE_ISO).catch(() => {})}>
              Télécharger l’ISO
            </button>
          </>
        )}
      </p>
      {!fini && !enCours && (
        <p className="aide">
          <button type="button" className="md-lien" onClick={onPasser}>
            J’ai déjà une clé BHOST OS
          </button>
        </p>
      )}
      {confirmer && cle && (
        <Confirmer
          titre="Effacer la clé ?"
          texte={`Tout ce qu’il y a sur « ${cle.nom} »${cle.lecteur ? ` (${cle.lecteur})` : ""} sera effacé.`}
          bouton="Effacer et écrire"
          onOui={ecrire}
          onFermer={() => setConfirmer(false)}
        />
      )}
    </>
  );
}

function Liaison({ onLiee }: { onLiee: (m: Machine) => void }) {
  const [code, setCode] = useState("");
  const [note, setNote] = useState("");
  const [envoi, setEnvoi] = useState(false);

  const relier = () => {
    if (!codeComplet(code) || envoi) return;
    setEnvoi(true);
    lierMachine(code).then(onLiee, (e) => {
      setNote(typeof e === "string" ? e : PAS_ENCORE);
      setEnvoi(false);
    });
  };

  return (
    <>
      <div className="wz-sec">Sur ce PC, tape le code</div>
      <ChampCode
        grand
        code={code}
        onChange={(c) => {
          setNote("");
          setCode(c);
        }}
      />
      <div className="md-actions">
        <button type="button" className="btn acc" disabled={!codeComplet(code) || envoi} onClick={relier}>
          {envoi ? "Connexion…" : "Relier"}
        </button>
      </div>
      {note && (
        <p className="aide md-erreur" role="alert">
          {note}
        </p>
      )}
      <p className="aide">Le code reste dans les réglages de la machine. Garde-le pour toi : il donne accès à tes serveurs.</p>
      <p className="wz-note">
        <IconDeviceMobile size={15} stroke={2} aria-hidden />
        <span>Sur ton téléphone, scanne le QR code : tu pilotes tes serveurs sans rien installer.</span>
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
                <b>K7M2-Q9XR</b>
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
