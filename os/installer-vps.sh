#!/bin/bash
# BHOST : fait d'un VPS Ubuntu ou Debian une machine BHOST (comme BHOST OS, sans réinstaller le système).
#
# BHOST donne la commande (Ajouter une machine › « Un VPS ? »), avec un code tiré au hasard :
#   curl -fsSL https://github.com/rerebxrn/bhost/releases/latest/download/installer-vps.sh | BHOST_CODE=K7M2Q9XR bash
# À coller dans le terminal du VPS, ou dans son script « au démarrage » (cloud-init, user data).
# Ensuite, dans BHOST : Ajouter (le code est déjà dans la case).
#
# Ce que ça installe : l'agent BHOST (service bhost-agent, utilisateur bhost, données dans /var/lib/bhost),
# le service qui redémarre et met à jour la machine depuis BHOST (bhost-systeme), et le processeur à pleine vitesse.
# Sur un VPS, l'adresse est publique : les amis viennent en direct (sans relais), et BHOST pilote la machine
# en direct (port 4446, chiffré) ou par le relais.
#
# Relancer la commande met l'agent à jour (les serveurs et le code restent).
# Les copies de bhost-systeme et bhost-performances ci-dessous sont celles de os/fichiers (un test le vérifie).
set -euo pipefail

SOI=https://github.com/rerebxrn/bhost/releases/latest/download/installer-vps.sh
DEPOT=${BHOST_DEPOT:-https://github.com/rerebxrn/bhost/releases/latest/download}
AGENT_URL=${BHOST_AGENT_URL:-$DEPOT/bhost-agent-linux-x86_64}
CODE=${BHOST_CODE:-}
DONNEES=/var/lib/bhost

dire() { printf '\033[1m%s\033[0m\n' "$*"; }
stop() {
	printf '\033[31m%s\033[0m\n' "$*" >&2
	exit 1
}

# Pas root : on relance avec sudo (la commande se colle telle quelle, même sans « sudo » devant).
if [ "$(id -u)" -ne 0 ]; then
	command -v sudo >/dev/null || stop "Lance cette commande en root."
	curl -fsSL "$SOI" | sudo BHOST_CODE="$CODE" BHOST_DEPOT="${BHOST_DEPOT:-}" BHOST_AGENT_URL="${BHOST_AGENT_URL:-}" bash
	exit $?
fi

# ---------- 1. Le système ----------
[ -r /etc/os-release ] || stop "Système inconnu : il faut Ubuntu ou Debian."
. /etc/os-release
case " ${ID:-} ${ID_LIKE:-} " in
*" debian "* | *" ubuntu "*) ;;
*) stop "Il faut Ubuntu (22.04 ou plus récent) ou Debian (12 ou plus récent)." ;;
esac
[ "$(uname -m)" = x86_64 ] || stop "Il faut un processeur x86_64 (Intel ou AMD) : $(uname -m) n'est pas encore pris en charge."
[ -d /run/systemd/system ] || stop "Il faut systemd."
[ -f /sys/fs/cgroup/cgroup.controllers ] || dire "Note : pas de cgroup v2, les limites par serveur seront coupées."
if [ -n "$CODE" ] && ! printf '%s' "$CODE" | grep -Eq '^[2-9A-HJ-NP-Z]{8}$'; then
	stop "Code invalide : $CODE"
fi

dire "BHOST : installation sur $(hostname) (${PRETTY_NAME:-Linux})…"
export DEBIAN_FRONTEND=noninteractive
# Seulement ce qui manque : certificats, curl, et OpenSSL 3 pour l'agent (libssl3, ou libssl3t64 sur les systèmes récents).
manque=""
[ -s /etc/ssl/certs/ca-certificates.crt ] || manque="$manque ca-certificates"
command -v curl >/dev/null || manque="$manque curl"
if [ -n "$manque" ] || ! ldconfig -p | grep -q 'libssl\.so\.3 '; then
	apt-get update -qq || true
	# shellcheck disable=SC2086
	[ -z "$manque" ] || apt-get install -y -qq --no-install-recommends $manque >/dev/null
	ldconfig -p | grep -q 'libssl\.so\.3 ' || apt-get install -y -qq --no-install-recommends libssl3t64 >/dev/null 2>&1 || apt-get install -y -qq --no-install-recommends libssl3 >/dev/null
fi

# ---------- 2. L'agent ----------
dire "Téléchargement de l'agent…"
curl -fsSL --retry 3 -o /usr/local/bin/bhost-agent.nouveau "$AGENT_URL"
if curl -fsSL -o /tmp/bhost-agent.sha256 "$AGENT_URL.sha256" 2>/dev/null; then
	attendu=$(cut -d' ' -f1 </tmp/bhost-agent.sha256)
	vu=$(sha256sum /usr/local/bin/bhost-agent.nouveau | cut -d' ' -f1)
	rm -f /tmp/bhost-agent.sha256
	[ "$attendu" = "$vu" ] || stop "Agent abîmé (empreinte différente). Relance la commande."
fi
chmod 755 /usr/local/bin/bhost-agent.nouveau
mv -f /usr/local/bin/bhost-agent.nouveau /usr/local/bin/bhost-agent

id bhost >/dev/null 2>&1 || useradd --system --home-dir "$DONNEES" --shell /usr/sbin/nologin bhost
install -d -o bhost -g bhost -m 750 "$DONNEES"
# Le code donné par BHOST : la machine le prend (BHOST le connaît déjà).
if [ -n "$CODE" ]; then
	printf '%s' "$CODE" >"$DONNEES/machine-code.txt"
	chown bhost:bhost "$DONNEES/machine-code.txt"
	chmod 600 "$DONNEES/machine-code.txt"
fi
# La page « Machine » de BHOST (version, redémarrer, mises à jour).
echo "vps-1" >/etc/bhost-os-version

# ---------- 3. Les services ----------
cat >/etc/systemd/system/bhost-agent.service <<'FIN'
[Unit]
Description=Agent BHOST
After=network-online.target time-sync.target
Wants=network-online.target

[Service]
User=bhost
Group=bhost
Environment=BHOST_DOSSIER=/var/lib/bhost
ExecStart=/usr/local/bin/bhost-agent
Restart=always
RestartSec=5
KillSignal=SIGTERM
TimeoutStopSec=90
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/bhost
PrivateTmp=yes
LimitNOFILE=65536
# Chaque serveur a son groupe (mémoire max, part du processeur) : systemd confie le groupe de l'agent à l'agent.
Delegate=yes

[Install]
WantedBy=multi-user.target
FIN
# Essais seulement : un autre relais (BHOST_RELAIS_ESSAI=10.0.2.2:4499).
rm -rf /etc/systemd/system/bhost-agent.service.d
if [ -n "${BHOST_RELAIS_ESSAI:-}" ]; then
	mkdir -p /etc/systemd/system/bhost-agent.service.d
	printf '[Service]\nEnvironment=BHOST_RELAIS=%s\n' "$BHOST_RELAIS_ESSAI" >/etc/systemd/system/bhost-agent.service.d/essai.conf
fi

cat >/etc/systemd/system/bhost-systeme.path <<'FIN'
[Unit]
Description=BHOST : demandes de l'agent (redémarrer, mises à jour…)

[Path]
PathExists=/var/lib/bhost/systeme/demande

[Install]
WantedBy=multi-user.target
FIN
cat >/etc/systemd/system/bhost-systeme.service <<'FIN'
[Unit]
Description=BHOST : demande de l'agent

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/bhost-systeme
FIN
cat >/etc/systemd/system/bhost-systeme-etat.service <<'FIN'
[Unit]
Description=BHOST : état des mises à jour
After=apt-daily-upgrade.service

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/bhost-systeme etat

[Install]
WantedBy=multi-user.target apt-daily-upgrade.service
FIN
cat >/etc/systemd/system/bhost-performances.service <<'FIN'
[Unit]
Description=BHOST : processeur à pleine vitesse

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/local/sbin/bhost-performances

[Install]
WantedBy=multi-user.target
FIN

mkdir -p /usr/local/sbin
cat >/usr/local/sbin/bhost-systeme <<'FIN'
#!/bin/sh
# BHOST OS : fait ce que l'agent demande depuis BHOST (page « Machine »), car l'agent n'a pas les droits.
# L'agent écrit un seul mot dans /var/lib/bhost/systeme/demande ; seuls ces mots sont compris :
#   redemarrer, eteindre, verifier (les mises à jour), mettre-a-jour.
# Sans argument : lancé par bhost-systeme.path quand une demande arrive. « etat » : relit seulement l'état.
# L'état (pour BHOST) est écrit dans /run/bhost-systeme/etat.json, que l'agent peut lire mais pas modifier.

DEMANDE=/var/lib/bhost/systeme/demande
DOSSIER=/run/bhost-systeme
export DEBIAN_FRONTEND=noninteractive LC_ALL=C

mkdir -p "$DOSSIER"
chmod 755 "$DOSSIER"

etat() {
	en_cours=$1
	attente=$(apt-get -s -o Debug::NoLocking=1 upgrade 2>/dev/null | grep -c '^Inst ')
	# Redémarrage nécessaire : un noyau plus récent que celui qui tourne est installé.
	dernier=$(ls /usr/lib/modules 2>/dev/null | sort -V | tail -1)
	redemarrage=false
	[ -n "$dernier" ] && [ "$dernier" != "$(uname -r)" ] && redemarrage=true
	verifie=$(stat -c %Y /var/lib/apt/periodic/update-success-stamp 2>/dev/null || echo 0)
	if [ -n "$en_cours" ]; then en_cours="\"$en_cours\""; else en_cours=null; fi
	printf '{"enCours":%s,"attente":%s,"redemarrage":%s,"verifie":%s}\n' "$en_cours" "${attente:-0}" "$redemarrage" "$verifie" >"$DOSSIER/etat.tmp"
	mv "$DOSSIER/etat.tmp" "$DOSSIER/etat.json"
	chmod 644 "$DOSSIER/etat.json"
}

if [ "${1:-}" = etat ]; then
	etat ""
	exit 0
fi

# Un vrai fichier seulement (pas un lien), lu sur quelques octets, puis effacé.
if [ ! -f "$DEMANDE" ] || [ -L "$DEMANDE" ]; then
	rm -f "$DEMANDE"
	exit 0
fi
action=$(head -c 20 "$DEMANDE" | tr -cd 'a-z-')
rm -f "$DEMANDE"

case "$action" in
	redemarrer) systemctl reboot ;;
	eteindre) systemctl poweroff ;;
	verifier)
		etat verification
		apt-get update -qq && mkdir -p /var/lib/apt/periodic && touch /var/lib/apt/periodic/update-success-stamp
		etat ""
		;;
	mettre-a-jour)
		etat installation
		apt-get update -qq && mkdir -p /var/lib/apt/periodic && touch /var/lib/apt/periodic/update-success-stamp
		apt-get -y -q -o Dpkg::Options::=--force-confdef -o Dpkg::Options::=--force-confold upgrade
		apt-get -y -q autoremove --purge
		etat ""
		;;
esac
exit 0
FIN
cat >/usr/local/sbin/bhost-performances <<'FIN'
#!/bin/sh
# BHOST OS : un serveur Minecraft a besoin de la vitesse maximale d'un cœur, tout le temps.
for g in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
	[ -w "$g" ] && echo performance >"$g" 2>/dev/null
done
for e in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do
	[ -w "$e" ] && echo performance >"$e" 2>/dev/null
done
# Les disques : pas d'économie d'énergie sur le lien SATA (évite des pauses).
for l in /sys/class/scsi_host/host*/link_power_management_policy; do
	[ -w "$l" ] && echo max_performance >"$l" 2>/dev/null
done
exit 0
FIN
chmod 755 /usr/local/sbin/bhost-systeme /usr/local/sbin/bhost-performances

# ---------- 4. Le pare-feu (s'il y en a un) ----------
# 4446 : BHOST en direct ; 25565-25665 : les serveurs Java ; 19132-19232 : les joueurs Bedrock.
if command -v ufw >/dev/null && ufw status 2>/dev/null | grep -q "Status: active"; then
	ufw allow 4446/tcp >/dev/null
	ufw allow 25565:25665/tcp >/dev/null
	ufw allow 19132:19232/udp >/dev/null
	ufw allow 19132:19232/tcp >/dev/null
elif command -v firewall-cmd >/dev/null && firewall-cmd --state >/dev/null 2>&1; then
	firewall-cmd -q --permanent --add-port=4446/tcp --add-port=25565-25665/tcp --add-port=19132-19232/udp --add-port=19132-19232/tcp
	firewall-cmd -q --reload
elif command -v iptables >/dev/null && iptables -S INPUT 2>/dev/null | grep -q -- "-j REJECT"; then
	# Images qui refusent tout par défaut (Oracle Cloud…) : on ouvre avant le refus, et on le garde.
	for r in "-p tcp --dport 4446" "-p tcp --dport 25565:25665" "-p udp --dport 19132:19232" "-p tcp --dport 19132:19232"; do
		# shellcheck disable=SC2086
		iptables -C INPUT $r -j ACCEPT 2>/dev/null || iptables -I INPUT $r -j ACCEPT
	done
	command -v netfilter-persistent >/dev/null && netfilter-persistent save >/dev/null 2>&1 || true
fi

# ---------- 5. C'est parti ----------
systemctl daemon-reload
systemctl enable -q bhost-performances bhost-systeme.path bhost-systeme-etat bhost-agent
systemctl start bhost-performances bhost-systeme.path || true
systemctl start bhost-systeme-etat || true
systemctl restart bhost-agent

# Le code (créé par l'agent s'il n'a pas été donné).
for _ in $(seq 1 30); do
	[ -s "$DONNEES/machine-code.txt" ] && break
	sleep 1
done
code=$(cat "$DONNEES/machine-code.txt" 2>/dev/null || true)
[ -n "$code" ] || stop "L'agent ne démarre pas : journalctl -u bhost-agent"
echo
dire "BHOST est prêt sur ce VPS."
if [ -n "$CODE" ]; then
	echo "Dans BHOST, clique sur Ajouter."
else
	echo "Dans BHOST : Ajouter une machine, code ${code:0:4}-${code:4:4}"
fi
