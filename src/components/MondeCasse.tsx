import { useEffect, useState } from "react";
import { IconAlertTriangle } from "@tabler/icons-react";
import { datapacksManquants, ecouterExtensions, remettreDatapack, type Manquant } from "../api";
import "./MondeCasse.css";

type Props = {
  id: string; // le serveur
  manquants?: Manquant[]; // déjà connus (onglet Datapacks) ; absent : on les demande (Accueil)
  onInfo: (texte: string) => void;
  onErreur: (texte: string) => void;
};

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n’a pas marché.");
const liste = (noms: string[]) => (noms.length > 1 ? `${noms.slice(0, -1).join(", ")} et ${noms[noms.length - 1]}` : noms[0]);

// Le serveur ne démarre plus : son monde attend un datapack qui a été retiré (Terralith…).
// Un bouton le remet (rallumé, ou réinstallé depuis le catalogue).
export default function MondeCasse({ id, manquants, onInfo, onErreur }: Props) {
  const [lus, setLus] = useState<Manquant[]>([]);
  const [occupe, setOccupe] = useState(false);
  const soi = manquants === undefined;

  useEffect(() => {
    if (!soi) return;
    const lire = () => datapacksManquants(id).then(setLus, () => setLus([]));
    lire();
    const arreter = ecouterExtensions((qui) => qui === id && lire());
    return arreter;
  }, [id, soi]);

  const presents = manquants ?? lus;
  if (!presents.length) return null;
  const noms = presents.map((m) => m.nom);

  const remettre = async () => {
    onErreur("");
    setOccupe(true);
    try {
      for (const m of presents) await remettreDatapack(id, m.fichier);
      onInfo(`${liste(noms)} remis. Relance le serveur.`);
      if (soi) setLus([]);
    } catch (e) {
      onErreur(phrase(e));
    } finally {
      setOccupe(false);
    }
  };

  return (
    <div className="monde-casse" role="alert">
      <IconAlertTriangle size={18} stroke={2} aria-hidden />
      <div>
        <b>Le monde ne se charge plus</b>
        <span>
          Il lui manque {liste(noms)}, {noms.length > 1 ? "qui ont créé" : "qui a créé"} son terrain.
        </span>
      </div>
      <button type="button" className="monde-casse-bt" disabled={occupe} onClick={remettre}>
        {occupe ? "Installation…" : noms.length > 1 ? "Tout remettre" : `Remettre ${noms[0]}`}
      </button>
    </div>
  );
}
