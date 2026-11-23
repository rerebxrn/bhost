import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ecouterConsole, lireConsole } from "../api";
import "./Console.css";

const LIGNES_MAX = 2000;

type Ligne = { heure: string; niveau: string; texte: string };

// Découpe une ligne du serveur : « [18:02:36 INFO]: Done » ou « [18:02:36] [Server thread/INFO]: Done ».
function lire(brut: string): Ligne {
  if (brut.startsWith("> ")) return { heure: "", niveau: "CMD", texte: brut };
  // Les messages de BHOST lui-même : « [BHOST!] » pour une erreur, « [BHOST] » pour une information.
  if (brut.startsWith("[BHOST!] ")) return { heure: "", niveau: "ERROR", texte: brut.slice("[BHOST!] ".length) };
  if (brut.startsWith("[BHOST] ")) return { heure: "", niveau: "BHOST", texte: brut.slice("[BHOST] ".length) };
  const m = /^\[(\d\d:\d\d:\d\d)(?:\] \[[^\]]*\/| )([A-Z]+)\]:? ?(.*)$/.exec(brut);
  if (!m) return { heure: "", niveau: "", texte: brut };
  const niveau = m[2] === "WARNING" ? "WARN" : m[2] === "SEVERE" || m[2] === "FATAL" ? "ERROR" : m[2];
  return { heure: m[1], niveau, texte: m[3] };
}

// La console d’un serveur, en direct.
export default function Console({ id }: { id: string }) {
  const [lignes, setLignes] = useState<Ligne[]>([]);
  const journal = useRef<HTMLDivElement>(null);
  const enBas = useRef(true); // on suit le bas tant que l’utilisateur ne remonte pas

  // À l’ouverture : on s’abonne d’abord aux nouvelles lignes, puis on demande les anciennes.
  // Les numéros de ligne évitent d’en perdre une ou d’en afficher une deux fois.
  useEffect(() => {
    let ferme = false;
    let dernier: number | null = null; // numéro de la dernière ligne affichée
    const attente: { n: number; ligne: Ligne }[] = [];
    setLignes([]);
    enBas.current = true;

    const abonnement = ecouterConsole((qui, n, brut) => {
      if (qui !== id || ferme) return;
      const ligne = lire(brut);
      if (dernier === null) attente.push({ n, ligne });
      else if (n > dernier) {
        dernier = n;
        setLignes((l) => [...l, ligne].slice(-LIGNES_MAX));
      }
    });

    abonnement
      .then(() => lireConsole(id))
      .then((passe) => {
        if (ferme) return;
        const suite = attente.filter((a) => a.n > passe.dernier);
        dernier = suite.length ? suite[suite.length - 1].n : passe.dernier;
        setLignes([...passe.lignes.map(lire), ...suite.map((a) => a.ligne)].slice(-LIGNES_MAX));
      })
      // Le passé n’a pas pu être lu (machine lente…) : on montre au moins ce qui arrive maintenant.
      .catch(() => {
        if (ferme) return;
        dernier = attente.length ? attente[attente.length - 1].n : 0;
        setLignes(attente.map((a) => a.ligne).slice(-LIGNES_MAX));
      });

    return () => {
      ferme = true;
      abonnement.then((arreter) => arreter());
    };
  }, [id]);

  // Après chaque nouvelle ligne : on redescend en bas, sauf si l’utilisateur lit plus haut.
  useLayoutEffect(() => {
    const el = journal.current;
    if (el && enBas.current) el.scrollTop = el.scrollHeight;
  }, [lignes]);

  return (
    <section className="console" aria-label="Console">
      <div className="con-h">
        <b>Console</b>
        <span className="tnum">
          {lignes.length} {lignes.length > 1 ? "lignes" : "ligne"}
        </span>
      </div>
      <div
        className="log"
        ref={journal}
        onScroll={(e) => {
          const el = e.currentTarget;
          enBas.current = el.scrollHeight - el.scrollTop - el.clientHeight < 120;
        }}
      >
        {lignes.length === 0 && <div className="con-empty">Rien pour l’instant. Démarre le serveur.</div>}
        {lignes.map((l, i) => (
          <div key={i} className={l.niveau === "CMD" ? "lv-CMD" : undefined}>
            {l.heure && <span className="t">{l.heure} </span>}
            {l.niveau && l.niveau !== "CMD" && <span className={"lv-" + l.niveau}>{l.niveau} </span>}
            {l.texte}
          </div>
        ))}
      </div>
    </section>
  );
}
