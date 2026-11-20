# BHOST

Héberge tes serveurs Minecraft Java sur ton PC Windows, en quelques clics.

- Crée un serveur : Vanilla, Paper, Purpur, Spigot, Fabric, Forge, NeoForge, Mohist, Youer, Arclight.
- Console, joueurs, plugins, mods et datapacks, fichiers, sauvegardes, automatisations.
- « Ouvert à tes amis » : BHOST ouvre le port de ta box (UPnP), ou passe par le relais BHOST.
- Joueurs Bedrock (Geyser + Floodgate).

## Installer

Télécharge la dernière version dans [Releases](../../releases). BHOST se met à jour tout seul.

## Développer

Il faut : Windows 10 ou 11, [Node.js](https://nodejs.org) 22 ou plus, [Rust](https://rustup.rs), WebView2 (déjà dans Windows 11).

```powershell
npm ci
npm run tauri dev
```

Tests :

```powershell
cd src-tauri
cargo test --lib
```

Le relais (serveur Linux) est dans [`relais/`](relais/README.md).

## Licence

Le code de BHOST est sous licence [GPL-3.0](LICENSE) : tu peux le copier, le modifier et le partager, à condition de garder la même licence et de publier ton code.

Ne sont pas sous GPL :

- le nom et le logo BHOST ;
- les logos des logiciels de serveur (Paper, Fabric, Purpur, Spigot, Forge, NeoForge, MohistMC, Arclight) : ils appartiennent à leurs auteurs et servent seulement à reconnaître le logiciel (voir [`docs/logos/LICENCES.md`](docs/logos/LICENCES.md)) ;
- les polices Mona Sans et JetBrains Mono : licence SIL Open Font License (fichiers `*-LICENCE.txt` à côté des polices).

BHOST n'est pas un produit officiel de Minecraft. Il n'est ni approuvé par Mojang ou Microsoft, ni associé à eux.
