import { useState, type ReactNode } from "react";
import {
  IconAlertCircle,
  IconArrowRight,
  IconCheck,
  IconCircleCheck,
  IconDeviceGamepad2,
  IconDeviceLaptop,
  IconDeviceMobile,
  IconMoon,
  IconServer,
  IconSun,
  IconUsb,
  IconUsers,
  IconZzz,
} from "@tabler/icons-react";
import "./PremierLancement.css";

// Premier lancement : on choisit où héberger ses serveurs.
// « Sur cet ordinateur » ouvre l'appli. La machine dédiée (BHOST OS) n'est pas encore prête.
type Choix = "local" | "machine";

export default function PremierLancement({ onLocal }: { onLocal: () => void }) {
  const [choix, setChoix] = useState<Choix>("local");
  const [pasPret, setPasPret] = useState(false);

  const choisir = (c: Choix) => {
    setChoix(c);
    setPasPret(false);
  };

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
            onClick={() => (choix === "local" ? onLocal() : setPasPret(true))}
          >
            {choix === "local" ? "Héberger sur ce PC" : "Préparer la machine"}
            <IconArrowRight size={17} stroke={2.2} aria-hidden />
          </button>
          <p className="pl-note" role="status" aria-live="polite">
            {pasPret
              ? "La machine dédiée arrive dans une prochaine version. Commence sur ce PC en attendant."
              : "Pas sûr ? Commence ici. Tu pourras passer sur une machine dédiée plus tard."}
          </p>
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
function Tuile() {
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

// Une tête de joueur, toute simple.
function Tete({ classe, peau, yeux }: { classe: string; peau: string; yeux: string }) {
  return (
    <g className={classe}>
      <rect width="18" height="18" rx="4" fill={peau} />
      <rect x="4" y="7" width="3.5" height="3.5" fill={yeux} />
      <rect x="10.5" y="7" width="3.5" height="3.5" fill={yeux} />
      <rect x="5.5" y="12.5" width="7" height="2" rx="1" fill={yeux} opacity=".35" />
    </g>
  );
}

const COURBE = "0,22 8,16 16,19 24,10 32,14 40,6 48,12 56,8 64,15 72,9 80,13 88,5 96,11 104,8 112,16 120,10 128,13 136,6 144,12 152,9 160,22";

// Sur ce PC : tu joues, tes amis arrivent sur ton serveur.
function SceneLocal() {
  return (
    <svg viewBox="0 0 400 200" preserveAspectRatio="xMidYMid slice" aria-hidden>
      <defs>
        <linearGradient id="pl-jour-a" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" className="pl-st-jour-1" />
          <stop offset="1" className="pl-st-jour-2" />
        </linearGradient>
        <clipPath id="pl-graphe">
          <rect x="136" y="78" width="76" height="36" rx="6" />
        </clipPath>
      </defs>
      <rect width="400" height="200" fill="url(#pl-jour-a)" />
      <g className="pl-c-soleil">
        <IconSun x={36} y={18} size={44} stroke={1.15} />
      </g>
      <ellipse cx="200" cy="158" rx="54" ry="5" className="pl-f-ombre" />

      <rect x="124" y="32" width="152" height="100" rx="12" className="pl-f-ecran" />
      <rect x="130" y="38" width="140" height="82" rx="7" className="pl-f-vitre" />
      <rect x="136" y="44" width="128" height="28" rx="6" className="pl-f-carte" />
      <rect x="141" y="50" width="16" height="16" rx="4" className="pl-f-acc" />
      <g style={{ color: "var(--on-acc)" }}>
        <IconServer x={143} y={52} size={12} stroke={3.2} />
      </g>
      <rect x="162" y="52" width="40" height="5" rx="2.5" className="pl-f-encre" />
      <rect x="162" y="60" width="26" height="4" rx="2" className="pl-f-gris" />
      <rect x="210" y="51" width="49" height="14" rx="7" className="pl-f-ok" fillOpacity=".16" />
      <circle cx="218" cy="58" r="3" className="pl-f-ok pl-pouls" />
      <text x="237" y="61" textAnchor="middle" fontSize="7.5" fontWeight="700" className="pl-f-ok">
        En ligne
      </text>
      <rect x="136" y="78" width="76" height="36" rx="6" className="pl-f-carte" />
      <g clipPath="url(#pl-graphe)">
        <polyline transform="translate(136 82)" points={COURBE} fill="none" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="pl-s-acc" />
      </g>
      <rect x="216" y="78" width="48" height="36" rx="6" className="pl-f-carte" />
      <g className="pl-c-acc">
        <IconUsers x={222} y={83} size={11} stroke={3.7} />
      </g>
      <g className="pl-compte">
        {[1, 2, 3, 4].map((n) => (
          <text key={n} x="240" y="109" textAnchor="middle" fontSize="15" fontWeight="750" className={`pl-f-encre pl-n n${n}`}>
            {n}
          </text>
        ))}
      </g>
      <rect x="190" y="132" width="20" height="14" className="pl-f-ecran" />
      <rect x="170" y="146" width="60" height="8" rx="4" className="pl-f-ecran" />

      <path d="M92 112 C 104 106 112 100 122 96" fill="none" strokeWidth="2" strokeDasharray="2 5" strokeLinecap="round" className="pl-s-acc" />
      <circle cx="64" cy="114" r="27" className="pl-f-rond" />
      <g className="pl-c-encre">
        <IconDeviceGamepad2 x={48} y={98} size={32} stroke={1.2} />
      </g>
      <text x="64" y="158" textAnchor="middle" fontSize="11" fontWeight="700" className="pl-f-encre">
        Toi
      </text>

      <Tete classe="pl-ami a1" peau="#c98f6b" yeux="#2b1d14" />
      <Tete classe="pl-ami a2" peau="#f0c9a0" yeux="#2b1d14" />
      <Tete classe="pl-ami a3" peau="#8a5a3c" yeux="#f0e2d4" />
      <text x="350" y="182" textAnchor="middle" fontSize="11" fontWeight="700" className="pl-f-encre">
        Tes amis
      </text>
    </svg>
  );
}

const VERS_PC = "M140 96 C 196 64, 236 54, 274 56";
const VERS_TEL = "M140 124 C 206 132, 250 146, 302 146";
const ETOILES: [number, number, number][] = [
  [30, 24, 1.3], [84, 14, 1], [150, 30, 1.2], [246, 18, 1], [318, 40, 1.3], [372, 14, 1], [196, 64, 0.9], [120, 58, 1], [360, 82, 1],
];

// Machine dédiée : le jour passe, la nuit tombe, ton PC dort mais le serveur continue.
function SceneMachine() {
  return (
    <svg viewBox="0 0 400 200" preserveAspectRatio="xMidYMid slice" aria-hidden>
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
      <rect width="400" height="200" fill="url(#pl-jour-b)" />
      <g className="pl-soleil pl-c-soleil">
        <IconSun x={181} y={25} size={38} stroke={1.33} />
      </g>
      <rect width="400" height="200" fill="url(#pl-nuit)" className="pl-nuit" />
      <g className="pl-nuit">
        {ETOILES.map(([x, y, r]) => (
          <circle key={`${x}-${y}`} cx={x} cy={y} r={r} fill="#ffffff" />
        ))}
      </g>
      <g className="pl-lune pl-c-lune">
        <IconMoon x={183} y={27} size={34} stroke={1.48} />
      </g>

      <g className="pl-trait">
        <path d={VERS_PC} fill="none" stroke="currentColor" strokeOpacity=".45" strokeWidth="2" strokeDasharray="2 6" strokeLinecap="round" />
        <path d={VERS_TEL} fill="none" stroke="currentColor" strokeOpacity=".45" strokeWidth="2" strokeDasharray="2 6" strokeLinecap="round" />
        <g className="pl-jour-seul">
          <circle className="pl-paquet p1" r="3.4" fill="currentColor" />
        </g>
        <circle className="pl-paquet p2" r="3.4" fill="currentColor" />
      </g>

      <rect x="46" y="72" width="94" height="104" rx="14" className="pl-f-serveur" stroke="#ffffff" strokeOpacity=".25" strokeWidth="1.5" />
      <rect x="58" y="86" width="70" height="20" rx="5" className="pl-f-baie" />
      <rect x="58" y="112" width="70" height="20" rx="5" className="pl-f-baie" />
      <rect x="66" y="94" width="30" height="4" rx="2" className="pl-f-baie-2" />
      <rect x="66" y="120" width="22" height="4" rx="2" className="pl-f-baie-2" />
      <circle cx="118" cy="96" r="3.2" fill="#5fd38d" />
      <circle cx="118" cy="122" r="3.2" fill="#8fb2ee" />
      <rect x="58" y="154" width="26" height="5" rx="2.5" className="pl-f-baie-2" />
      <rect x="104" y="150" width="24" height="12" rx="3" fill="#000000" fillOpacity=".35" />
      <rect x="108" y="152" width="16" height="8" rx="1.5" fill="#c9d3e3" />
      <rect x="124" y="148" width="30" height="16" rx="5" className="pl-f-rond" />
      <g className="pl-c-acc">
        <IconUsb x={132} y={149} size={14} stroke={3.1} />
      </g>
      <rect x="61" y="60" width="64" height="22" rx="11" className="pl-f-rond" />
      <circle cx="74" cy="71" r="3.4" className="pl-f-ok pl-pouls" />
      <text x="98" y="75" textAnchor="middle" fontSize="10.5" fontWeight="750" className="pl-f-encre">
        24 h/24
      </text>

      <g className="pl-pc">
        <circle cx="302" cy="54" r="27" className="pl-f-rond" />
        <g className="pl-c-encre">
          <IconDeviceLaptop x={286} y={38} size={32} stroke={1.28} />
        </g>
      </g>
      <g className="pl-dodo">
        <rect x="318" y="20" width="30" height="20" rx="10" className="pl-f-rond" />
        <g className="pl-c-acc">
          <IconZzz x={325} y={22} size={16} stroke={2.85} />
        </g>
      </g>
      <text x="302" y="98" textAnchor="middle" fontSize="10.5" fontWeight="700" className="pl-f-encre pl-txt-jour">
        Ton PC
      </text>
      <text x="302" y="98" textAnchor="middle" fontSize="10.5" fontWeight="700" fill="#ffffff" className="pl-txt-nuit">
        Ton PC dort
      </text>

      <circle cx="326" cy="146" r="24" className="pl-f-rond" />
      <g className="pl-c-encre">
        <IconDeviceMobile x={312} y={132} size={28} stroke={1.46} />
      </g>
      <circle cx="343" cy="129" r="5" className="pl-f-ok pl-pouls" stroke="#ffffff" strokeWidth="2" />
      <text x="326" y="184" textAnchor="middle" fontSize="10.5" fontWeight="700" className="pl-etiquette">
        Tes amis jouent
      </text>
    </svg>
  );
}
