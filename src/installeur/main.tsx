// BHOST Setup : même thème que l'appli (Azur par défaut, clair/sombre comme Windows).
import "../theme/theme.css";
import "../theme/glass.css";
import React from "react";
import ReactDOM from "react-dom/client";
import Installeur from "./Installeur";
import { ThemeProvider } from "../theme/ThemeProvider";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <Installeur />
    </ThemeProvider>
  </React.StrictMode>,
);
