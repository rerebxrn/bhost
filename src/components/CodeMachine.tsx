import { useEffect, useRef, useState, type FormEvent } from "react";
import { afficherCode, codeComplet, LONGUEUR, nettoyerCode } from "../code";
import { lierMachine } from "../api";
import type { Machine } from "../types";
import "./Menus.css";
import "./CodeMachine.css";

// Le champ du code d'une machine dédiée : « K7M2-Q9XR », le tiret se met tout seul.
export function ChampCode({
  code,
  onChange,
  grand = false,
  autoFocus = false,
}: {
  code: string;
  onChange: (code: string) => void;
  grand?: boolean;
  autoFocus?: boolean;
}) {
  return (
    <input
      className={"code-champ" + (grand ? " grand" : "")}
      type="text"
      inputMode="text"
      maxLength={LONGUEUR + 1}
      spellCheck={false}
      autoComplete="off"
      autoCapitalize="characters"
      aria-label="Code de la machine"
      placeholder="XXXX-XXXX"
      autoFocus={autoFocus}
      value={afficherCode(code)}
      onChange={(e) => onChange(nettoyerCode(e.target.value))}
    />
  );
}

type Props = {
  onAjoutee: (m: Machine) => void;
  onPreparer?: () => void; // « Pas encore de machine ? » : ouvre le tutoriel
  onFermer: () => void;
};

// La petite fenêtre « Ajouter une machine dédiée » : on tape le code affiché sur la machine.
export default function AjouterMachine({ onAjoutee, onPreparer, onFermer }: Props) {
  const [code, setCode] = useState("");
  const [erreur, setErreur] = useState("");
  const [envoi, setEnvoi] = useState(false);
  const [ouvert, setOuvert] = useState(false);
  const forme = useRef<HTMLFormElement>(null);

  useEffect(() => {
    const image = requestAnimationFrame(() => setOuvert(true));
    return () => cancelAnimationFrame(image);
  }, []);

  const ajouter = (e: FormEvent) => {
    e.preventDefault();
    if (!codeComplet(code) || envoi) return;
    setEnvoi(true);
    lierMachine(code)
      .then((m) => {
        onAjoutee(m);
        onFermer();
      })
      .catch((raison) => {
        setErreur(typeof raison === "string" ? raison : "Ça n'a pas marché.");
        setEnvoi(false);
      });
  };

  return (
    <div
      className={"scrim" + (ouvert ? " open" : "")}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !envoi) onFermer();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !envoi) onFermer();
      }}
    >
      <form ref={forme} className="g dialog code-dlg" data-lens="panel" role="dialog" aria-modal="true" aria-labelledby="am-titre" onSubmit={ajouter}>
        <h4 id="am-titre">Ajouter une machine dédiée</h4>
        <p className="dlg-txt">Tape le code affiché sur la machine.</p>
        <ChampCode
          code={code}
          autoFocus
          onChange={(c) => {
            setCode(c);
            setErreur("");
          }}
        />
        {erreur ? (
          <p className="dlg-err" role="alert">
            {erreur}
          </p>
        ) : (
          <p className="code-aide">Il est aussi dans les réglages de la machine.</p>
        )}
        <div className="dlg-btns">
          <button type="button" className="dlg-cancel" disabled={envoi} onClick={onFermer}>
            Annuler
          </button>
          <button type="submit" className="dlg-ok" disabled={!codeComplet(code) || envoi}>
            {envoi ? "Connexion…" : "Ajouter"}
          </button>
        </div>
        {onPreparer && (
          <p className="code-pied">
            Pas encore de machine ?{" "}
            <button type="button" className="code-lien" onClick={onPreparer}>
              Préparer une machine
            </button>
          </p>
        )}
      </form>
    </div>
  );
}
