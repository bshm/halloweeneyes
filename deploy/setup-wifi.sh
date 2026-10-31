#!/bin/bash
# Configure WiFi on a running board: DHCP, autoconnect, wired off.
#
#   setup-wifi.sh <user@host> <ssid> <psk>
#
# Env: SSH_PORT (default 2222), BOARD_PASSWORD (default radxa)
set -euo pipefail

HOST=${1:?usage: setup-wifi.sh <user@host> <ssid> <psk>}
SSID=${2:?ssid required}
PSK=${3:?psk required}
PORT=${SSH_PORT:-2222}
PW=${BOARD_PASSWORD:-radxa}
KH=${KNOWN_HOSTS:-/tmp/eyes_known_hosts}

sshpass -p "$PW" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile="$KH" \
  -o ConnectTimeout=8 -p "$PORT" "$HOST" bash -s -- "$SSID" "$PSK" <<'EOF'
set -euo pipefail
SSID="$1"; PSK="$2"

# create or update the profile; DHCP + autoconnect
if nmcli -t -f NAME connection show | grep -qx "$SSID"; then
  sudo nmcli con mod "$SSID" \
    wifi.ssid "$SSID" wifi-sec.key-mgmt wpa-psk wifi-sec.psk "$PSK" \
    ipv4.method auto ipv4.addresses "" ipv4.gateway "" ipv4.dns "" ipv4.routes "" \
    connection.autoconnect yes connection.autoconnect-retries 0
else
  sudo nmcli con add type wifi ifname wlan0 con-name "$SSID" ssid "$SSID" \
    wifi-sec.key-mgmt wpa-psk wifi-sec.psk "$PSK" \
    ipv4.method auto connection.autoconnect yes connection.autoconnect-retries 0
fi
sudo nmcli con up "$SSID"
sleep 4

# the board must not depend on a cable
WIRED=$(nmcli -t -f NAME,TYPE connection show | awk -F: '$2=="802-3-ethernet"{print $1}')
for c in $WIRED; do
  sudo nmcli con mod "$c" connection.autoconnect no || true
done

ip -4 -br addr show wlan0
ip route show default
nmcli -t -f DEVICE,STATE,CONNECTION device status
EOF
