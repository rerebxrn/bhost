#!/bin/bash
# Fabrique l'ISO de BHOST OS (à lancer en root sur Ubuntu 24.04 ou Debian récent).
#   sudo ./construire.sh <bhost-agent> [dossier de travail]
# Résultat : <dossier de travail>/bhost-os.iso (démarre en BIOS et en UEFI, sur DVD ou clé USB).
#
# Base : Ubuntu 24.04 LTS minimale (famille Debian), sans interface graphique.
# Outils nécessaires : debootstrap squashfs-tools xorriso grub-pc-bin grub-efi-amd64-bin mtools dosfstools.
set -euo pipefail

AGENT=$(realpath "${1:?chemin du binaire bhost-agent (Linux x86_64)}")
TRAVAIL=$(realpath -m "${2:-./travail}")
ICI=$(dirname "$(realpath "$0")")
RACINE=$TRAVAIL/racine
ISO=$TRAVAIL/iso
MIROIR=http://archive.ubuntu.com/ubuntu
mkdir -p "$TRAVAIL"

dans() { chroot "$RACINE" env DEBIAN_FRONTEND=noninteractive LC_ALL=C "$@"; }
monter() {
	mountpoint -q "$RACINE/proc" || mount -t proc proc "$RACINE/proc"
	mountpoint -q "$RACINE/sys" || mount -t sysfs sys "$RACINE/sys"
	mountpoint -q "$RACINE/dev" || mount --bind /dev "$RACINE/dev"
	mountpoint -q "$RACINE/dev/pts" || mount --bind /dev/pts "$RACINE/dev/pts"
}
demonter() { for d in dev/pts dev sys proc; do umount "$RACINE/$d" 2>/dev/null || true; done; }
trap demonter EXIT

# ---------- 1. Le système de base ----------
if [ ! -x "$RACINE/usr/bin/apt-get" ]; then
	debootstrap --variant=minbase --components=main,universe --include=systemd-sysv,dbus,ca-certificates noble "$RACINE" "$MIROIR"
fi
cat >"$RACINE/etc/apt/sources.list" <<FIN
deb $MIROIR noble main universe
deb $MIROIR noble-updates main universe
deb http://security.ubuntu.com/ubuntu noble-security main universe
FIN
printf '#!/bin/sh\nexit 101\n' >"$RACINE/usr/sbin/policy-rc.d"
chmod +x "$RACINE/usr/sbin/policy-rc.d"
cp "$ICI/fichiers/etc/dpkg/dpkg.cfg.d/bhost-allege" "$RACINE/etc/dpkg/dpkg.cfg.d/" 2>/dev/null || true
monter
printf '%s\n' \
	'keyboard-configuration keyboard-configuration/layoutcode string fr' \
	'keyboard-configuration keyboard-configuration/modelcode string pc105' \
	'keyboard-configuration keyboard-configuration/variant select French' \
	'console-setup console-setup/charmap47 select UTF-8' | dans debconf-set-selections
dans apt-get update -q
dans apt-get install -y -q --no-install-recommends \
	udev kmod iproute2 locales console-setup kbd keyboard-configuration whiptail \
	network-manager wpasupplicant iw linux-image-generic initramfs-tools busybox-initramfs \
	grub-pc-bin grub-efi-amd64-bin grub-efi-amd64-signed shim-signed grub2-common \
	gdisk dosfstools e2fsprogs squashfs-tools util-linux libssl3t64 unattended-upgrades \
	tzdata zstd less nano curl libpam-systemd qrencode eject systemd-timesyncd systemd-resolved systemd-zram-generator

# ---------- 2. Alléger : pas de pilotes de son, de caméra, de cartes serveur, ni leurs microcodes ----------
K=$(ls "$RACINE/usr/lib/modules" | sort -V | tail -1)
# Un seul noyau (une reconstruction sur un dossier existant peut en avoir ajouté un) : l'installeur prend celui-là.
for v in $(ls "$RACINE/usr/lib/modules"); do
	[ "$v" = "$K" ] || dans apt-get purge -y -q "linux-image-$v" "linux-modules-$v" "linux-modules-extra-$v" >/dev/null 2>&1 || true
done
M=$RACINE/usr/lib/modules/$K/kernel
for d in sound drivers/media drivers/iio drivers/infiniband drivers/staging drivers/comedi drivers/mtd drivers/target \
	drivers/isdn drivers/net/can drivers/gpu/drm/amd zfs drivers/net/wireless/ti drivers/net/ethernet/mellanox \
	drivers/net/ethernet/chelsio drivers/net/ethernet/qlogic drivers/net/ethernet/cavium drivers/net/ethernet/netronome \
	drivers/net/ethernet/sfc; do
	[ -d "$M/$d" ] && find "$M/$d" -type f -delete
done
dans depmod "$K"
# Microcodes : seulement ceux que demandent les pilotes gardés (Wi-Fi, réseau, cartes graphiques Intel/anciennes AMD).
dans sh -c "find /usr/lib/modules/$K -name '*.ko*' | xargs -n 50 modinfo -F firmware 2>/dev/null" | sort -u >"$TRAVAIL/microcodes-voulus.txt"
python3 "$ICI/garder-microcodes.py" "$RACINE/usr/lib/firmware" "$TRAVAIL/microcodes-voulus.txt"
find "$RACINE/usr/lib/firmware" -xtype l -delete
find "$RACINE/usr/lib/firmware" "$M" -type d -empty -delete
dans apt-get clean
find "$RACINE/var/lib/apt/lists" "$RACINE/usr/share/doc" "$RACINE/usr/share/man" -type f -delete

# ---------- 3. BHOST OS ----------
cp -r "$ICI/fichiers/." "$RACINE/"
chmod 755 "$RACINE/usr/local/bin/bhost-ecran" "$RACINE/usr/local/sbin/bhost-"* "$RACINE/etc/kernel/postinst.d/zz-bhost-grub" "$RACINE/etc/kernel/postrm.d/zz-bhost-grub"
install -m 755 "$AGENT" "$RACINE/usr/local/bin/bhost-agent"
# Essais seulement (agent compilé en mode debug) : un autre relais, par exemple RELAIS_ESSAI=10.0.2.2:4499.
rm -rf "$RACINE/etc/systemd/system/bhost-agent.service.d"
if [ -n "${RELAIS_ESSAI:-}" ]; then
	mkdir -p "$RACINE/etc/systemd/system/bhost-agent.service.d"
	printf '[Service]\nEnvironment=BHOST_RELAIS=%s\n' "$RELAIS_ESSAI" >"$RACINE/etc/systemd/system/bhost-agent.service.d/essai.conf"
fi
chmod 600 "$RACINE/etc/netplan/"*.yaml
dans id bhost >/dev/null 2>&1 || dans useradd --system --home-dir /var/lib/bhost --shell /usr/sbin/nologin bhost
dans install -d -o bhost -g bhost -m 750 /var/lib/bhost
dans passwd -l root >/dev/null
# Français, heure de Paris, clavier AZERTY.
sed -i 's/^# *fr_FR.UTF-8/fr_FR.UTF-8/' "$RACINE/etc/locale.gen"
dans locale-gen >/dev/null
echo 'LANG=fr_FR.UTF-8' >"$RACINE/etc/default/locale"
ln -sf /usr/share/zoneinfo/Europe/Paris "$RACINE/etc/localtime"
echo Europe/Paris >"$RACINE/etc/timezone"
dans setupcon --save-only 2>/dev/null || true
# Services : l'agent, l'écran et les performances sur la machine installée ; l'installeur sur la clé.
dans systemctl enable bhost-agent bhost-ecran bhost-performances bhost-installeur bhost-systeme.path bhost-systeme-etat NetworkManager systemd-timesyncd systemd-resolved >/dev/null
dans systemctl mask getty@tty1.service sleep.target suspend.target hibernate.target hybrid-sleep.target >/dev/null
dans systemctl disable apt-daily.timer >/dev/null 2>&1 || true
dans systemctl enable apt-daily.timer apt-daily-upgrade.timer >/dev/null
ln -sf ../run/systemd/resolve/stub-resolv.conf "$RACINE/etc/resolv.conf"
# Chaque machine a son identité (créée au premier démarrage).
: >"$RACINE/etc/machine-id"
rm -f "$RACINE/var/lib/dbus/machine-id"
rm -rf "$RACINE/var/lib/bhost/"*
# L'image de démarrage (sait démarrer depuis la clé : scripts/bhost).
dans update-initramfs -u -k "$K"
rm -f "$RACINE/usr/sbin/policy-rc.d"
demonter

# ---------- 4. L'ISO ----------
# BIOS : GRUB (El Torito + MBR hybride pour la clé USB). UEFI : shim et GRUB signés (Secure Boot accepté).
# Sur la clé, la partition du système est de type Linux et il n'y a pas de partition de remplissage :
# Windows ne lui donne pas de lettre et ne propose pas de « formater le disque ».
rm -rf "$ISO"
mkdir -p "$ISO/boot/grub/i386-pc" "$ISO/bhost" "$ISO/.disk"
cp "$RACINE/boot/vmlinuz-$K" "$ISO/boot/vmlinuz"
cp "$RACINE/boot/initrd.img-$K" "$ISO/boot/initrd"
cp "$ICI/iso/grub.cfg" "$ISO/boot/grub/grub.cfg"
echo "BHOST OS $(cat "$ICI/fichiers/etc/bhost-os-version")" >"$ISO/.disk/info"
# La zone du Wi-Fi : BHOST la remplit en écrivant la clé (étape « Clé USB »), sans refaire l'ISO.
# Taille fixe ; la 1re ligne sert de repère pour la retrouver dans l'image.
python3 - "$ISO/bhost/wifi.txt" <<'FIN'
import sys
debut = b"# BHOST-WIFI v1\nnom=\nmotdepasse=\n"
open(sys.argv[1], "wb").write(debut + b"\n" * (4096 - len(debut)))
FIN
# Le noyau et l'image de démarrage ne sont pas dans le système (l'installeur les copie depuis la clé).
mksquashfs "$RACINE" "$ISO/bhost/systeme.squashfs" -comp xz -Xbcj x86 -b 1M -noappend -quiet \
	-e boot/vmlinuz-"$K" boot/initrd.img-"$K" boot/System.map-"$K" boot/vmlinuz boot/initrd.img boot/vmlinuz.old boot/initrd.img.old

# BIOS
cp /usr/lib/grub/i386-pc/*.mod /usr/lib/grub/i386-pc/*.lst "$ISO/boot/grub/i386-pc/"
grub-mkimage -O i386-pc-eltorito -p /boot/grub -o "$ISO/boot/grub/i386-pc/eltorito.img" \
	biosdisk iso9660 part_gpt part_msdos ext2 fat search search_fs_file configfile normal linux echo test
# UEFI : petite partition FAT avec shim (signé par Microsoft) → GRUB (signé par Canonical) → noyau signé.
ESP=$TRAVAIL/efi.img
rm -f "$ESP"
mkfs.vfat -C -n BHOST_EFI "$ESP" 10240 >/dev/null
mmd -i "$ESP" ::/EFI ::/EFI/BOOT ::/boot ::/boot/grub
mcopy -i "$ESP" "$RACINE/usr/lib/shim/shimx64.efi.signed.latest" ::/EFI/BOOT/BOOTX64.EFI
mcopy -i "$ESP" "$RACINE/usr/lib/shim/mmx64.efi" ::/EFI/BOOT/mmx64.efi
mcopy -i "$ESP" "$RACINE/usr/lib/grub/x86_64-efi-signed/gcdx64.efi.signed" ::/EFI/BOOT/grubx64.efi
# Le GRUB signé cherche /.disk/info puis /boot/grub/grub.cfg sur le DVD ; à défaut, ce petit relais.
printf 'search --no-floppy --file --set=root /.disk/info\nset prefix=($root)/boot/grub\nconfigfile $prefix/grub.cfg\n' >"$TRAVAIL/relais.cfg"
mcopy -i "$ESP" "$TRAVAIL/relais.cfg" ::/EFI/BOOT/grub.cfg
mcopy -i "$ESP" "$TRAVAIL/relais.cfg" ::/boot/grub/grub.cfg

rm -f "$TRAVAIL/bhost-os.iso"
xorriso -as mkisofs -iso-level 3 -r -J -joliet-long -V BHOST_OS \
	--grub2-mbr /usr/lib/grub/i386-pc/boot_hybrid.img -partition_offset 16 \
	-iso_mbr_part_type 0FC63DAF-8483-4772-8E79-3D69D8477DE4 \
	-append_partition 2 0xef "$ESP" -appended_part_as_gpt -no-pad \
	-c boot.catalog \
	-b boot/grub/i386-pc/eltorito.img -no-emul-boot -boot-load-size 4 -boot-info-table --grub2-boot-info \
	-eltorito-alt-boot -e '--interval:appended_partition_2:all::' -no-emul-boot \
	-o "$TRAVAIL/bhost-os.iso" "$ISO" 2>/dev/null
ls -lh "$TRAVAIL/bhost-os.iso"
