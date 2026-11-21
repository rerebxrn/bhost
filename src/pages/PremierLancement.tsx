import { useState, type ReactNode } from "react";
import {
  IconAlertCircle,
  IconArrowRight,
  IconCheck,
  IconCircleCheck,
  IconDeviceDesktop,
  IconDeviceGamepad2,
  IconDeviceLaptop,
  IconMoon,
  IconServer2,
  IconSun,
  IconUsb,
  IconUser,
  IconUsers,
  IconZzz,
} from "@tabler/icons-react";
import "./PremierLancement.css";

// Premier lancement : on choisit où héberger ses serveurs.
// « Sur cet ordinateur » ouvre l'appli, « Sur une machine dédiée » ouvre le tutoriel.
type Choix = "local" | "machine";

type Props = {
  onLocal: () => void;
  onMachine: () => void; // tutoriel « Préparer la machine »
  onDejaMachine: () => void; // la machine est déjà prête : on tape son code
};

export default function PremierLancement({ onLocal, onMachine, onDejaMachine }: Props) {
  const [choix, setChoix] = useState<Choix>("local");
  const choisir = (c: Choix) => setChoix(c);

  return (
    <div className="pl">
      <section className="g pl-panneau" aria-labelledby="pl-titre">
        <header className="pl-tete pl-entree">
          <Tuile />
          <h1 id="pl-titre">Bienvenue sur BHOST</h1>
          <p>Où veux-tu héberger tes serveurs ?</p>
        </header>

        <div className="pl-cartes" role="radiogroup" aria-labelledby="pl-titre">
          <Carte
            choisie={choix === "local"}
            onChoisir={() => choisir("local")}
            delai={0.15}
            scene={<SceneLocal />}
            titre="Sur cet ordinateur"
            phrase="Tu joues et tu héberges sur le même PC."
            puces={["Prêt en 1 min", "Rien d’autre à acheter"]}
            plus={["Le plus simple pour commencer", "Toute la puissance de ton PC"]}
            moins="PC éteint = serveur éteint"
          />
          <Carte
            choisie={choix === "machine"}
            onChoisir={() => choisir("machine")}
            delai={0.25}
            scene={<SceneMachine />}
            titre="Sur une machine dédiée"
            phrase="Un autre PC, même vieux, devient ton serveur."
            puces={["≈ 15 min, guidé", "2ᵉ PC + clé USB"]}
            plus={["Tes amis jouent même quand ton PC est éteint", "Pilotable depuis ton téléphone"]}
            moins="La 2ᵉ machine est effacée à l’installation"
          />
        </div>

        <div className="pl-suite pl-entree" style={{ animationDelay: "0.35s" }}>
          <button
            type="button"
            className="btn acc pl-bouton"
            data-gel
            onClick={() => (choix === "local" ? onLocal() : onMachine())}
          >
            {choix === "local" ? "Héberger sur ce PC" : "Préparer la machine"}
            <IconArrowRight size={17} stroke={2.2} aria-hidden />
          </button>
          {choix === "local" ? (
            <p className="pl-note">Pas sûr ? Commence ici. Tu pourras passer sur une machine dédiée plus tard.</p>
          ) : (
            <p className="pl-note">
              Ta machine est déjà prête ?{" "}
              <button type="button" className="pl-lien" onClick={onDejaMachine}>
                Entre son code
              </button>
            </p>
          )}
        </div>
      </section>
    </div>
  );
}

function Carte(p: {
  choisie: boolean;
  onChoisir: () => void;
  delai: number;
  scene: ReactNode;
  titre: string;
  phrase: string;
  puces: string[];
  plus: string[];
  moins: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={p.choisie}
      className={"g pl-carte pl-entree" + (p.choisie ? " choisie" : "")}
      style={{ animationDelay: `${p.delai}s` }}
      onClick={p.onChoisir}
    >
      <div className="pl-scene">{p.scene}</div>
      <div className="pl-texte">
        <div className="pl-titre">
          <h2>{p.titre}</h2>
          <span className="pl-rond" aria-hidden>
            <IconCheck size={14} stroke={3} />
          </span>
        </div>
        <p className="pl-phrase">{p.phrase}</p>
        <div className="pl-puces">
          {p.puces.map((t) => (
            <span key={t}>{t}</span>
          ))}
        </div>
        <ul className="pl-liste">
          {p.plus.map((t) => (
            <li key={t} className="oui">
              <IconCircleCheck size={18} stroke={1.8} aria-hidden />
              <span>{t}</span>
            </li>
          ))}
          <li className="non">
            <IconAlertCircle size={18} stroke={1.8} aria-hidden />
            <span>{p.moins}</span>
          </li>
        </ul>
      </div>
    </button>
  );
}

// Le logo de l'appli (le B et son point), aux couleurs du thème.
export function Tuile() {
  return (
    <svg className="pl-tuile" viewBox="-165 -765 1060 1060" aria-hidden>
      <rect x="-128" y="-728" width="986" height="986" rx="222" style={{ fill: "var(--pl-tuile)", stroke: "var(--pl-tuile-bord)" }} strokeWidth="14" />
      <g transform="translate(137.1 23.0) scale(0.86)">
        <path
          style={{ fill: "var(--pl-tuile-b)" }}
          d="M30 0Q38 -149 38.0 -300.0Q38 -451 30 -600H224Q299 -600 347.0 -593.5Q395 -587 422.0 -569.5Q449 -552 459.0 -521.5Q469 -491 469 -444Q469 -379 450.5 -347.5Q432 -316 384 -306Q419 -303 441.5 -294.0Q464 -285 477.0 -268.0Q490 -251 495.0 -225.5Q500 -200 500 -162Q500 -110 490.0 -78.5Q480 -47 451.5 -29.5Q423 -12 370.5 -6.0Q318 0 234 0ZM327 -391Q354 -391 361.5 -396.0Q369 -401 369 -411Q369 -421 361.5 -426.0Q354 -431 327 -431L164 -430Q164 -420 163.5 -410.0Q163 -400 163 -390ZM346 -171Q378 -171 389.0 -177.0Q400 -183 400 -197Q400 -210 389.0 -215.5Q378 -221 346 -221L163 -220Q163 -207 163.5 -195.0Q164 -183 164 -170Z"
        />
        <circle cx="524" cy="-566" r="68" strokeWidth="34" style={{ fill: "var(--pl-tuile-point)", stroke: "var(--pl-tuile)" }} />
      </g>
    </svg>
  );
}

// Les deux scènes ont la même grille : trois pastilles alignées au centre (x = 86, 200, 314),
// la plus grande au milieu, une légende sous chacune, des points qui circulent sur les liens.
// Uniquement des icônes Tabler.
function Lien({ x1, x2 }: { x1: number; x2: number }) {
  return <path d={`M${x1} 96 H ${x2}`} fill="none" stroke="currentColor" strokeOpacity=".45" strokeWidth="2" strokeDasharray="2 6" strokeLinecap="round" />;
}

function Legende({ x, texte, nuit = false }: { x: number; texte: string; nuit?: boolean }) {
  return (
    <text x={x} y="160" textAnchor="middle" fontSize="12" fontWeight="700" className={nuit ? "pl-etiquette" : "pl-f-encre"}>
      {texte}
    </text>
  );
}

// Sur ce PC : ton PC fait les deux, tu y joues (badge manette) et il héberge le serveur (badge serveur).
// Tes amis s'y connectent des deux côtés.
function SceneLocal() {
  return (
    <svg viewBox="0 8 400 200" preserveAspectRatio="xMidYMid meet" aria-hidden>
      <defs>
        <linearGradient id="pl-jour-a" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" className="pl-st-jour-1" />
          <stop offset="1" className="pl-st-jour-2" />
        </linearGradient>
      </defs>
      <rect x="-300" y="-150" width="1000" height="500" fill="url(#pl-jour-a)" />

      <g className="pl-c-acc">
        <Lien x1={118} x2={156} />
        <Lien x1={244} x2={282} />
        <circle className="pl-point g-vers-c" r="3.4" fill="currentColor" />
        <circle className="pl-point g-vers-c b" r="3.4" fill="currentColor" />
        <circle className="pl-point d-vers-c" r="3.4" fill="currentColor" />
        <circle className="pl-point d-vers-c b" r="3.4" fill="currentColor" />
      </g>

      <circle cx="86" cy="96" r="30" className="pl-f-rond" />
      <g className="pl-c-encre">
        <IconUser x={70} y={80} size={32} stroke={1.4} />
      </g>
      <Legende x={86} texte="Un ami" />

      <circle cx="200" cy="96" r="42" className="pl-f-rond" />
      <g className="pl-c-encre">
        <IconDeviceDesktop x={176} y={72} size={48} stroke={1.3} />
      </g>
      {/* Tu joues dessus… */}
      <circle cx="168" cy="64" r="14" className="pl-f-acc" />
      <g style={{ color: "var(--on-acc)" }}>
        <IconDeviceGamepad2 x={160} y={56} size={16} stroke={2} />
      </g>
      {/* … et il fait tourner le serveur */}
      <circle cx="232" cy="64" r="14" className="pl-f-acc" />
      <g style={{ color: "var(--on-acc)" }}>
        <IconServer2 x={224} y={56} size={16} stroke={2} />
      </g>
      <circle cx="244" cy="54" r="4" className="pl-f-ok pl-pouls" />
      <Legende x={200} texte="Ton PC" />
      <text x="200" y="176" textAnchor="middle" fontSize="10.5" fontWeight="500" className="pl-f-gris">
        Tu joues + il héberge
      </text>

      <circle cx="314" cy="96" r="30" className="pl-f-rond" />
      <g className="pl-c-encre">
        <IconUser x={298} y={80} size={32} stroke={1.4} />
      </g>
      <Legende x={314} texte="Un ami" />
    </svg>
  );
}

// Machine dédiée : la machine tourne jour et nuit. La nuit, ton PC dort, tes amis jouent toujours.
function SceneMachine() {
  return (
    <svg viewBox="0 8 400 200" preserveAspectRatio="xMidYMid meet" aria-hidden>
      <defs>
        <linearGradient id="pl-jour-b" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" className="pl-st-jour-1" />
          <stop offset="1" className="pl-st-jour-2" />
        </linearGradient>
        <linearGradient id="pl-nuit" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" className="pl-st-nuit-1" />
          <stop offset="1" className="pl-st-nuit-2" />
        </linearGradient>
      </defs>
      <rect x="-300" y="-150" width="1000" height="500" fill="url(#pl-jour-b)" />
      <rect x="-300" y="-150" width="1000" height="500" fill="url(#pl-nuit)" className="pl-nuit" />
      <g className="pl-c-soleil pl-jour-seul">
        <IconSun x={187} y={14} size={26} stroke={1.8} />
      </g>
      <g className="pl-c-lune pl-nuit">
        <IconMoon x={187} y={14} size={26} stroke={1.8} />
      </g>

      <g className="pl-trait">
        <g className="pl-jour-seul">
          <Lien x1={118} x2={156} />
          <circle className="pl-point c-vers-g" r="3.4" fill="currentColor" />
        </g>
        <Lien x1={244} x2={282} />
        <circle className="pl-point c-vers-d" r="3.4" fill="currentColor" />
        <circle className="pl-point c-vers-d b" r="3.4" fill="currentColor" />
      </g>

      <g className="pl-pc">
        <circle cx="86" cy="96" r="30" className="pl-f-rond" />
        <g className="pl-c-encre">
          <IconDeviceLaptop x={70} y={80} size={32} stroke={1.4} />
        </g>
      </g>
      <g className="pl-dodo">
        <circle cx="110" cy="70" r="12" className="pl-f-rond" />
        <g className="pl-c-acc">
          <IconZzz x={102} y={62} size={16} stroke={2.2} />
        </g>
      </g>
      <Legende x={86} texte="Ton PC" nuit />

      <circle cx="200" cy="96" r="42" className="pl-f-rond" />
      <g className="pl-c-encre">
        <IconServer2 x={176} y={72} size={48} stroke={1.3} />
      </g>
      <circle cx="232" cy="64" r="14" className="pl-f-acc" />
      <g style={{ color: "var(--on-acc)" }}>
        <IconUsb x={224} y={56} size={16} stroke={2} />
      </g>
      <circle cx="244" cy="54" r="4" className="pl-f-ok pl-pouls" />
      <Legende x={200} texte="Allumée 24 h/24" nuit />

      <circle cx="314" cy="96" r="30" className="pl-f-rond" />
      <g className="pl-c-encre">
        <IconUsers x={298} y={80} size={32} stroke={1.4} />
      </g>
      <Legende x={314} texte="Tes amis" nuit />
    </svg>
  );
}
