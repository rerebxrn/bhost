# BHOST OS

L'ISO à installer sur une machine dédiée (vieux PC, mini-PC, portable) : elle devient un serveur Minecraft piloté depuis BHOST.

- **Base** : Ubuntu 24.04 LTS minimale (famille Debian), sans interface graphique, clavier AZERTY, heure de Paris.
- **Installation** : la clé démarre sur l'installeur (`fichiers/usr/local/sbin/bhost-installeur`). Questions : le Wi-Fi (seulement sans câble et sans `bhost/wifi.txt` sur la clé), puis le disque (entièrement effacé). Aucun compte, aucun mot de passe.
- **Machine installée** : `bhost-agent` (service systemd, utilisateur `bhost`, données dans `/var/lib/bhost`) se connecte au relais ; `bhost-ecran` affiche sur l'écran le code de la machine, un QR code et l'état.
- **Réglages serveur** : processeur à pleine vitesse, `noatime`, chien de garde, mises à jour la nuit sans redémarrage, capot fermé ignoré, pas de veille.
- **Démarrage** : BIOS et UEFI. Si BHOST OS est déjà installé et que la clé est restée branchée, la clé démarre la machine installée (après 5 s).

## Fabriquer l'ISO

Sur Ubuntu 24.04 (ou une machine virtuelle Linux), en root :

```
apt install debootstrap squashfs-tools xorriso grub-pc-bin grub-efi-amd64-bin mtools dosfstools
cargo build --release --no-default-features --features agent --bin bhost-agent   # dans src-tauri
./construire.sh ../src-tauri/target/release/bhost-agent /tmp/bhost-os
```

Résultat : `/tmp/bhost-os/bhost-os.iso`.
