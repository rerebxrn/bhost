// Le thème choisi par l’utilisateur : un thème (Azur, par défaut, ou Ambre) + un mode (clair, sombre ou auto).
// Les couleurs sont dans theme.css : un seul accent par thème, non réglable.
export type Mode = "clair" | "sombre" | "systeme";
export type Fond = "azur" | "ambre";

export const MODES_VALIDES: Mode[] = ["clair", "sombre", "systeme"];
export const FONDS_VALIDES: Fond[] = ["azur", "ambre"];
