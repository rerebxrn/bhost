import { IconAlertTriangle, IconSparkles } from "@tabler/icons-react";
import { ouvrirPlus, usePlus } from "./etat";
import "./Plus.css";

// Le bouton BHOST+ de la barre de titre, juste avant la bascule clair / sombre.
// Abonné : pas de bouton, le logo devient « BHOST+ » (paiement en retard : le « + » reste, avec un bouton orange).
export default function BoutonPlus() {
  const plus = usePlus();
  if (!plus?.disponible || plus.etat === "actif") return null;
  if (plus.etat !== "aucun") {
    return (
      <button type="button" className="plus-b alerte" onClick={ouvrirPlus}>
        <IconAlertTriangle size={15} stroke={2} aria-hidden />
        {{ retard: "Paiement en retard", expire: "BHOST+ expiré", pause: "BHOST+ en pause" }[plus.etat]}
      </button>
    );
  }
  return (
    <button type="button" className="plus-b" onClick={ouvrirPlus}>
      <IconSparkles size={15} stroke={2} aria-hidden />
      BHOST+
    </button>
  );
}
