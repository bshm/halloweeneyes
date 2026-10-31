#!/bin/bash
# Copy the built artifacts and units to a running board and restart the services.
#
#   install-board.sh <host> <left|right>
set -euo pipefail

HOST=${1:?usage: install-board.sh <user@host> <left|right>}
ROLE=${2:?role: left|right}
PORT=${SSH_PORT:-2222}

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/.." && pwd)
SSH=(sshpass -p "${BOARD_PASSWORD:-radxa}" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/tmp/eyes_known_hosts -p "$PORT")
SCP=(sshpass -p "${BOARD_PASSWORD:-radxa}" scp -o StrictHostKeyChecking=no -o UserKnownHostsFile=/tmp/eyes_known_hosts -P "$PORT")

"${SSH[@]}" "$HOST" 'sudo mkdir -p /opt/eyes/crates/frontend && sudo chown -R 1000:1000 /opt/eyes
sudo timedatectl set-timezone Europe/Berlin 2>/dev/null || true'

if [[ "$ROLE" == left ]]; then
  "${SCP[@]}" "$REPO/target/aarch64-unknown-linux-gnu/release/detector" "$HOST:/tmp/detector.new"
  "${SSH[@]}" "$HOST" 'sudo install -m 755 /tmp/detector.new /opt/eyes/detector'
fi

"${SCP[@]}" -r "$REPO/crates/frontend/dist" "$HOST:/opt/eyes/crates/frontend/"
"${SCP[@]}" "$HERE/kiosk.sh" "$HOST:/tmp/kiosk.sh"
"${SCP[@]}" "$HERE/eyes-sshd.service" "$HOST:/tmp/eyes-sshd.service"
"${SCP[@]}" "$HERE/wifi-watchdog.sh" "$HOST:/tmp/wifi-watchdog.sh"
"${SCP[@]}" "$HERE/eyes-wifi-watchdog.service" "$HOST:/tmp/eyes-wifi-watchdog.service"
"${SCP[@]}" "$HERE/eyes-kiosk.service" "$HOST:/tmp/eyes-kiosk.service"
if [[ "$ROLE" == left ]]; then
  "${SCP[@]}" "$HERE/eyes-detector.service" "$HOST:/tmp/eyes-detector.service"
fi

"${SSH[@]}" "$HOST" bash -s -- "$ROLE" <<'EOF'
set -euo pipefail
ROLE="$1"
sudo install -m 755 /tmp/kiosk.sh /opt/eyes/kiosk.sh
if [ "$ROLE" = right ]; then
  sudo sed -i 's|http://127.0.0.1:8080/|http://192.168.42.92:8080/?eye=right|' /opt/eyes/kiosk.sh
fi
sudo install -m 644 /tmp/eyes-sshd.service /etc/systemd/system/eyes-sshd.service
sudo install -m 755 /tmp/wifi-watchdog.sh /opt/eyes/wifi-watchdog.sh
sudo install -m 644 /tmp/eyes-wifi-watchdog.service /etc/systemd/system/eyes-wifi-watchdog.service
sudo install -m 644 /tmp/eyes-kiosk.service /etc/systemd/system/eyes-kiosk.service
if [ "$ROLE" = left ]; then
  sudo install -m 644 /tmp/eyes-detector.service /etc/systemd/system/eyes-detector.service
fi
sudo chown -R 1000:1000 /opt/eyes
sudo timedatectl set-timezone Europe/Berlin 2>/dev/null || true
sudo systemctl daemon-reload
sudo systemctl enable eyes-sshd eyes-kiosk eyes-wifi-watchdog >/dev/null
if [ "$ROLE" = left ]; then
  sudo systemctl enable eyes-detector >/dev/null
  sudo systemctl restart eyes-detector
fi
sudo systemctl restart eyes-wifi-watchdog
sudo systemctl restart eyes-kiosk
sleep 4
systemctl is-active eyes-detector eyes-kiosk eyes-sshd 2>/dev/null || true
EOF

echo "installed on $HOST (role=$ROLE)"
