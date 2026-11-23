import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { changerIcone } from "../api";
import { FONDS_VALIDES, MODES_VALIDES, type Fond, type Mode } from "./palette";

type Reglages = {
  mode: Mode; // clair, sombre ou auto (suit Windows)
  fond: Fond; // Azur (par défaut) ou Ambre (le thème d’origine)
  teinte: number; // intensité du verre, de 0 (transparent) à 1 (teinté)
  reel: boolean; // « Réfraction réelle » : le vrai Liquid Glass au lieu d’un flou simple
};

type Theme = Reglages & {
  sombre: boolean; // le résultat final : true si on affiche le mode sombre
  changer: (r: Partial<Reglages>) => void;
};

const CLE = "bhost.theme";
const PAR_DEFAUT: Reglages = { mode: "systeme", fond: "azur", teinte: 0.3, reel: true };

// Même logique que le petit script de index.html (à changer ensemble).
// Pour l’instant les réglages sont gardés dans le navigateur intégré.
// Plus tard, ils iront dans %APPDATA%\BHOST\ via Rust.
function lire(): Reglages {
  try {
    const garde = JSON.parse(localStorage.getItem(CLE) ?? "{}");
    // Essais du 20/11 : Lagon et Nuit étaient des modes à part, puis le thème s’appelait « vague ».
    if (garde.fond === "vague") garde.fond = "azur";
    if (garde.mode === "lagon" || garde.mode === "nuit") {
      garde.fond = "azur";
      garde.mode = garde.mode === "nuit" ? "sombre" : "clair";
    }
    return {
      mode: MODES_VALIDES.includes(garde.mode) ? garde.mode : PAR_DEFAUT.mode,
      fond: FONDS_VALIDES.includes(garde.fond) ? garde.fond : PAR_DEFAUT.fond,
      teinte: typeof garde.teinte === "number" ? Math.min(1, Math.max(0, garde.teinte)) : PAR_DEFAUT.teinte,
      reel: typeof garde.reel === "boolean" ? garde.reel : PAR_DEFAUT.reel,
    };
  } catch {
    return PAR_DEFAUT;
  }
}

const requeteSombre = window.matchMedia("(prefers-color-scheme: dark)");
const ThemeContext = createContext<Theme | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [reglages, setReglages] = useState<Reglages>(lire);
  const [systemeSombre, setSystemeSombre] = useState(requeteSombre.matches);

  // Suit Windows en direct quand on passe le système en clair/sombre.
  useEffect(() => {
    const ecoute = (e: MediaQueryListEvent) => setSystemeSombre(e.matches);
    requeteSombre.addEventListener("change", ecoute);
    return () => requeteSombre.removeEventListener("change", ecoute);
  }, []);

  const sombre = reglages.mode === "systeme" ? systemeSombre : reglages.mode === "sombre";

  // Applique le thème sur la page : data-theme choisit la palette dans theme.css,
  // et --t donne l’intensité du verre.
  useEffect(() => {
    const racine = document.documentElement;
    racine.dataset.theme = sombre ? "dark" : "light";
    // Azur : ses couleurs et son fond se posent par-dessus (data-fond, dans theme.css).
    racine.dataset.fond = reglages.fond;
    racine.style.setProperty("--t", String(reglages.teinte));
    localStorage.setItem(CLE, JSON.stringify(reglages));
  }, [reglages, sombre]);

  // L’icône de BHOST dans Windows suit le thème (au lancement, puis à chaque changement).
  useEffect(() => {
    changerIcone(`${reglages.fond}-${sombre ? "sombre" : "clair"}`).catch(() => {});
  }, [reglages.fond, sombre]);

  const changer = (r: Partial<Reglages>) => setReglages((avant) => ({ ...avant, ...r }));

  return (
    <ThemeContext.Provider value={{ ...reglages, sombre, changer }}>{children}</ThemeContext.Provider>
  );
}

export function useTheme() {
  const t = useContext(ThemeContext);
  if (!t) throw new Error("useTheme doit être utilisé dans <ThemeProvider>");
  return t;
}
