#!/bin/bash
# Apply headless + kiosk configuration to a card already written by `just flash`.
#
#   apply-card.sh <rootfs-partition> <mode> <hostname> [kiosk-url]
#     mode = headless | eyes-left | eyes-right
#
# Environment:
#   WIFI_FILE  pre-generated NetworkManager keyfile (see `just wifi-file`)
#   RTSP_URL   camera URL, required for eyes-left
set -euo pipefail

ROOT_PART=${1:?usage: apply-card.sh <rootfs-partition> <headless|eyes-left|eyes-right> <hostname> [kiosk-url]}
MODE=${2:?mode required}
HOST=${3:?hostname required}
KIOSK_URL=${4:-http://127.0.0.1:8080/}

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/.." && pwd)
M=$(mktemp -d)

case "$MODE" in
  headless|eyes-left|eyes-right) ;;
  *) echo "unknown mode: $MODE" >&2; exit 1 ;;
esac

sudo mount "$ROOT_PART" "$M"
trap 'sudo umount "$M"' EXIT

# --- identity -------------------------------------------------------------
echo "$HOST" | sudo tee "$M/etc/hostname" >/dev/null
sudo sed -i "s/^127\.0\.1\.1.*/127.0.1.1\t$HOST/" "$M/etc/hosts"

# --- ssh ------------------------------------------------------------------
# rsetup removes ssh.service at every boot, so eyes-sshd is the durable unit.
sudo install -m 644 "$HERE/10-headless.conf" "$M/etc/ssh/sshd_config.d/10-headless.conf"
sudo ln -sf /lib/systemd/system/ssh.service "$M/etc/systemd/system/multi-user.target.wants/ssh.service"
sudo install -m 644 "$HERE/eyes-sshd.service" "$M/etc/systemd/system/eyes-sshd.service"
sudo ln -sf /etc/systemd/system/eyes-sshd.service "$M/etc/systemd/system/multi-user.target.wants/eyes-sshd.service"

# --- privileges -----------------------------------------------------------
sudo install -m 440 "$HERE/90-radxa-nopasswd" "$M/etc/sudoers.d/90-radxa-nopasswd"

# --- wifi -----------------------------------------------------------------
if [[ -n "${WIFI_FILE:-}" ]]; then
  sudo install -m 600 "$WIFI_FILE" \
    "$M/etc/NetworkManager/system-connections/TODOFixLater.nmconnection"
fi

# --- no display manager ---------------------------------------------------
sudo rm -f "$M/etc/systemd/system/display-manager.service"
sudo ln -sf /dev/null "$M/etc/systemd/system/gdm.service"
sudo ln -sf /dev/null "$M/etc/systemd/system/sddm.service"

# --- kiosk ----------------------------------------------------------------
if [[ "$MODE" != headless ]]; then
  sudo mkdir -p "$M/opt/eyes/crates/frontend"
  sudo cp -r "$REPO/crates/frontend/dist" "$M/opt/eyes/crates/frontend/"
  sed "s|http://127.0.0.1:8080/|$KIOSK_URL|" "$HERE/kiosk.sh" \
    | sudo tee "$M/opt/eyes/kiosk.sh" >/dev/null
  sudo chmod 755 "$M/opt/eyes/kiosk.sh"
  sudo install -m 644 "$HERE/eyes-kiosk.service" "$M/etc/systemd/system/eyes-kiosk.service"
  sudo ln -sf /etc/systemd/system/eyes-kiosk.service \
    "$M/etc/systemd/system/graphical.target.wants/eyes-kiosk.service"
fi

# --- detector (left eye only) --------------------------------------------
if [[ "$MODE" == eyes-left ]]; then
  : "${RTSP_URL:?set RTSP_URL for eyes-left}"
  sudo install -m 755 \
    "$REPO/target/aarch64-unknown-linux-gnu/release/detector" "$M/opt/eyes/detector"
  printf 'RTSP_URL=%s\n' "$RTSP_URL" | sudo tee "$M/opt/eyes/detector.env" >/dev/null
  sudo chmod 600 "$M/opt/eyes/detector.env"
  sudo install -m 644 "$HERE/eyes-detector.service" "$M/etc/systemd/system/eyes-detector.service"
  sudo ln -sf /etc/systemd/system/eyes-detector.service \
    "$M/etc/systemd/system/multi-user.target.wants/eyes-detector.service"
fi

[[ -d "$M/opt/eyes" ]] && sudo chown -R 1000:1000 "$M/opt/eyes"
sudo sync

echo "applied: mode=$MODE host=$HOST kiosk=$KIOSK_URL"
