// Vrais logos des logiciels de serveur. Origine et règles d’usage : docs/logos/LICENCES.md.
// Vanilla (règles de Mojang) et Modrinth (interdit sans permission écrite) n’en ont pas : icônes neutres.
import paper from "../assets/logiciels/paper.svg";
import purpur from "../assets/logiciels/purpur.png";
import spigot from "../assets/logiciels/spigot.png";
import fabric from "../assets/logiciels/fabric.png";
import forge from "../assets/logiciels/forge.png";
import neoforge from "../assets/logiciels/neoforge.png";
import mohist from "../assets/logiciels/mohist.png";
import arclight from "../assets/logiciels/arclight.png";

export type LogoLogiciel = {
  src: string;
  plein?: boolean; // le logo a déjà son fond carré : il remplit la tuile
};

export const LOGOS: Record<string, LogoLogiciel> = {
  Paper: { src: paper },
  Purpur: { src: purpur },
  Spigot: { src: spigot },
  Fabric: { src: fabric },
  Forge: { src: forge, plein: true },
  NeoForge: { src: neoforge },
  Youer: { src: mohist }, // Youer est fait par MohistMC et n’a pas de logo à lui
  Mohist: { src: mohist },
  Arclight: { src: arclight, plein: true },
};
