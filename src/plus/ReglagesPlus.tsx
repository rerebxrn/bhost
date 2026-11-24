import { useState } from "react";
import { IconAlertTriangle, IconDeviceDesktop, IconExternalLink, IconSparkles, IconWifiOff } from "@tabler/icons-react";
import Confirmer from "../components/Confirmer";
import { activerPlus, libererPc, ouvrirPagePlus, pcsPlus, retirerPlus, type PcPlus } from "../api";
import { noterPlus, ouvrirPlus, usePlus } from "./etat";
import "./Plus.css";

const date = (ms: number) => new Date(ms).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" });
const jour = (ms: number) => new Date(ms).toLocaleDateString("fr-FR", { day: "numeric", month: "short" });

// Réglages › BHOST+ (premier groupe des Réglages). Caché tant que BHOST+ n’est pas en vente.
export default function ReglagesPlus() {
  const plus = usePlus();
  const [cle, setCle] = useState("");
  const [erreur, setErreur] = useState("");
  const [enCours, setEnCours] = useState(false);
  // La clé collée est déjà sur 3 PC : ses PC, pour en libérer un avant d’activer celui-ci.
  const [pleine, setPleine] = useState<PcPlus[] | null>(null);
  const [aRetirer, setARetirer] = useState(false);
  const [aLiberer, setALiberer] = useState<PcPlus | null>(null);
  if (!plus?.disponible) return null;

  const { etat } = plus;
  const actif = etat === "actif" || etat === "retard" || etat === "pause";

  const faire = (travail: Promise<unknown>) => {
    setErreur("");
    setEnCours(true);
    return travail.catch((e) => setErreur(String(e))).finally(() => setEnCours(false));
  };
  const activer = () =>
    faire(
      activerPlus(cle)
        .then((v) => (noterPlus(v), setCle(""), setPleine(null)))
        .catch((e) => {
          if (e !== "PLEINE") throw e;
          return pcsPlus(cle).then(setPleine);
        }),
    );
  const espace = () => faire(ouvrirPagePlus("espace"));
  // Libérer un autre PC : depuis ce PC abonné, ou avec la clé collée (puis on active celui-ci).
  const liberer = (pc: PcPlus) =>
    faire(pleine ? libererPc(cle, pc.id).then(() => activerPlus(cle)).then((v) => (noterPlus(v), setCle(""), setPleine(null))) : libererPc(null, pc.id).then(noterPlus));

  const pcs = pleine ?? plus.pcs;
  const places = pleine ? 3 : (plus.places ?? 3);
  const sousTitre = {
    aucun: "Thème Ambre, 40 icônes, relais prioritaire, nouveautés en avance.",
    actif: plus.fin
      ? `${plus.formule === "an" ? "Annuel" : plus.formule === "mois" ? "Mensuel" : "Abonné"} · ${plus.arrete ? "s’arrête" : "renouvelé"} le ${date(plus.fin)}`
      : "Merci de soutenir BHOST.",
    retard: "Ta carte a été refusée. Mets-la à jour pour garder BHOST+.",
    pause: "Thème et icônes en pause.",
    expire: "Thème et icônes repassés en gratuit.",
  }[etat];

  return (
    <>
      <div className="grp-t">BHOST+</div>
      {(etat === "actif" || etat === "retard") && plus.joursSansInternet !== null && (
        <div className="plus-bandeau plus-alerte" role="status">
          <IconWifiOff size={15} stroke={2} aria-hidden />
          <span>
            BHOST+ n’a pas pu vérifier ta clé depuis 3 jours. Reconnecte-toi à Internet d’ici {plus.joursSansInternet}{" "}
            jour{plus.joursSansInternet > 1 ? "s" : ""} pour le garder.
          </span>
        </div>
      )}
      {etat === "pause" && (
        <div className="plus-bandeau plus-alerte" role="status">
          <IconWifiOff size={15} stroke={2} aria-hidden />
          <span>BHOST+ en pause : pas d’Internet depuis 7 jours. Il revient dès que BHOST se reconnecte.</span>
        </div>
      )}
      {pleine && (
        <div className="plus-bandeau plus-alerte" role="alert">
          <IconAlertTriangle size={15} stroke={2} aria-hidden />
          <span>Les 3 places sont prises. Libère-en une pour activer ce PC.</span>
        </div>
      )}
      <section className="surf plus-alerte">
        {/* L’état de l’abonnement */}
        <div className="row">
          <div className="plus-etat">
            <span className={"pi" + (etat === "aucun" ? " gris" : etat === "actif" ? "" : " orange")}>
              {etat === "aucun" || etat === "actif" ? <IconSparkles size={17} stroke={2} aria-hidden /> : <IconAlertTriangle size={17} stroke={2} aria-hidden />}
            </span>
            <div className="lbl">
              <span>{{ aucun: "BHOST+", actif: "BHOST+ actif", retard: "Paiement en retard", pause: "BHOST+ en pause", expire: "BHOST+ expiré" }[etat]}</span>
              <span className={"t2" + (etat === "actif" || etat === "aucun" ? "" : " plus-orange")}>{sousTitre}</span>
            </div>
          </div>
          {etat === "aucun" ? (
            <button type="button" className="btn tint" onClick={ouvrirPlus}>
              Découvrir
            </button>
          ) : etat === "expire" ? (
            <button type="button" className="btn acc" onClick={ouvrirPlus}>
              Renouveler
            </button>
          ) : (
            <button type="button" className={"btn" + (etat === "retard" ? " acc" : "")} onClick={espace} disabled={enCours}>
              {etat === "retard" ? "Mettre à jour" : "Gérer l’abonnement"}
              <IconExternalLink size={14} stroke={2} aria-hidden />
            </button>
          )}
        </div>

        {/* Pas de clé valable sur ce PC : on la colle ici. */}
        {!actif && (
          <div className="row">
            <div className="lbl">
              <label htmlFor="plus-cle">Clé</label>
              {erreur ? (
                <span className="t2 plus-err" role="alert">
                  {erreur}
                </span>
              ) : (
                <span className="t2">Reçue par email après l’abonnement.</span>
              )}
            </div>
            <form
              className="rg-btns"
              onSubmit={(e) => {
                e.preventDefault();
                if (cle.trim()) activer();
              }}
            >
              <input
                id="plus-cle"
                className={"field plus-cle" + (erreur ? " faux" : "")}
                type="text"
                value={cle}
                placeholder="Colle ta clé"
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => {
                  setCle(e.target.value);
                  setErreur("");
                  setPleine(null);
                }}
              />
              <button type="submit" className={"btn" + (cle.trim() && !pleine ? " acc" : "")} disabled={!cle.trim() || enCours || !!pleine}>
                Activer
              </button>
            </form>
          </div>
        )}

        {/* Les places : 3 PC par clé. Sans réponse du service BHOST+ : seulement ce PC. */}
        {(actif || pleine) && pcs.length > 0 && (
          <div className="row">
            <div className="lbl">
              <span>Places</span>
              <span className="t2">
                {pcs.length} sur {places}
                {!pleine && plus.cle ? ` · clé ${plus.cle}` : ""}
              </span>
            </div>
            <span className="plus-places" aria-label={`${pcs.length} places prises sur ${places}`}>
              {Array.from({ length: places }, (_, i) => (
                <i key={i} className={i < pcs.length ? "pris" : ""} />
              ))}
            </span>
          </div>
        )}
        {(actif || pleine) &&
          pcs.map((pc) => (
            <div className="row" key={pc.id}>
              <div className="plus-app">
                <IconDeviceDesktop size={16} stroke={2} aria-hidden />
                <div className="lbl">
                  <span>
                    {pc.nom}
                    {pc.ici && <span className="badge-ici">Ce PC</span>}
                  </span>
                  <span className="t2">{pc.depuis ? `Activé le ${jour(pc.depuis)}` : "Activé"}</span>
                </div>
              </div>
              <button type="button" className="btn" disabled={enCours} onClick={() => (pc.ici ? setARetirer(true) : setALiberer(pc))}>
                {pc.ici ? "Retirer de ce PC…" : "Libérer…"}
              </button>
            </div>
          ))}
        {actif && pcs.length === 0 && (
          <div className="row">
            <div className="plus-app">
              <IconDeviceDesktop size={16} stroke={2} aria-hidden />
              <div className="lbl">
                <span>
                  Ce PC<span className="badge-ici">Activé</span>
                </span>
                <span className="t2 rg-chemin">{plus.cle}</span>
              </div>
            </div>
            <button type="button" className="btn" disabled={enCours} onClick={() => setARetirer(true)}>
              Retirer de ce PC…
            </button>
          </div>
        )}
        {actif && erreur && (
          <div className="row">
            <span className="t2 plus-err" role="alert">
              {erreur}
            </span>
          </div>
        )}
      </section>

      {aRetirer && (
        <Confirmer
          titre="Retirer BHOST+ de ce PC ?"
          texte="La place se libère pour un autre PC. Ici, le thème et les icônes repassent en gratuit."
          bouton="Retirer"
          onOui={() => faire(retirerPlus().then(noterPlus))}
          onFermer={() => setARetirer(false)}
        />
      )}
      {aLiberer && (
        <Confirmer
          titre={`Libérer ${aLiberer.nom} ?`}
          texte={pleine ? "Il perd BHOST+, et ce PC prend sa place." : "Il perd BHOST+. Sa place revient à un autre PC."}
          bouton="Libérer"
          onOui={() => liberer(aLiberer)}
          onFermer={() => setALiberer(null)}
        />
      )}
    </>
  );
}
