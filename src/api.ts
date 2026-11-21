// Le pont entre l'interface et la partie Rust de BHOST.
// Chaque fonction appelle une commande de src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Acces, Machine, Priorite, Serveur, Statut } from "./types";

export function listerServeurs() {
  return invoke<Serveur[]>("lister_serveurs");
}

// Ajoute à BHOST un serveur qui existe déjà dans un dossier.
// En cas de refus, l'erreur est une phrase en français à afficher telle quelle.
export function importerServeur(dossier: string) {
  return invoke<Serveur>("importer_serveur", { dossier });
}

// ---- Assistant « Créer un serveur » ----

export type InfosCreation = {
  dossier: string; // où les serveurs sont créés
  disque: string; // « C: »
  libreGo: number;
  totalGo: number;
  memoireGo: number; // mémoire du PC
  premier: boolean; // aucun serveur encore : l'assistant commence par le dossier
};
export const infosCreation = () => invoke<InfosCreation>("infos_creation");
export const changerDossierServeurs = (dossier: string) => invoke<void>("changer_dossier_serveurs", { dossier });

export type VersionMc = { id: string; java: number | null; instable: boolean };
// Les versions qu'on peut installer (demandées à Mojang, PaperMC ou Purpur).
export const versionsMinecraft = (logiciel: string, snapshots: boolean) =>
  invoke<VersionMc[]>("versions_minecraft", { logiciel, snapshots });

export type DemandeCreation = {
  nom: string;
  logiciel: string;
  version: string;
  ramGo: number;
  eula: boolean;
  mode: string; // survie, creatif, aventure, hardcore
  difficulte: string; // paisible, facile, normale, difficile
  joueursMax: number;
  couleur: string; // l'icône du serveur (clé de theme/icones.ts)
  ouvertAmis: boolean; // Ouvert à tes amis dès le départ
  machine?: string; // id d'une machine dédiée ; absent = ce PC
  stockageGo?: number; // machine dédiée : stockage max
};
// Crée le serveur. L'avancement arrive par la nouvelle « creation ».
export const creerServeur = (demande: DemandeCreation) => invoke<Serveur>("creer_serveur", { demande });

// S'abonne à l'avancement de la création (tâche 0 = Java, 1 = serveur, 2 = monde).
export function ecouterCreation(recu: (tache: number, pourcent: number) => void) {
  const abonnement = listen<{ tache: number; pourcent: number }>("creation", (e) => recu(e.payload.tache, e.payload.pourcent));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// Change le nom d'un serveur. En cas de refus, l'erreur est une phrase à afficher.
export const renommerServeur = (id: string, nom: string) => invoke<Serveur>("renommer_serveur", { id, nom });

// Change la couleur du cube d'un serveur.
export const couleurServeur = (id: string, couleur: string) => invoke<Serveur>("couleur_serveur", { id, couleur });

// Supprime un serveur : son dossier va à la Corbeille.
export const supprimerServeur = (id: string) => invoke<void>("supprimer_serveur", { id });

// Retire un serveur de BHOST : son dossier reste sur le PC.
export const retirerServeur = (id: string) => invoke<void>("retirer_serveur", { id });

// Ouvre le dossier d'un serveur dans l'Explorateur de fichiers.
export const ouvrirDossier = (id: string) => invoke<void>("ouvrir_dossier", { id });

// Ouvre la fenêtre de Windows pour choisir un dossier. Renvoie null si on annule.
export async function choisirDossier(titre: string) {
  const choix = await open({ directory: true, multiple: false, title: titre });
  return typeof choix === "string" ? choix : null;
}

// Les quatre actions de la capsule. Elles répondent tout de suite :
// la suite arrive par les nouvelles « statut » et « console ».
// Machine dédiée : si la RAM ne suffit pas, le refus est un ManqueMemoire (pas une phrase).
export const demarrerServeur = (id: string) => invoke<void>("demarrer_serveur", { id });
export type ManqueMemoire = { manqueMo: number; allumes: { id: string; nom: string; ramMo: number }[] };
export const estManqueMemoire = (e: unknown): e is ManqueMemoire => typeof e === "object" && e !== null && "manqueMo" in e;
export const arreterServeur = (id: string) => invoke<void>("arreter_serveur", { id });
export const redemarrerServeur = (id: string) => invoke<void>("redemarrer_serveur", { id });
export const forcerArret = (id: string) => invoke<void>("forcer_arret", { id });

// Envoie une commande au serveur, comme tapée dans sa console.
export const envoyerCommande = (id: string, commande: string) => invoke<void>("envoyer_commande", { id, commande });

// Les lignes de console déjà reçues (pour l'ouverture de la page).
// « dernier » est le numéro de la dernière ligne de la liste.
export const lireConsole = (id: string) => invoke<{ dernier: number; lignes: string[] }>("lire_console", { id });

// S'abonne aux changements d'état des serveurs. Renvoie de quoi se désabonner.
export function ecouterStatuts(recu: (id: string, statut: Statut, depuis: number | null) => void) {
  const abonnement = listen<{ id: string; statut: Statut; depuis: number | null }>("statut", (e) =>
    recu(e.payload.id, e.payload.statut, e.payload.depuis),
  );
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// S'abonne à la liste des joueurs connectés. Renvoie de quoi se désabonner.
export function ecouterJoueurs(recu: (id: string, joueurs: string[]) => void) {
  const abonnement = listen<{ id: string; joueurs: string[] }>("joueurs", (e) => recu(e.payload.id, e.payload.joueurs));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// Ouvert à tes amis : l'état des ports ouverts sur la box, et ses changements.
export const etatsAcces = () => invoke<Acces[]>("etats_acces");
export const ouvrirAuxAmis = (id: string, ouvert: boolean) => invoke<Serveur>("ouvrir_aux_amis", { id, ouvert });

// Paramètres › Joueurs Bedrock (Geyser + Floodgate).
export type VueBedrock = {
  possible: boolean;
  raison?: string | null; // « Pas possible avec Forge. »
  actif: boolean;
  port: number; // 19132, ou un suivant
  redemarrer: boolean; // le serveur tourne : compte au prochain démarrage
  note?: string; // « ViaVersion n'a pas pu être installé… »
};
export const bedrockServeur = (id: string) => invoke<VueBedrock>("bedrock_serveur", { id });
export const reglerBedrock = (id: string, actif: boolean) => invoke<VueBedrock>("regler_bedrock", { id, actif });
export function ecouterAcces(recu: (liste: Acces[]) => void) {
  const abonnement = listen<Acces[]>("acces", (e) => recu(e.payload));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// S'abonne aux mesures (mémoire, processeur), envoyées toutes les deux secondes pour chaque serveur allumé.
export function ecouterMesures(recu: (id: string, processeur: number, memoireMo: number) => void) {
  const abonnement = listen<{ id: string; processeur: number; memoireMo: number }>("mesure", (e) =>
    recu(e.payload.id, e.payload.processeur, e.payload.memoireMo),
  );
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// S'abonne au téléchargement de Java (pourcent : null quand c'est fini).
export function ecouterJava(recu: (id: string, version: number, pourcent: number | null) => void) {
  const abonnement = listen<{ id: string; version: number; pourcent: number | null }>("java", (e) =>
    recu(e.payload.id, e.payload.version, e.payload.pourcent),
  );
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// S'abonne aux ticks par seconde (20 = tout va bien), relevés toutes les dix secondes.
export function ecouterTps(recu: (id: string, tps: number) => void) {
  const abonnement = listen<{ id: string; tps: number }>("tps", (e) => recu(e.payload.id, e.payload.tps));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// Dit à Rust quel serveur est affiché à l'accueil : il ne demande les ticks par seconde qu'à celui-là.
export const regarderTps = (id: string, actif: boolean) => invoke<void>("regarder_tps", { id, actif });

// La tête d'un joueur (une image prête à afficher), demandée à Mojang par Rust. null s'il n'en a pas.
export const teteJoueur = (nom: string) => invoke<string | null>("tete_joueur", { nom });

// L'icône de BHOST dans Windows (barre des tâches, Alt+Tab, zone de notification) suit le thème : « azur-clair »…
export const changerIcone = (theme: string) => invoke<void>("changer_icone", { theme });

// S'abonne aux nouvelles lignes de console (n : numéro de la ligne).
// La promesse est tenue quand l'abonnement est en place ; elle donne de quoi se désabonner.
export function ecouterConsole(recu: (id: string, n: number, ligne: string) => void) {
  return listen<{ id: string; n: number; ligne: string }>("console", (e) =>
    recu(e.payload.id, e.payload.n, e.payload.ligne),
  );
}

// ---- Onglet Paramètres ----

export type Partie = { mode: string; difficulte: string; joueursMax: number };
export type Parametres = { partie: Partie; javaAuto: number | null; commande: string; memoireGo: number };
export type ReglagesServeur = {
  ramMo: number;
  demarrerAuto: boolean;
  javaChoix: string | null;
  optimise: boolean;
  arguments: string;
  priorite: Priorite;
};

export const parametresServeur = (id: string) => invoke<Parametres>("parametres_serveur", { id });
// Enregistre les réglages gardés par BHOST. Renvoie le serveur et l'aperçu à jour.
export const reglerServeur = (id: string, reglages: ReglagesServeur) =>
  invoke<[Serveur, Parametres]>("regler_serveur", { id, reglages });
// Mode, difficulté, joueurs : écrits dans server.properties.
export const reglerPartie = (id: string, partie: Partie) => invoke<Serveur>("regler_partie", { id, partie });

// Choisit un java.exe sur le PC.
export async function choisirJava() {
  const choix = await open({ multiple: false, title: "Choisis java.exe", filters: [{ name: "Java", extensions: ["exe"] }] });
  return typeof choix === "string" ? choix : null;
}

// Copie un serveur arrêté (clic droit › Dupliquer). L'avancement arrive par la nouvelle « copie ».
export const dupliquerServeur = (id: string) => invoke<Serveur>("dupliquer_serveur", { id });

export function ecouterCopie(recu: (id: string, pourcent: number) => void) {
  const abonnement = listen<{ id: string; pourcent: number }>("copie", (e) => recu(e.payload.id, e.payload.pourcent));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// ---- Paramètres › Règles du jeu ----

// Une règle : valeur 1/0 (vrai/faux), ou un nombre (« dormir » : pourcentage de joueurs).
export type Regle = { cle: string; valeur: number };
export const reglesServeur = (id: string) => invoke<Regle[]>("regles_serveur", { id });
// Tout de suite si le serveur est en ligne, sinon à son prochain démarrage. Renvoie les règles à jour.
export const reglerRegle = (id: string, cle: string, valeur: number) => invoke<Regle[]>("regler_regle", { id, cle, valeur });

// ---- Paramètres › Mondes ----

export type Monde = {
  dossier: string; // nom du dossier : c'est lui qui désigne le monde
  nom: string;
  version: string | null;
  taille: number; // octets
  joue: number | null; // dernière partie (ms)
  mode: string | null; // survie, creatif, aventure, spectateur, hardcore
  actuel: boolean; // le monde joué par le serveur
};
export type MondeSolo = Monde & { chemin: string; tropRecent: boolean };

export const mondesServeur = (id: string) => invoke<Monde[]>("mondes_serveur", { id });
export const utiliserMonde = (id: string, dossier: string) => invoke<Monde[]>("utiliser_monde", { id, dossier });
export const nouveauMonde = (id: string) => invoke<Monde[]>("nouveau_monde", { id });
// Le monde va à la Corbeille (jamais celui joué).
export const supprimerMonde = (id: string, dossier: string) => invoke<Monde[]>("supprimer_monde", { id, dossier });
// Sauvegarde, puis efface : le serveur le recrée (même graine, ou une nouvelle).
export const reinitialiserMonde = (id: string, dossier: string, memeGraine: boolean) =>
  invoke<Monde[]>("reinitialiser_monde", { id, dossier, memeGraine });
export const exporterMonde = (id: string, dossier: string, fichier: string) => invoke<void>("exporter_monde", { id, dossier, fichier });
// Les mondes du jeu solo (%APPDATA%\.minecraft\saves).
export const mondesSolo = (id: string) => invoke<MondeSolo[]>("mondes_solo", { id });
// source : un dossier de monde, ou une archive .zip. actuel : il devient le monde joué.
export const importerMonde = (id: string, source: string, actuel: boolean) => invoke<Monde[]>("importer_monde", { id, source, actuel });

// Choisir où enregistrer une archive .zip. Renvoie null si on annule.
export async function choisirEnregistrement(nom: string) {
  const choix = await save({ title: "Enregistrer le monde", defaultPath: nom, filters: [{ name: "Archive", extensions: ["zip"] }] });
  return typeof choix === "string" ? choix : null;
}

// Choisir une archive .zip de monde. Renvoie null si on annule.
export async function choisirZip() {
  const choix = await open({ multiple: false, title: "Choisis une archive de monde", filters: [{ name: "Archive", extensions: ["zip"] }] });
  return typeof choix === "string" ? choix : null;
}

// ---- Paramètres › Changer de version ----

// Le serveur doit être arrêté. Le monde est sauvegardé juste avant.
export const changerVersion = (id: string, logiciel: string, version: string) => invoke<Serveur>("changer_version", { id, logiciel, version });

// S'abonne à l'avancement d'un long travail (sauvegarde, copie, installation…).
export function ecouterTravail(recu: (id: string, etape: string, pourcent: number) => void) {
  const abonnement = listen<{ id: string; etape: string; pourcent: number }>("travail", (e) => recu(e.payload.id, e.payload.etape, e.payload.pourcent));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// ---- Onglet Joueurs ----

export type Inscrit = { nom: string; quand?: string; motif?: string };
export type Joueurs = {
  connectes: { nom: string; depuis: number | null }[];
  ops: string[];
  listeBlanche: boolean; // white-list=true
  blancs: Inscrit[];
  bannis: Inscrit[];
  demandes: Demande[]; // refusés par la liste blanche, le plus récent en premier
};
// Un joueur refusé par la liste blanche, qui attend une réponse.
export type Demande = { nom: string; quand: number; fois: number };
export type ActionJoueur = "op" | "deop" | "expulser" | "bannir" | "debannir" | "blanc_ajouter" | "blanc_retirer";

export const joueursServeur = (id: string) => invoke<Joueurs>("joueurs_serveur", { id });
// En ligne : par commande. Arrêté : dans ops.json, whitelist.json, banned-players.json.
export const actionJoueur = (id: string, action: ActionJoueur, nom: string, motif?: string) =>
  invoke<Joueurs>("action_joueur", { id, action, nom, motif: motif ?? null });
export const changerListeBlanche = (id: string, active: boolean) => invoke<Joueurs>("liste_blanche", { id, active });
// Joueurs › Demandes : Accepter (liste blanche) ou Refuser (ses prochains essais sont ignorés).
export const repondreDemande = (id: string, nom: string, accepter: boolean) => invoke<Joueurs>("repondre_demande", { id, nom, accepter });
// Les demandes d'un serveur ont changé. `nouvelle` : quelqu'un vient d'essayer d'entrer.
export function ecouterDemandes(recu: (id: string, demandes: Demande[], nouvelle: Demande | null) => void) {
  const abonnement = listen<{ id: string; demandes: Demande[]; nouvelle: Demande | null }>("demandes", (e) =>
    recu(e.payload.id, e.payload.demandes, e.payload.nouvelle),
  );
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

export type Fiche = {
  tempsJeu: number | null; // secondes
  premiere: number | null; // ms
  morts: number | null;
  position: [number, number, number] | null;
  dimension: string | null;
  mode: string | null;
  enregistre: number | null; // ms
};
// null : BHOST ne trouve pas ce joueur dans le monde.
export const ficheJoueur = (id: string, nom: string) => invoke<Fiche | null>("fiche_joueur", { id, nom });
// action : « mode:survie », « mode:creatif », « mode:aventure », « mode:spectateur » ou « apparition ». Serveur en ligne.
export const commandeJoueur = (id: string, nom: string, action: string) => invoke<void>("commande_joueur", { id, nom, action });

// ---- Onglet Sauvegardes ----

export type ReglagesSauvegarde = { auto: boolean; frequence: "1h" | "6h" | "24h"; garder: number; avantMaj: boolean };
export type Sauvegarde = {
  fichier: string; // c'est lui qui la désigne
  quand: number; // ms
  taille: number; // octets
  genre: string; // auto, manuelle, maj, restauration, reinit
  quoi: string; // tout ou monde
  etiquette: string | null;
  epinglee: boolean;
};
export type Sauvegardes = { reglages: ReglagesSauvegarde; liste: Sauvegarde[]; prochaine: number | null; libreGo: number };

export const sauvegardesServeur = (id: string) => invoke<Sauvegardes>("sauvegardes_serveur", { id });
export const reglerSauvegardes = (id: string, reglages: ReglagesSauvegarde) => invoke<Sauvegardes>("regler_sauvegardes", { id, reglages });
export const sauvegarderMaintenant = (id: string) => invoke<Sauvegardes>("sauvegarder_maintenant", { id });
export const epinglerSauvegarde = (id: string, fichier: string, epinglee: boolean) =>
  invoke<Sauvegardes>("epingler_sauvegarde", { id, fichier, epinglee });
export const exporterSauvegarde = (id: string, fichier: string, cible: string) => invoke<void>("exporter_sauvegarde", { id, fichier, cible });
export const supprimerSauvegarde = (id: string, fichier: string) => invoke<Sauvegardes>("supprimer_sauvegarde", { id, fichier });
// Serveur arrêté. tout : tout le serveur ; sinon seulement le monde. L'état actuel est sauvegardé avant.
export const restaurerSauvegarde = (id: string, fichier: string, tout: boolean) => invoke<Serveur>("restaurer_sauvegarde", { id, fichier, tout });
export const ouvrirSauvegardes = (id: string) => invoke<void>("ouvrir_sauvegardes", { id });

// ---- Onglet Fichiers ----

export type Element = { nom: string; dossier: boolean; taille: number; modifie: number | null }; // taille : octets, ou éléments d'un dossier
export type ContenuFichier =
  | { genre: "texte"; texte: string; latin1: boolean }
  | { genre: "image"; adresse: string }
  | { genre: "tropGros" }
  | { genre: "binaire" };
export type FichierLu = ContenuFichier & { taille: number; modifie: number | null };

// Les chemins sont relatifs au dossier du serveur, avec des « / » (« plugins/Essentials »).
export const fichiersServeur = (id: string, dossier: string) => invoke<Element[]>("fichiers_serveur", { id, dossier });
export const lireFichier = (id: string, chemin: string) => invoke<FichierLu>("lire_fichier", { id, chemin });
export const ecrireFichier = (id: string, chemin: string, texte: string) => invoke<void>("ecrire_fichier", { id, chemin, texte });
export const importerFichiers = (id: string, dossier: string, sources: string[]) => invoke<number>("importer_fichiers", { id, dossier, sources });
export const iconeServeur = (id: string, source: string) => invoke<void>("icone_serveur", { id, source });
/** Pose l'icône BHOST comme icône dans Minecraft (sauf image à soi, ou `remettre`) ; vrai si c'est une image à soi. */
export const iconeDuJeu = (id: string, remettre = false) => invoke<boolean>("icone_du_jeu", { id, remettre });
export const montrerFichier = (id: string, chemin: string) => invoke<void>("montrer_fichier", { id, chemin });
export const proprietesServeur = (id: string) => invoke<Record<string, string>>("proprietes_serveur", { id });
export const reglerProprietes = (id: string, valeurs: Record<string, string>) => invoke<Record<string, string>>("regler_proprietes", { id, valeurs });

// Choisir des fichiers à importer. Renvoie [] si on annule.
export async function choisirFichiers() {
  const choix = await open({ multiple: true, title: "Fichiers à importer" });
  return Array.isArray(choix) ? choix : typeof choix === "string" ? [choix] : [];
}

// Choisir une image PNG.
export async function choisirImage() {
  const choix = await open({ multiple: false, title: "Choisis une image carrée", filters: [{ name: "Image PNG", extensions: ["png"] }] });
  return typeof choix === "string" ? choix : null;
}

// ---- Onglet Automatisations ----

export type Declencheur = "time" | "crash" | "empty" | "join" | "boot" | "lag" | "mem" | "wake" | "upd";
export type ActionAuto = "start" | "stop" | "restart" | "backup" | "say" | "cmd" | "notify" | "update";
export type Automatisation = {
  id: string; // vide : nouvelle
  on: boolean;
  trig: Declencheur;
  act: ActionAuto;
  when: "once" | "daily" | "days" | "every";
  time: string; // « 05:00 »
  days: number[]; // 0 = lundi
  date: string; // « 2026-11-18 » (une fois)
  hours: number; // toutes les N heures
  warn: boolean;
  warnMin: number; // 1, 5 ou 15
  msg: string; // {joueur} = son pseudo
  cmd: string;
  mins: number; // il est vide depuis N minutes
  first: boolean; // première venue seulement
  motd: string; // message pendant le sommeil (réveil)
};
export type Fait = { quoi: string; act: string; quand: number };
export type VueAutomatisations = { liste: Automatisation[]; historique: Fait[]; tpsPossible: boolean; majPossible: boolean };
export const automatisationsServeur = (id: string) => invoke<VueAutomatisations>("automatisations_serveur", { id });
export const enregistrerAutomatisation = (id: string, automatisation: Automatisation) =>
  invoke<VueAutomatisations>("enregistrer_automatisation", { id, automatisation });
export const supprimerAutomatisation = (id: string, aid: string) => invoke<VueAutomatisations>("supprimer_automatisation", { id, aid });
export const activerAutomatisation = (id: string, aid: string, on: boolean) =>
  invoke<VueAutomatisations>("activer_automatisation", { id, aid, on });

// Une automatisation vient d'agir sur ce serveur (« Dernières actions » a changé).
export function ecouterAutomatisations(recu: (id: string) => void) {
  const abonnement = listen<string>("automatisations", (e) => recu(e.payload));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// ---- Réglages de BHOST ----

export type VueReglages = {
  lancerWindows: boolean;
  garder: boolean; // fermer la fenêtre range BHOST en bas à droite, les serveurs continuent
  proprement: boolean; // quand Windows s'éteint, BHOST arrête d'abord les serveurs
  veille: boolean; // empêcher la veille tant qu'un serveur tourne
  notifications: boolean;
  pseudo: string; // vide : aucun
  dossier: string; // le dossier des serveurs
  serveurs: number; // combien de serveurs de BHOST il contient
  octets: number; // sa taille
  version: string; // de BHOST
};
export type ChangementReglages = Partial<{ lancerWindows: boolean; garder: boolean; proprement: boolean; veille: boolean; notifications: boolean; pseudo: string }>;
export const reglagesBhost = () => invoke<VueReglages>("reglages_bhost");
export const reglerBhost = (changement: ChangementReglages) => invoke<VueReglages>("regler_bhost", { changement });
export const ouvrirDossierServeurs = () => invoke<void>("ouvrir_dossier_serveurs");

// Réglages › Rechercher les mises à jour.
export type VueMaj = { etat: "a-jour" | "dispo"; version: string | null };
export const chercherMiseAJour = () => invoke<VueMaj>("chercher_mise_a_jour");
// Télécharge, arrête les serveurs proprement, puis lance l'installeur (BHOST se ferme).
export const installerMiseAJour = () => invoke<void>("installer_mise_a_jour");
// L'avancement du téléchargement (0 à 100).
export function suivreMiseAJour(recu: (pourcent: number) => void): () => void {
  const abonnement = listen<number>("mise-a-jour", (e) => recu(e.payload));
  return () => {
    abonnement.then((arret) => arret());
  };
}

// ---- Onglet Plugins / Mods / Datapacks ----

export type Extension = {
  fichier: string; // nom sur le disque (« WorldEdit.jar », ou « ….jar.disabled » s'il est coupé)
  nom: string;
  version: string | null;
  description: string | null;
  couleur: number | null; // 0xRRGGBB, couleur de son icône dans le catalogue
  actif: boolean;
  dossier: boolean; // datapack en dossier : on ne peut que le supprimer
  maj: string | null; // version plus récente disponible
  pour: string | null; // installé pour un autre (dépendance)
  alerte: string | null;
  projet: string | null; // identifiant dans le catalogue
  icone: string | null; // adresse de son icône dans le catalogue
  panne: string | null; // « Ne démarre pas. Il lui faut Java 25. » (vu au dernier démarrage)
  panneEnJeu: boolean; // la panne arrive en jeu (« Plante en jeu. … ») et pas au démarrage
  conflit: string | null; // « En double avec X.jar. » ou « Ne va pas avec Y. »
  monde: boolean; // datapack qui a créé le terrain du monde : le retirer peut le casser
};
// Un datapack que le monde attend et qui n'est plus là.
export type Manquant = { fichier: string; nom: string };
export type GenreExtensions = "plugins" | "mods" | "datapacks";
export type VueExtensions = {
  genre: GenreExtensions;
  genres: GenreExtensions[]; // ce que ce serveur sait charger (le principal, puis les datapacks depuis la 1.13)
  nombres: Partial<Record<GenreExtensions, number>>;
  monde: string | null; // datapacks : le monde où ils vont (« world »)
  liste: Extension[];
  redemarrer: boolean; // le serveur tourne et ses extensions ont changé
  horsLigne: boolean; // le catalogue n'a pas répondu
  manquants: Manquant[]; // le monde ne se charge plus : il lui manque ces datapacks
};
export type Projet = {
  id: string;
  nom: string;
  description: string;
  couleur: number | null;
  icone: string | null;
  auteur: string | null;
  telechargements: number;
};
export type Tri = "pertinence" | "telecharges" | "moins" | "suivis" | "recents" | "maj";
export type PageCatalogue = { projets: Projet[]; total: number; suite: boolean };

// genre : null = le genre principal du serveur.
type G = GenreExtensions | null;
export const extensionsServeur = (id: string, genre: G = null) => invoke<VueExtensions>("extensions_serveur", { id, genre });
export const chercherExtensions = (id: string, texte: string, categorie: string | null, tri: Tri, page: number, genre: G = null) =>
  invoke<PageCatalogue>("chercher_extensions", { id, texte, categorie, tri, page, genre });
// Un plugin n'a pas démarré (vu dans la console) : la liste a changé.
export function ecouterExtensions(recu: (id: string) => void) {
  const abonnement = listen<string>("extensions", (e) => recu(e.payload));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}
// avec : dépendances installées en plus ; manque : dépendances introuvables pour ce serveur.
export const installerExtension = (id: string, projet: string, genre: G = null) =>
  invoke<{ vue: VueExtensions; avec: string[]; manque: string[] }>("installer_extension", { id, projet, genre });
// forcer : couper ou retirer quand même un datapack qui a créé le terrain du monde (après confirmation).
export const activerExtension = (id: string, fichier: string, actif: boolean, genre: G = null, forcer = false) =>
  invoke<VueExtensions>("activer_extension", { id, fichier, actif, genre, forcer });
export const desinstallerExtension = (id: string, fichier: string, genre: G = null, forcer = false) =>
  invoke<VueExtensions>("desinstaller_extension", { id, fichier, genre, forcer });
export const datapacksManquants = (id: string) => invoke<Manquant[]>("datapacks_manquants", { id });
// Remet un datapack que le monde attend (rallumé, ou réinstallé depuis le catalogue). Renvoie son nom.
export const remettreDatapack = (id: string, fichier: string) => invoke<string>("remettre_datapack", { id, fichier });
// fichiers vide : tout ce qui a une mise à jour.
// faits : combien de fichiers ont vraiment changé (0 : rien de plus récent qui marche sur ce serveur).
export const mettreAJourExtensions = (id: string, fichiers: string[], genre: G = null) =>
  invoke<{ vue: VueExtensions; faits: number }>("mettre_a_jour_extensions", { id, fichiers, genre });

// ---- Problèmes (cartes d'erreur de l'Accueil) ----

// Ce qui empêche un serveur de tourner (problemes.rs). Disparaît au démarrage suivant.
export type Probleme =
  | { genre: "java"; version: number }
  | { genre: "port"; port: number; libre: number | null; serveur: string | null } // serveur : un autre de BHOST qui a ce port
  | { genre: "crash" }
  | { genre: "memoire"; go: number; libreGo: number; demarrage: boolean }; // demarrage : le PC n'avait pas assez de mémoire libre

export const problemesServeurs = () => invoke<Record<string, Probleme>>("problemes_serveurs");
export function ecouterProblemes(recu: (id: string, probleme: Probleme | null) => void) {
  const abonnement = listen<{ id: string; probleme: Probleme | null }>("probleme", (e) => recu(e.payload.id, e.payload.probleme));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}
// Installe le Java demandé (au démarrage du serveur, qui suit).
export const installerJava = (id: string, version: number) => invoke<void>("installer_java_serveur", { id, version });

// La connexion Internet, surveillée quand un serveur ouvert aux amis est allumé.
export const etatInternet = () => invoke<boolean>("etat_internet");
export const verifierInternet = () => invoke<boolean>("verifier_internet");
export function ecouterInternet(recu: (ok: boolean) => void) {
  const abonnement = listen<boolean>("internet", (e) => recu(e.payload));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}

// Machine dédiée (BHOST OS) : les clés USB branchées sur ce PC.
export type CleUsb = { lecteur: string; nom: string; totalGo: number };
export const listerClesUsb = () => invoke<CleUsb[]>("lister_cles_usb");

// ---- Machines dédiées (BHOST OS) ----

// Les machines reliées à ce PC. Leurs serveurs arriveront avec lister_serveurs (champ `machine`).
export const machinesLiees = () => invoke<Machine[]>("machines_liees");
// Relie une machine avec son code (8 caractères, sans tiret). Rejetée avec une phrase si le code est faux.
export const lierMachine = (code: string) => invoke<Machine>("lier_machine", { code });
export const delierMachine = (id: string) => invoke<void>("delier_machine", { id });
export const renommerMachine = (id: string, nom: string) => invoke<void>("renommer_machine", { id, nom });
// S'abonne aux changements (une machine s'allume, s'éteint, est reliée). Renvoie de quoi se désabonner.
export function ecouterMachines(recu: (machines: Machine[]) => void) {
  const abonnement = listen<Machine[]>("machines", (e) => recu(e.payload));
  return () => {
    abonnement.then((arreter) => arreter());
  };
}
