# Publier une version de BHOST

BHOST cherche ses mises à jour dans les Releases de ce dépôt (Réglages › Rechercher les mises à jour).
L'installeur est signé : BHOST refuse une mise à jour qui n'est pas signée avec la clé du projet.

## Une seule fois

1. La clé de signature (PowerShell, dans le dossier du projet) :
   ```
   npx tauri signer generate -w "$HOME\.tauri\bhost.key"
   ```
   Choisis un mot de passe et **garde-le**. Deux fichiers sont créés dans `$HOME\.tauri\` :
   - `bhost.key` : **secrète**. Jamais dans Git. Sauvegarde-la ailleurs (clé USB) : sans elle, plus de mises à jour possibles.
   - `bhost.key.pub` : publique. Son contenu va dans `src-tauri/src/mise_a_jour.rs` (`CLE_PUBLIQUE`).
2. Dans le dépôt GitHub › Settings › Secrets and variables › Actions, deux secrets :
   - `TAURI_SIGNING_PRIVATE_KEY` : tout le contenu de `bhost.key`.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` : le mot de passe.

## À chaque version

1. Change le numéro (ex. `0.2.0`) dans `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` et `package.json`.
2. Commit, puis `git push`.
3. GitHub › Actions › « Publier BHOST » › Run workflow (≈ 15 min).
4. La version apparaît dans Releases. Les BHOST installés la trouvent.

## Machines dédiées (BHOST OS et VPS)

À joindre à la même Release (Edit › glisser les fichiers), à côté de l'installeur Windows :

- `bhost-os.iso` et `bhost-os.iso.sha256` : l'ISO (voir `os/LISEZMOI.md`). BHOST les télécharge à l'étape « Clé USB ».
- `installer-vps.sh` : le fichier `os/installer-vps.sh` tel quel. C'est lui que lance la commande donnée par BHOST
  (Ajouter une machine › « Un VPS ? »).
- `bhost-agent-linux-x86_64` et `bhost-agent-linux-x86_64.sha256` : l'agent pour les VPS. Compilé pour une glibc
  ancienne (2.31) pour marcher sur Ubuntu 22.04 et Debian 12 aussi :
  ```
  pip install ziglang && cargo install cargo-zigbuild
  cargo zigbuild --release --no-default-features --features agent --bin bhost-agent --target x86_64-unknown-linux-gnu.2.31
  sha256sum bhost-agent-linux-x86_64 > bhost-agent-linux-x86_64.sha256
  ```
