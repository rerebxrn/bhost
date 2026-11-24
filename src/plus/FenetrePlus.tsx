import { useState } from "react";
import { IconBolt, IconExternalLink, IconIcons, IconPalette, IconRocket } from "@tabler/icons-react";
import Fenetre from "../components/Fenetre";
import LogoBhost from "../components/LogoBhost";
import Segment from "../components/Segment";
import { ouvrirPagePlus } from "../api";
import { usePlus } from "./etat";
import "./Plus.css";

type Props = { onCle: () => void; onFermer: () => void };

const GAINS = [
  { ic: IconPalette, nom: "Thème Ambre", desc: "En clair et en sombre." },
  { ic: IconIcons, nom: "40 icônes en plus", desc: "Fête, gourmandises, objets, nature." },
  { ic: IconBolt, nom: "Relais prioritaire", desc: "Tes amis entrent même quand le relais est plein." },
  { ic: IconRocket, nom: "Nouveautés en avance", desc: "Les nouvelles versions avant tout le monde." },
];

// La fenêtre BHOST+ (clic sur le bouton de la barre de titre, ou sur un cadenas).
export default function FenetrePlus({ onCle, onFermer }: Props) {
  const plus = usePlus();
  const [duree, setDuree] = useState<"mois" | "an">("an");
  const [erreur, setErreur] = useState("");
  const expire = plus?.etat === "expire";

  const sAbonner = () => {
    setErreur("");
    ouvrirPagePlus(duree === "an" ? "annuel" : "mensuel").catch((e) => setErreur(String(e)));
  };

  return (
    <Fenetre titre="BHOST+" classe="plus-dlg" onFermer={onFermer}>
      {/* Le logo, avec le « + » à la couleur du thème */}
      <h4 className="plus-logo">
        <LogoBhost hauteur={26} plus />
      </h4>
      <p className="dlg-txt">{expire ? "Ton abonnement a expiré. Renouvelle pour tout retrouver." : "Soutiens BHOST. Le reste reste gratuit."}</p>

      <ul className="plus-liste">
        {GAINS.map((g) => (
          <li key={g.nom}>
            <span className="pi">
              <g.ic size={17} stroke={2} aria-hidden />
            </span>
            <span className="rs-t">
              <b>{g.nom}</b>
              <span>{g.desc}</span>
            </span>
          </li>
        ))}
      </ul>

      <div className="plus-prix">
        <div className="pp-v">
          <b className="tnum">
            {duree === "an" ? "30 €" : "3 €"}
            <small>{duree === "an" ? "par an" : "par mois"}</small>
          </b>
          <span>{duree === "an" ? "2 mois offerts" : "Sans engagement"}</span>
        </div>
        <Segment
          etiquette="Durée"
          options={[
            { id: "mois", nom: "Mensuel" },
            { id: "an", nom: "Annuel" },
          ]}
          valeur={duree}
          onChoisir={setDuree}
        />
      </div>
      <p className={"plus-note" + (erreur ? " plus-err" : "")} role={erreur ? "alert" : undefined}>
        {erreur || "Une clé pour 3 PC. Sans compte : elle arrive par email."}
      </p>

      <div className="dlg-btns">
        <button type="button" className="dlg-cancel" onClick={onCle}>
          J’ai déjà une clé
        </button>
        <button type="button" className="dlg-ok" onClick={sAbonner}>
          {expire ? "Renouveler" : "S’abonner"}
          <IconExternalLink size={14} stroke={2} aria-hidden />
        </button>
      </div>
    </Fenetre>
  );
}
