// L’état de BHOST+, partagé par toute l’interface (lu une fois, puis suivi par l’event « bhost-plus »).
import { useSyncExternalStore } from "react";
import { ecouterPlus, etatPlus, type VuePlus } from "../api";

let vue: VuePlus | null = null;
const abonnes = new Set<() => void>();

export function noterPlus(v: VuePlus) {
  vue = v;
  abonnes.forEach((f) => f());
}

let suivi = false;
function suivre() {
  if (suivi) return;
  suivi = true;
  etatPlus().then(noterPlus, () => {});
  ecouterPlus(noterPlus);
}

function abonner(f: () => void) {
  suivre();
  abonnes.add(f);
  return () => abonnes.delete(f);
}

// null tant que la partie Rust n’a pas répondu.
export function usePlus() {
  return useSyncExternalStore(abonner, () => vue);
}

// Les avantages sont ouverts (abonné, ou BHOST+ pas encore en vente : rien n’est fermé).
export const ouvert = (v: VuePlus | null) => !v || !v.disponible || abonne(v);

// Abonné : actif, ou paiement en retard (Polar réessaie, tout reste ouvert).
export const abonne = (v: VuePlus | null) => v?.etat === "actif" || v?.etat === "retard";

// Ouvre la fenêtre BHOST+ (bouton de la barre de titre, cadenas…), ou Réglages › BHOST+ si abonné.
export const ouvrirPlus = () => window.dispatchEvent(new Event("ouvrir-bhost-plus"));
