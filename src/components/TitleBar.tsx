import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconMoon, IconSun } from "@tabler/icons-react";
import { useTheme } from "../theme/ThemeProvider";
import LogoBhost from "./LogoBhost";
import "./TitleBar.css";

const fenetre = getCurrentWindow();

// Barre de titre maison : on retire celle de Windows pour garder le style BHOST,
// mais on garde les vrais boutons − ▢ ✕ de Windows 11.
export default function TitleBar() {
  const [agrandie, setAgrandie] = useState(false);
  const theme = useTheme();

  // Quand la fenêtre change de taille, on vérifie si elle est agrandie
  // pour afficher la bonne icône (▢ ou deux carrés).
  useEffect(() => {
    fenetre.isMaximized().then(setAgrandie);
    const arret = fenetre.onResized(() => {
      fenetre.isMaximized().then(setAgrandie);
    });
    return () => {
      arret.then((f) => f());
    };
  }, []);

  // Fenêtre agrandie : pas de liseré clair sur les bords (il faisait une ligne en haut de l’écran).
  useEffect(() => {
    document.documentElement.classList.toggle("agrandie", agrandie);
  }, [agrandie]);

  return (
    // data-tauri-drag-region : on peut déplacer la fenêtre en tirant ici,
    // et un double-clic l’agrandit, comme une fenêtre Windows normale.
    <header className="titlebar" data-tauri-drag-region>
      <div className="tb-app">
        <LogoBhost hauteur={18} />
      </div>

      <div className="tb-right">
        {/* Bascule rapide clair / sombre : garde le thème choisi (Ambre ou Azur). */}
        <div className={"g seg" + (theme.sombre ? " sombre" : "")} role="group" aria-label="Apparence">
          <span className="seg-pouce" aria-hidden />
          <button
            type="button"
            className={theme.sombre ? "" : "on"}
            aria-label="Mode clair"
            aria-pressed={!theme.sombre}
            onClick={() => theme.changer({ mode: "clair" })}
          >
            <IconSun size={15} stroke={2} aria-hidden />
          </button>
          <button
            type="button"
            className={theme.sombre ? "on" : ""}
            aria-label="Mode sombre"
            aria-pressed={theme.sombre}
            onClick={() => theme.changer({ mode: "sombre" })}
          >
            <IconMoon size={15} stroke={2} aria-hidden />
          </button>
        </div>

        <div className="wc-group">
          <button type="button" className="wc" aria-label="Réduire" onClick={() => fenetre.minimize()}>
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <path d="M0 5.5h10" stroke="currentColor" strokeWidth="1" />
            </svg>
          </button>
          <button
            type="button"
            className="wc"
            aria-label={agrandie ? "Restaurer" : "Agrandir"}
            onClick={() => fenetre.toggleMaximize()}
          >
            {agrandie ? (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <rect
                  x=".5"
                  y="2.5"
                  width="7"
                  height="7"
                  rx="1.2"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1"
                />
                <path
                  d="M2.5 2.5V1.7A1.2 1.2 0 0 1 3.7.5h4.6a1.2 1.2 0 0 1 1.2 1.2v4.6a1.2 1.2 0 0 1-1.2 1.2h-.8"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1"
                />
              </svg>
            ) : (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                <rect
                  x=".5"
                  y=".5"
                  width="9"
                  height="9"
                  rx="1.5"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1"
                />
              </svg>
            )}
          </button>
          <button type="button" className="wc close" aria-label="Fermer" onClick={() => fenetre.close()}>
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <path d="M.5.5l9 9M9.5.5l-9 9" stroke="currentColor" strokeWidth="1" />
            </svg>
          </button>
        </div>
      </div>
    </header>
  );
}
