import { useEffect, useRef } from "react";
import { demarrerVerre } from "./lens";
import { demarrerGel } from "./gel";

// Branche le moteur Liquid Glass sur toute l'appli.
// actif : le réglage « Réfraction réelle ». teinte : l'intensité du verre.
// sombre : le thème affiché (les reflets sont plus discrets en sombre).
export function useLiquidGlass(actif: boolean, teinte: number, sombre: boolean) {
  const moteur = useRef<ReturnType<typeof demarrerVerre> | null>(null);

  useEffect(() => {
    if (!actif) return;
    const m = demarrerVerre(document.body);
    moteur.current = m;
    document.documentElement.classList.add("lg-on");
    return () => {
      document.documentElement.classList.remove("lg-on");
      moteur.current = null;
      m.arreter();
    };
  }, [actif]);

  // Refaire les filtres de verre coûte cher : pendant qu'on fait glisser « Liquid Glass »,
  // on attend que le curseur s'arrête (le fond du verre, lui, suit tout de suite grâce à --t).
  useEffect(() => {
    const minuterie = window.setTimeout(() => moteur.current?.rafraichir(), 300);
    return () => window.clearTimeout(minuterie);
  }, [teinte, sombre]);

  // L'effet gel et le reflet qui suit la souris marchent toujours, avec ou sans réfraction.
  useEffect(() => demarrerGel(document.body), []);
}
