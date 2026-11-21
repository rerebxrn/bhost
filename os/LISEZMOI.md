# BHOST OS

L'ISO à installer sur une machine dédiée (vieux PC, mini-PC, portable) : elle devient un serveur Minecraft piloté depuis BHOST.

- **Base** : Ubuntu 24.04 LTS minimale (famille Debian), sans interface graphique, clavier AZERTY, heure de Paris.
- **Démarrage** : BIOS et UEFI, **Secure Boot accepté** (shim et GRUB signés, noyau signé). DVD ou clé USB. Si BHOST OS est déjà installé et que la clé est restée branchée, la clé démarre la machine installée (après 5 s).
- **Installation** : la clé démarre sur l'installeur (`fichiers/usr/local/sbin/bhost-installeur`). Questions : le Wi-Fi (seulement sans câble et sans Wi-Fi écrit sur la clé), puis le disque (entièrement effacé). Aucun compte, aucun mot de passe.
- **Wi-Fi sur la clé** : l'ISO contient `bhost/wifi.txt`, une zone de 4 096 octets qui commence par `# BHOST-WIFI v1`. BHOST la remplit (`nom=`, `motdepasse=`) en écrivant la clé (`src-tauri/src/cle.rs`), sans refaire l'ISO.
- **Machine installée** : `bhost-agent` (service systemd, utilisateur `bhost`, données dans `/var/lib/bhost`) se connecte au relais ; `bhost-ecran` affiche sur l'écran le code de la machine, un QR code et l'état.
- **Limites par serveur** : un groupe de contrôle (cgroup v2) par serveur, confié à l'agent par systemd (`Delegate=yes`) : mémoire max (RAM du serveur + 1 Go pour Java) et part du processeur (priorité Basse / Normale / Haute).
- **Page « Machine » de BHOST** : l'agent écrit sa demande dans `/var/lib/bhost/systeme/demande` ; `bhost-systeme` (root, lancé par `bhost-systeme.path`) ne comprend que `redemarrer`, `eteindre`, `verifier`, `mettre-a-jour`, et écrit l'état dans `/run/bhost-systeme/etat.json`.
- **Pilotage** : chiffré (TLS 1.3, `src-tauri/src/chiffre.rs`), par le relais ou en direct sur le port **4446** (même réseau, ou VPS). La machine se présente avec sa clé ed25519 (celle de son id) ; BHOST la retient. Puis BHOST donne le code (5 codes faux par adresse : 15 minutes d'attente).
- **Fichiers envoyés par BHOST** (mondes, plugins, mods, icône) : par morceaux dans `/var/lib/bhost/envois/`, puis rangés et effacés.
- **Réglages serveur** : processeur à pleine vitesse, `noatime`, chien de garde, mises à jour la nuit sans redémarrage, capot fermé ignoré, pas de veille.

## Fabriquer l'ISO

Sur Ubuntu 24.04 (ou une machine virtuelle Linux), en root :

```
apt install debootstrap squashfs-tools xorriso grub-pc-bin grub-efi-amd64-bin mtools dosfstools
cargo build --release --no-default-features --features agent --bin bhost-agent   # dans src-tauri
./construire.sh ../src-tauri/target/release/bhost-agent /tmp/bhost-os
```

Résultat : `/tmp/bhost-os/bhost-os.iso`. À publier dans la Release GitHub avec `bhost-os.iso.sha256` (`sha256sum bhost-os.iso > bhost-os.iso.sha256`) : BHOST les télécharge à l'étape « Clé USB ».

Essais seulement : `RELAIS_ESSAI=10.0.2.2:4499` avec un agent compilé en mode debug (autre relais).

## VPS (sans ISO)

`installer-vps.sh` fait d'un VPS Ubuntu 22.04+ ou Debian 12+ (x86_64) une machine BHOST : l'agent, `bhost-systeme`, `bhost-performances`, le pare-feu (4446, 25565-25665, 19132-19232) s'il y en a un. BHOST donne la commande (avec un code tiré au hasard) : à coller dans le terminal ou dans le script « au démarrage » (cloud-init). Sur une adresse publique, « Ouvert à tes amis » donne l'adresse du VPS (sans box ni relais). Les copies de `bhost-systeme` et `bhost-performances` doivent rester identiques à `fichiers/` (un test le vérifie).
