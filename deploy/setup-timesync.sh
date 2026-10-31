#!/bin/bash
# Install/point chrony at the LAN NTP server on a running board.
#
#   setup-timesync.sh <user@host> [ntp-server]
#
# systemd-timesyncd is disabled because it ignores the DHCP NTP option and
# picks random pool servers, which made the two boards disagree by ~8 ms.
set -euo pipefail

HOST=${1:?usage: setup-timesync.sh <user@host> [ntp-server]}
NTP=${2:-192.168.42.253}
PORT=${SSH_PORT:-2222}
PW=${BOARD_PASSWORD:-radxa}
KH=${KNOWN_HOSTS:-/tmp/eyes_known_hosts}

sshpass -p "$PW" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile="$KH" \
  -o ConnectTimeout=8 -p "$PORT" "$HOST" bash -s -- "$NTP" <<'EOF'
set -euo pipefail
NTP="$1"

if ! command -v chronyc >/dev/null; then
  sudo apt-get update -qq
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq chrony
fi

sudo tee /etc/chrony/chrony.conf >/dev/null <<CONF
server $NTP iburst minpoll 4 maxpoll 6 prefer
driftfile /var/lib/chrony/chrony.drift
makestep 1.0 3
rtcsync
logdir /var/log/chrony
CONF

sudo systemctl disable --now systemd-timesyncd 2>/dev/null || true
sudo systemctl enable chrony >/dev/null
sudo systemctl restart chrony
sleep 8

chronyc tracking | grep -E 'Reference ID|System time|Last offset'
chronyc sources | tail -2
EOF
