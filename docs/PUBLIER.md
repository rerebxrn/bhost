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
4. La version apparaît dans Releases, avec **BHOST-Setup.exe** : c'est le fichier à télécharger depuis le site
   (`https://github.com/rerebxrn/bhost/releases/latest/download/BHOST-Setup.exe`). Les BHOST installés trouvent la mise à jour.

## Versions en avance (BHOST+)

Les abonnés qui cochent Réglages › Versions en avance lisent d’abord la release au tag `avance`
(`…/releases/download/avance/latest.json`), puis la release normale.
Pour donner une version en avance : publie-la en « pre-release » avec le tag `avance` (remplace l’ancienne).
À chaque version normale, remets aussi la release `avance` à jour : sinon elle paraît plus vieille et rien ne se propose.

## BHOST Setup sur ton PC

Pour essayer l'installeur sans publier (PowerShell, dans le dossier du projet) :
```
npm run tauri build
npm run setup
```
Résultat : `setup\target\release\bhost-setup.exe`. Il emporte l'installeur Windows de BHOST (`src-tauri\target\release\bundle\nsis\`)
et le lance sans fenêtre : mêmes raccourcis, même désinstallation, mêmes mises à jour. Sans WebView2, il ouvre l'installeur Windows classique.

## Machines dédiées (BHOST OS et VPS)

GitHub Actions joint tout seul à la Release (tâche « machines ») :
- `bhost-agent-linux-x86_64`, `.sha256` et `.sig` : l'agent, compilé pour une glibc ancienne (2.31, marche sur
  Ubuntu 22.04 et Debian 12) et **signé** avec la même clé que l'installeur. Les machines le trouvent dans
  BHOST › Machine › Rechercher, et l'installent avec « Mettre à jour… » (l'agent refuse un fichier mal signé).
- `installer-vps.sh` : la commande donnée par BHOST (Ajouter une machine › « Un VPS ? »).

À joindre à la main (Edit › glisser les fichiers) : `bhost-os.iso` et `bhost-os.iso.sha256` (voir `os/LISEZMOI.md`).
BHOST les télécharge à l'étape « Clé USB ».
