// Les mots de base de BHOST, partagés par toute l'interface.

export type Statut = "arrete" | "demarrage" | "en-ligne" | "arret" | "plante";

export type Logiciel =
  | "Vanilla"
  | "Paper"
  | "Purpur"
  | "Spigot"
  | "Fabric"
  | "Forge"
  | "NeoForge"
  | "Youer"
  | "Arclight"
  | "Mohist";

export type Serveur = {
  id: string;
  nom: string;
  dossier: string;
  jar: string;
  logiciel: Logiciel;
  version: string; // vide si inconnue
  ramMo: number;
  port: number;
  statut: Statut;
  depuis: number | null; // heure (ms) de son passage en ligne, s'il l'est
  joueurs: string[]; // pseudos des joueurs connectés
  joueursMax: number;
  java?: { version: number; pourcent: number }; // pendant le téléchargement de Java, avant le démarrage
  couleur?: string; // couleur du cube (clic droit › Icône…)
  javaChoix?: string; // Paramètres › Avancé : absent = auto ; « 8 », « 17 », « 21 », « 25 » ou chemin d'un java.exe
  demarrerAuto?: boolean; // Paramètres › Démarrer avec BHOST
  ouvertAmis?: boolean; // Paramètres › Ouvert à tes amis (port ouvert sur la box)
  optimise?: boolean; // Paramètres › Réglages de démarrage optimisés
  arguments?: string; // Paramètres › Arguments en plus
  priorite?: Priorite; // Paramètres › Performances › Priorité (absente : normale)
};

export type Priorite = "basse" | "normale" | "haute";

// Une machine dédiée (BHOST OS) reliée à ce PC avec son code.
export type Machine = {
  id: string;
  nom: string; // « Salon », modifiable
  code: string; // 8 caractères, sans tiret
  etat: "en-ligne" | "hors-ligne" | "connexion";
  serveurs: number; // combien de serveurs elle héberge
  ip?: string; // sur le réseau de la maison (« 192.168.1.42 »)
  // Mesures de la machine (absentes si elle est hors ligne ou ne sait pas les donner).
  disque?: { utiliseGo: number; totalGo: number };
  memoire?: { reserveeGo: number; totalGo: number }; // RAM réservée par les serveurs allumés
  processeur?: number; // %
  temperature?: number | null; // °C du processeur ; null : pas de capteur
  vueIlYa?: number; // hors ligne : dernière fois vue (ms)
};

// Ouvert à tes amis : où en est l'ouverture du port sur la box (absent = fermé).
export type Acces = {
  id: string;
  etat: "ouverture" | "ouvert" | "erreur";
  adresse?: string; // « 82.64.1.2 », ou avec « :port » si ce n'est pas 25565
  message?: string; // la phrase d'erreur
  bedrock?: string; // joueurs Bedrock : « 82.64.1.2 · 19132 »
  bedrockMessage?: string; // joueurs Bedrock : le port n'a pas pu être ouvert
};

// Les dernières mesures d'un serveur (une toutes les deux secondes), pour les mini-courbes.
// tps : les ticks par seconde (un relevé toutes les dix secondes ; vide si le serveur ne les donne pas).
export type Mesures = { processeur: number[]; memoireMo: number[]; joueurs: number[]; tps: number[] };

// « 3,2 » : un nombre à la française, avec une décimale au plus.
export function nombre(n: number): string {
  return n.toLocaleString("fr-FR", { maximumFractionDigits: 1 });
}

// « 20,0 » : toujours une décimale, pour les ticks par seconde.
export function decimale(n: number): string {
  return n.toLocaleString("fr-FR", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
}

// « localhost », ou « localhost:25570 » si le port n'est pas celui par défaut.
export function adresseLocale(s: Pick<Serveur, "port">): string {
  return s.port === 25565 ? "localhost" : `localhost:${s.port}`;
}

// « 3 h 42 », « 38 min », ou vide en dessous d'une minute.
export function duree(depuis: number, maintenant: number): string {
  const minutes = Math.floor((maintenant - depuis) / 60000);
  if (minutes < 1) return "";
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, "0")}`;
}

// « Paper 26.1 », ou juste « Paper » si la version est inconnue.
export function nomLogiciel(s: Pick<Serveur, "logiciel" | "version">): string {
  return s.version ? `${s.logiciel} ${s.version}` : s.logiciel;
}

export type Onglet =
  | "accueil"
  | "console"
  | "joueurs"
  | "extensions"
  | "fichiers"
  | "sauvegardes"
  | "automatisations"
  | "parametres";

// Ce qu'on lit sous le nom du serveur et à côté de la pastille.
export const NOM_STATUT: Record<Statut, string> = {
  arrete: "Arrêté",
  demarrage: "Démarrage…",
  "en-ligne": "En ligne",
  arret: "Arrêt en cours…",
  plante: "A planté",
};

// L'état en toutes lettres : « Java 21 · 42 % » pendant le téléchargement de Java.
export function texteStatut(s: Pick<Serveur, "statut" | "java">): string {
  if (s.java && s.statut === "demarrage") return `Java ${s.java.version} · ${s.java.pourcent} %`;
  return NOM_STATUT[s.statut];
}

// L'onglet des extensions change de nom selon le logiciel du serveur.
export function nomExtensions(logiciel: Logiciel): string {
  if (logiciel === "Vanilla") return "Datapacks";
  if (logiciel === "Fabric" || logiciel === "Forge" || logiciel === "NeoForge") return "Mods";
  if (logiciel === "Youer" || logiciel === "Arclight" || logiciel === "Mohist") return "Mods & Plugins";
  return "Plugins";
}

export function listeOnglets(logiciel: Logiciel): { id: Onglet; nom: string }[] {
  return [
    { id: "accueil", nom: "Accueil" },
    { id: "console", nom: "Console" },
    { id: "joueurs", nom: "Joueurs" },
    { id: "extensions", nom: nomExtensions(logiciel) },
    { id: "fichiers", nom: "Fichiers" },
    { id: "sauvegardes", nom: "Sauvegardes" },
    { id: "automatisations", nom: "Automatisations" },
    { id: "parametres", nom: "Paramètres" },
  ];
}
