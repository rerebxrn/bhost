import { useEffect, useLayoutEffect, useRef, useState, type FormEvent } from "react";
import { IconBan, IconCheck, IconCrown, IconDots, IconDoorExit, IconPlus, IconUserPlus } from "@tabler/icons-react";
import Segment from "../components/Segment";
import Tete from "../components/Tete";
import Bannir from "../components/Bannir";
import FicheJoueur from "../components/FicheJoueur";
import Inviter from "../components/Inviter";
import { actionJoueur, changerListeBlanche, ecouterDemandes, joueursServeur, repondreDemande, type ActionJoueur, type Joueurs as Listes } from "../api";
import { duree, type Acces, type Serveur } from "../types";
import "../components/Inviter.css";
import "../components/Menus.css";
import "./Joueurs.css";

type Props = {
  serveur: Serveur;
  acces?: Acces; // pour l'adresse à partager (fenêtre « Inviter »)
  onErreur: (texte: string) => void;
  onInfo: (texte: string) => void;
};

type Liste = "connectes" | "blanche" | "demandes" | "bannis";

const phrase = (e: unknown) => (typeof e === "string" ? e : "Ça n'a pas marché.");

// « 2026-11-17 18:27:00 +0100 » (format de Minecraft) → « 17 nov. ».
function jour(quand?: string): string {
  const d = quand ? new Date(quand.replace(" ", "T").replace(/ ([+-]\d\d)(\d\d)$/, "$1:$2")) : null;
  return d && !isNaN(d.getTime()) ? d.toLocaleDateString("fr-FR", { day: "numeric", month: "short" }) : "";
}

// « il y a 5 min », « hier à 14:02 », « 17 nov. à 14:02 ».
function ilYa(quand: number, maintenant: number): string {
  const minutes = Math.floor((maintenant - quand) / 60_000);
  if (minutes < 1) return "à l’instant";
  if (minutes < 60) return `il y a ${minutes} min`;
  const d = new Date(quand);
  const heure = d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  const hier = new Date(maintenant);
  hier.setDate(hier.getDate() - 1);
  if (d.toDateString() === new Date(maintenant).toDateString()) return `aujourd’hui à ${heure}`;
  if (d.toDateString() === hier.toDateString()) return `hier à ${heure}`;
  return `${d.toLocaleDateString("fr-FR", { day: "numeric", month: "short" })} à ${heure}`;
}

// L'onglet Joueurs : connectés (avec fiche et actions), liste blanche, demandes, bannis.
// Serveur allumé : tout passe par des commandes. Arrêté : BHOST modifie les fichiers du serveur.
export default function Joueurs({ serveur, acces, onErreur, onInfo }: Props) {
  const [listes, setListes] = useState<Listes | null>(null);
  const [onglet, setOnglet] = useState<Liste>("connectes");
  const [menu, setMenu] = useState<{ nom: string; x: number; y: number } | null>(null);
  const [aBannir, setABannir] = useState<string | null>(null);
  const [fiche, setFiche] = useState<string | null>(null);
  const [ajout, setAjout] = useState("");
  const [inviter, setInviter] = useState<DOMRect | null>(null);
  const [, setTic] = useState(0);
  const allume = serveur.statut === "en-ligne";

  // On relit les listes à l'ouverture, et à chaque arrivée ou départ.
  const connectes = serveur.joueurs.join(",");
  useEffect(() => {
    joueursServeur(serveur.id).then(setListes, (e) => onErreur(phrase(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serveur.id, connectes, serveur.statut]);

  // Une demande arrive (ou part) : on relit tout (la liste blanche a pu changer aussi).
  useEffect(
    () =>
      ecouterDemandes((id) => {
        if (id === serveur.id) joueursServeur(id).then(setListes, () => {});
      }),
    [serveur.id],
  );

  // « depuis 12 min » avance tout seul.
  useEffect(() => {
    const minuterie = window.setInterval(() => setTic((t) => t + 1), 30_000);
    return () => window.clearInterval(minuterie);
  }, []);

  const agir = (action: ActionJoueur, nom: string, texte: string, motif?: string) =>
    actionJoueur(serveur.id, action, nom, motif).then((l) => {
      setListes(l);
      onInfo(texte);
    });
  const essayer = (action: ActionJoueur, nom: string, texte: string) => agir(action, nom, texte).catch((e) => onErreur(phrase(e)));

  const repondre = (nom: string, accepter: boolean) =>
    repondreDemande(serveur.id, nom, accepter).then(
      (l) => {
        setListes(l);
        onInfo(accepter ? `${nom} peut entrer` : `Demande de ${nom} refusée`);
      },
      (e) => onErreur(phrase(e)),
    );

  const estOp = (nom: string) => !!listes?.ops.some((o) => o.toLowerCase() === nom.toLowerCase());
  const basculerOp = (nom: string) =>
    estOp(nom) ? essayer("deop", nom, `${nom} n'est plus OP`) : essayer("op", nom, `${nom} est OP`);

  const ajouter = (e: FormEvent) => {
    e.preventDefault();
    const nom = ajout.trim();
    if (nom.length < 3) return;
    agir("blanc_ajouter", nom, `${nom} ajouté à la liste blanche`)
      .then(() => setAjout(""))
      .catch((er) => onErreur(phrase(er)));
  };

  const onglets = [
    { id: "connectes" as Liste, nom: "Connectés", n: listes?.connectes.length ?? 0 },
    { id: "blanche" as Liste, nom: "Liste blanche", n: listes?.blancs.length ?? 0 },
    { id: "demandes" as Liste, nom: "Demandes", n: listes?.demandes.length ?? 0 },
    { id: "bannis" as Liste, nom: "Bannis", n: listes?.bannis.length ?? 0 },
  ];
  const maintenant = Date.now();
  const joueurFiche = fiche ? listes?.connectes.find((c) => c.nom === fiche) : undefined;

  return (
    <div className="params joueurs">
      <div className="pl-top">
        <Segment etiquette="Listes de joueurs" options={onglets} valeur={onglet} onChoisir={setOnglet} />
        <button type="button" className="pill-acc grand" data-gel onClick={(e) => setInviter(e.currentTarget.getBoundingClientRect())}>
          <IconUserPlus size={13} stroke={2.2} aria-hidden />
          Inviter des amis
        </button>
      </div>

      {listes === null ? (
        <section className="surf">
          <div className="empty">Lecture…</div>
        </section>
      ) : onglet === "connectes" ? (
        <section className="surf">
          {listes.connectes.length === 0 ? (
            <div className="empty">{allume ? "Personne pour l’instant." : "Le serveur est arrêté."}</div>
          ) : (
            listes.connectes.map((j) => (
              <div className="li" key={j.nom}>
                <Tete nom={j.nom} />
                <button type="button" className="pname pbtn" title="Voir la fiche" onClick={() => setFiche(j.nom)}>
                  <b>
                    {j.nom}
                    {estOp(j.nom) && (
                      <span className="op">
                        <IconCrown size={10} stroke={2.6} aria-hidden />
                        OP
                      </span>
                    )}
                  </b>
                  <span>{j.depuis && duree(j.depuis, maintenant) ? `depuis ${duree(j.depuis, maintenant)}` : "à l’instant"}</span>
                </button>
                <button
                  type="button"
                  className={"icb more" + (menu?.nom === j.nom ? " open" : "")}
                  aria-label={`Actions pour ${j.nom}`}
                  aria-haspopup="menu"
                  aria-expanded={menu?.nom === j.nom}
                  onClick={(e) => {
                    const r = e.currentTarget.getBoundingClientRect();
                    setMenu({ nom: j.nom, x: r.right, y: r.bottom + 4 });
                  }}
                >
                  <IconDots size={16} stroke={2} aria-hidden />
                </button>
              </div>
            ))
          )}
        </section>
      ) : onglet === "blanche" ? (
        <>
          <section className="surf">
            <div className="row">
              <div className="lbl">
                <span id="pl-wl">Liste blanche</span>
                <span className="t2">{listes.listeBlanche ? "Seuls ces joueurs peuvent entrer." : "Tout le monde peut entrer."}</span>
              </div>
              <button
                type="button"
                role="switch"
                aria-checked={listes.listeBlanche}
                aria-labelledby="pl-wl"
                className={"sw" + (listes.listeBlanche ? " on" : "")}
                onClick={() =>
                  changerListeBlanche(serveur.id, !listes.listeBlanche).then(
                    (l) => {
                      setListes(l);
                      onInfo(l.listeBlanche ? "Liste blanche activée" : "Liste blanche coupée");
                    },
                    (e) => onErreur(phrase(e)),
                  )
                }
              >
                <i />
              </button>
            </div>
          </section>
          <form className="pl-add" onSubmit={ajouter}>
            <label>
              <IconPlus size={14} stroke={2} aria-hidden />
              <input type="text" value={ajout} maxLength={17} placeholder="Pseudo Minecraft à ajouter" aria-label="Pseudo à ajouter" spellCheck={false} onChange={(e) => setAjout(e.target.value)} />
            </label>
            <button type="submit" className="btn acc" disabled={ajout.trim().length < 3}>
              Ajouter
            </button>
          </form>
          <section className="surf">
            {listes.blancs.length === 0 ? (
              <div className="empty">Personne dans la liste.</div>
            ) : (
              listes.blancs.map((b) => (
                <div className="li" key={b.nom}>
                  <Tete nom={b.nom} />
                  <span className="pname">
                    <b>
                      {b.nom}
                      {estOp(b.nom) && <span className="op">OP</span>}
                    </b>
                  </span>
                  <button type="button" className="btn" onClick={() => essayer("blanc_retirer", b.nom, `${b.nom} retiré de la liste blanche`)}>
                    Retirer
                  </button>
                </div>
              ))
            )}
          </section>
        </>
      ) : onglet === "demandes" ? (
        <section className="surf">
          {listes.demandes.length === 0 ? (
            <div className="empty">{listes.listeBlanche ? "Aucune demande en attente." : "Liste blanche coupée : tout le monde peut entrer."}</div>
          ) : (
            listes.demandes.map((d) => (
              <div className="li demande" key={d.nom}>
                <Tete nom={d.nom} />
                <span className="pname">
                  <b>{d.nom}</b>
                  <span>{ilYa(d.quand, maintenant) + (d.fois > 1 ? ` · ${d.fois} essais` : "")}</span>
                </span>
                <button type="button" className="btn" onClick={() => repondre(d.nom, false)}>
                  Refuser
                </button>
                <button type="button" className="btn acc" onClick={() => repondre(d.nom, true)}>
                  <IconCheck size={13} stroke={2.4} aria-hidden />
                  Accepter
                </button>
              </div>
            ))
          )}
        </section>
      ) : (
        <section className="surf">
          {listes.bannis.length === 0 ? (
            <div className="empty">Personne n'est banni.</div>
          ) : (
            listes.bannis.map((b) => (
              <div className="li" key={b.nom}>
                <span className="banni">
                  <Tete nom={b.nom} />
                </span>
                <span className="pname">
                  <b>{b.nom}</b>
                  <span>{["Banni" + (jour(b.quand) ? ` le ${jour(b.quand)}` : ""), b.motif].filter(Boolean).join(" · ")}</span>
                </span>
                <button type="button" className="btn" onClick={() => essayer("debannir", b.nom, `${b.nom} a été débanni`)}>
                  Débannir
                </button>
              </div>
            ))
          )}
        </section>
      )}

      {menu && (
        <MenuJoueur
          {...menu}
          op={estOp(menu.nom)}
          onFermer={() => setMenu(null)}
          onOp={() => basculerOp(menu.nom)}
          onExpulser={() => essayer("expulser", menu.nom, `${menu.nom} a été expulsé`)}
          onBannir={() => setABannir(menu.nom)}
        />
      )}
      {fiche && (
        <FicheJoueur
          id={serveur.id}
          nom={fiche}
          connecte={!!joueurFiche}
          depuis={joueurFiche?.depuis ?? null}
          op={estOp(fiche)}
          onOp={() => basculerOp(fiche)}
          onExpulser={() => {
            essayer("expulser", fiche, `${fiche} a été expulsé`);
            setFiche(null);
          }}
          onBannir={() => setABannir(fiche)}
          onErreur={onErreur}
          onInfo={onInfo}
          onFermer={() => setFiche(null)}
        />
      )}
      {inviter && (
        <Inviter
          serveur={serveur}
          acces={acces}
          ancre={inviter}
          onFermer={() => setInviter(null)}
          onChange={setListes}
          onInfo={onInfo}
          onErreur={onErreur}
        />
      )}
      {aBannir && (
        <Bannir
          nom={aBannir}
          onFermer={() => setABannir(null)}
          onBannir={(motif) =>
            agir("bannir", aBannir, `${aBannir} a été banni`, motif).then(() => {
              if (fiche === aBannir) setFiche(null);
            })
          }
        />
      )}
    </div>
  );
}

const MARGE = 8;

type Menu = {
  nom: string;
  x: number; // bord droit du bouton « … »
  y: number;
  op: boolean;
  onFermer: () => void;
  onOp: () => void;
  onExpulser: () => void;
  onBannir: () => void;
};

// Le menu « … » d'un joueur connecté : un panneau de verre qui s'ouvre sous le bouton.
function MenuJoueur({ nom, x, y, op, onFermer, onOp, onExpulser, onBannir }: Menu) {
  const menu = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState({ x, y });

  useLayoutEffect(() => {
    const el = menu.current;
    if (!el) return;
    setPlace({
      x: Math.max(MARGE, Math.min(x - el.offsetWidth, window.innerWidth - el.offsetWidth - MARGE)),
      y: Math.max(MARGE, Math.min(y, window.innerHeight - el.offsetHeight - MARGE)),
    });
    el.querySelector<HTMLButtonElement>("button")?.focus();
  }, [x, y]);

  useEffect(() => {
    window.addEventListener("resize", onFermer);
    window.addEventListener("blur", onFermer);
    return () => {
      window.removeEventListener("resize", onFermer);
      window.removeEventListener("blur", onFermer);
    };
  }, [onFermer]);

  const choisir = (action: () => void) => () => {
    onFermer();
    action();
  };

  return (
    <>
      <div className="ctx-catch" onMouseDown={onFermer} onContextMenu={(e) => (e.preventDefault(), onFermer())} />
      <div
        ref={menu}
        className="g ctx pl-menu"
        data-lens="panel"
        role="menu"
        aria-label={`Actions pour ${nom}`}
        style={{ left: place.x, top: place.y }}
        onKeyDown={(e) => e.key === "Escape" && onFermer()}
      >
        <button type="button" role="menuitem" className="pm-item" onClick={choisir(onOp)}>
          <IconCrown size={16} stroke={2} aria-hidden />
          {op ? "Retirer l’OP" : "Donner l’OP"}
        </button>
        <button type="button" role="menuitem" className="pm-item" onClick={choisir(onExpulser)}>
          <IconDoorExit size={16} stroke={2} aria-hidden />
          Expulser
        </button>
        <div className="pm-sep" />
        <button type="button" role="menuitem" className="pm-item danger" onClick={choisir(onBannir)}>
          <IconBan size={16} stroke={2} aria-hidden />
          Bannir…
        </button>
      </div>
    </>
  );
}
