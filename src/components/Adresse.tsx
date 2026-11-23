import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { IconCheck, IconCopy, IconPencil } from "@tabler/icons-react";
import { adresseLibre, changerAdresse, motAdresse, type EtatAdresse } from "../api";
import type { Acces, Serveur } from "../types";

const SUFFIXE = ".bhost.app";

type Props = {
  serveur: Serveur;
  acces?: Acces; // Ouvert à tes amis (absent = fermé)
  onInfo: (texte: string) => void;
  onErreur: (texte: string) => void;
};

// La case « Adresse » de l’Accueil. Par le relais (« survie.bhost.app ») : un clic dans la case, et on tape
// le mot. « Libre », ou « Pris · survie2 ? » s’affiche dedans ; Entrée ou OK change l’adresse, Échap annule.
export default function Adresse({ serveur, acces, onInfo, onErreur }: Props) {
  const pourAmis = serveur.ouvertAmis ? acces : undefined;
  const publique = pourAmis?.etat === "ouvert" ? pourAmis.adresse : undefined;
  const texte = publique
    ? publique
    : !serveur.ouvertAmis
      ? "Fermé · local uniquement"
      : pourAmis?.etat === "erreur"
        ? "Fermé · voir Paramètres"
        : "Ouverture de ta box…";
  const modifiable = !!publique?.endsWith(SUFFIXE);
  const actuel = modifiable ? publique!.slice(0, -SUFFIXE.length) : "";

  const [mot, setMot] = useState<string | null>(null); // null : pas en train de modifier
  const [verif, setVerif] = useState<EtatAdresse | null>(null); // null : vérification en cours
  const [envoi, setEnvoi] = useState(false);
  const champ = useRef<HTMLInputElement>(null);
  const boite = useRef<HTMLDivElement>(null);

  // Un autre serveur, ou l’adresse n’est plus par le relais : on arrête de modifier.
  useEffect(() => setMot(null), [serveur.id, modifiable]);

  // Le relais dit si le mot est libre (un peu après la dernière touche).
  useEffect(() => {
    if (mot === null) return;
    if (mot === actuel) {
      setVerif({ etat: "actuelle" });
      return;
    }
    setVerif(null);
    let fini = false;
    const minuterie = window.setTimeout(() => {
      adresseLibre(serveur.id, mot).then(
        (v) => !fini && setVerif(v),
        // Une machine dédiée qui ne connaît pas encore cette commande : son agent est trop ancien.
        (e) => !fini && setVerif({ etat: typeof e === "string" && e.includes("inconnue") ? "ancienne" : "injoignable" }),
      );
    }, 350);
    return () => {
      fini = true;
      window.clearTimeout(minuterie);
    };
  }, [mot, actuel, serveur.id]);

  const copier = () => {
    if (publique) navigator.clipboard.writeText(publique).then(() => onInfo(`« ${publique} » copié`));
  };
  const commencer = () => {
    if (!modifiable) return;
    setMot(actuel);
    requestAnimationFrame(() => champ.current?.select());
  };
  const possible = mot !== null && !envoi && (verif?.etat === "libre" || verif?.etat === "actuelle");
  const valider = (choisi = mot) => {
    if (choisi === null || envoi) return;
    if (choisi === actuel) {
      setMot(null);
      return;
    }
    setEnvoi(true);
    changerAdresse(serveur.id, choisi).then(
      () => {
        setEnvoi(false);
        setMot(null);
        onInfo(`Nouvelle adresse : ${choisi}${SUFFIXE}`);
      },
      (e) => {
        setEnvoi(false);
        onErreur(typeof e === "string" ? e : "Ça n’a pas marché.");
      },
    );
  };
  const touche = (e: KeyboardEvent) => {
    if (e.key === "Escape") setMot(null);
    if (e.key === "Enter" && possible) valider();
  };

  if (mot === null) {
    return (
      <>
        {modifiable ? (
          <button type="button" id="addr" className="field adr-case" title="Changer l’adresse" onClick={commencer}>
            <span>{texte}</span>
            <IconPencil size={13} stroke={2} className="adr-crayon" aria-hidden />
          </button>
        ) : (
          <input
            id="addr"
            className={"field" + (publique ? "" : " is-off")}
            type="text"
            readOnly
            value={texte}
            title={pourAmis?.etat === "erreur" ? pourAmis.message : undefined}
          />
        )}
        <button type="button" className="btn acc" disabled={!publique} onClick={copier}>
          <IconCopy size={14} stroke={2} aria-hidden />
          Copier
        </button>
      </>
    );
  }

  // Ce qui s’affiche à droite, dans la case.
  let etat = null;
  if (verif === null) etat = <span className="adr-st">…</span>;
  else if (verif.etat === "libre")
    etat = (
      <span className="adr-st libre">
        <IconCheck size={12} stroke={2.6} aria-hidden /> Libre
      </span>
    );
  else if (verif.etat === "pris")
    etat = verif.proposition ? (
      <span className="adr-st">
        Pris ·{" "}
        <button type="button" className="adr-prop" onClick={() => setMot(verif.proposition!)}>
          {verif.proposition}
        </button>{" "}
        ?
      </span>
    ) : (
      <span className="adr-st">Pris</span>
    );
  else if (verif.etat === "invalide") etat = <span className="adr-st">3 à 32 lettres</span>;
  else if (verif.etat === "trop") etat = <span className="adr-st">Trop d’essais</span>;
  else if (verif.etat === "injoignable") etat = <span className="adr-st">Relais muet</span>;
  else if (verif.etat === "ancienne") etat = <span className="adr-st">Machine à mettre à jour</span>;

  return (
    <div
      ref={boite}
      className="adr-edit"
      onBlur={(e) => {
        // Un clic ailleurs : on annule (pas sur OK ni sur la proposition).
        if (!envoi && !boite.current?.contains(e.relatedTarget as Node | null)) setMot(null);
      }}
    >
      <label className="field adr-case edit">
        <input
          ref={champ}
          id="addr"
          className="adr-mot"
          value={mot}
          style={{ width: `${Math.max(mot.length, 1)}ch` }}
          maxLength={32}
          spellCheck={false}
          autoComplete="off"
          aria-label="Adresse"
          onChange={(e) => setMot(motAdresse(e.target.value))}
          onKeyDown={touche}
          disabled={envoi}
        />
        <span className="adr-suf">{SUFFIXE}</span>
        {etat}
      </label>
      <button type="button" className="btn acc" disabled={!possible} onClick={() => valider()}>
        {envoi ? "…" : "OK"}
      </button>
    </div>
  );
}
