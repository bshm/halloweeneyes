# high level entry points for the halloween eyes boards
#
#   just            list recipes
#   just build      cross-compile detector + wasm frontend
#   just flash-left /dev/sdX
#   just flash-right /dev/sdX
#   just deploy left
#   just verify left
#
# WiFi password is never stored in the repo: export WIFI_PSK before card recipes,
# or pre-generate a keyfile with `just wifi-file` and pass WIFI_FILE.

set shell := ["bash", "-euo", "pipefail", "-c"]

image      := "radxa-rk3576_bookworm_kde_b3.output_512.img.xz"
left_ip    := "192.168.42.92"
left_host  := "radxa@" + left_ip
right_ip   := "192.168.42.93"
wifi_ssid  := "TODOFixLater"
ssh_port   := "2222"
cam_rtsp   := "rtsp://admin:admin@192.168.42.115:554/live/mpeg4"
wifi_tmpl  := "deploy/wifi.nmconnection.tmpl"
wifi_key   := "/tmp/eyes-wifi.nmconnection"

_default:
    @just --list

# --- artifacts ------------------------------------------------------------

# install toolchain bits and cross-compile both artefacts
build:
    rustup target add aarch64-unknown-linux-gnu
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
      cargo build --release -p detector --target aarch64-unknown-linux-gnu
    cd crates/frontend && trunk build --release
    @echo "artifacts: target/aarch64-unknown-linux-gnu/release/detector, crates/frontend/dist"

# verify the image archive before touching a card
image-check:
    xz -t {{image}}

# --- card images ----------------------------------------------------------

# write the image to a card and extend the GPT to the card's end
flash dev:
    test -b {{dev}}
    xz -t {{image}}
    xz -dc {{image}} | sudo dd of={{dev}} bs=4M conv=fsync status=progress
    sudo sgdisk -e {{dev}}
    @echo "written to {{dev}}"

# generate a NetworkManager keyfile from the template (needs WIFI_PSK)
wifi-file:
    WIFI_PSK=$${WIFI_PSK:?'export WIFI_PSK first'}
    sed 's|__WIFI_PSK__|'"$$WIFI_PSK"'|' {{wifi_tmpl}} > {{wifi_key}}
    chmod 600 {{wifi_key}}
    @echo "keyfile: {{wifi_key}}"

# configure WiFi on a running board (DHCP, autoconnect, wired disabled)
#   just wifi host=radxa@192.168.42.92 ssid=TODOFixLater psk=...
wifi host="" ssid=wifi_ssid psk="":
    @set -euo pipefail; \
      h="{{left_host}}"; [[ -n "{{host}}" ]] && h="{{host}}"; \
      p="{{psk}}"; [[ -n "$$p" ]] || p="$${WIFI_PSK:?'pass psk=... or export WIFI_PSK'}"; \
      SSH_PORT={{ssh_port}} deploy/setup-wifi.sh "$$h" "{{ssid}}" "$$p"

# headless only: ssh + sudo + wifi, no eyes
card-headless dev host="left-eye":
    @[[ -n "$${WIFI_FILE:-}" ]] || just wifi-file
    WIFI_FILE=$${WIFI_FILE:-{{wifi_key}}} deploy/apply-card.sh {{dev}}3 headless {{host}}

# left eye: detector + kiosk, camera URL from CAM_RTSP
card-left dev host="left-eye":
    @[[ -n "$${WIFI_FILE:-}" ]] || just wifi-file
    WIFI_FILE=$${WIFI_FILE:-{{wifi_key}}} RTSP_URL=$${CAM_RTSP:-{{cam_rtsp}}} \
      deploy/apply-card.sh {{dev}}3 eyes-left {{host}} http://127.0.0.1:8080/

# right eye: kiosk only, renders the state served by the left eye
card-right dev host="right-eye" kiosk="":
    @[[ -n "$${WIFI_FILE:-}" ]] || just wifi-file
    @set -euo pipefail; \
      k="{{kiosk}}"; [[ -n "$$k" ]] || k="http://{{left_ip}}:8080/"; \
      WIFI_FILE=$${WIFI_FILE:-{{wifi_key}}} deploy/apply-card.sh {{dev}}3 eyes-right {{host}} "$$k"

# flash + prepare in one go
flash-left dev:
    just flash {{dev}}
    just card-left {{dev}}

flash-right dev:
    just flash {{dev}}
    just card-right {{dev}}

# --- running boards -------------------------------------------------------

# push artefacts + units to a board and restart services
deploy role="left" host="":
    @set -euo pipefail; \
      h="{{left_host}}"; [[ "{{role}}" == "right" ]] && h="radxa@{{right_ip}}"; [[ -n "{{host}}" ]] && h="{{host}}"; \
      SSH_PORT={{ssh_port}} deploy/install-board.sh "$$h" {{role}}

# reachability, services, renderer and camera checks
verify host="":
    @set -euo pipefail; \
      h="{{left_host}}"; [[ -n "{{host}}" ]] && h="{{host}}"; \
      sshpass -p radxa ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/tmp/eyes_known_hosts -p {{ssh_port}} "$$h" \
      'hostname; ip -4 -br addr; \
       systemctl is-active eyes-detector eyes-kiosk eyes-sshd; \
       curl -s -o /dev/null -w "detector / %{http_code}\n" http://127.0.0.1:8080/; \
       DISPLAY=:0 XAUTHORITY=/home/radxa/.Xauthority xrandr | head -2'

# camera stream check from the board that runs the detector
verify-camera host="":
    @set -euo pipefail; \
      h="{{left_host}}"; [[ -n "{{host}}" ]] && h="{{host}}"; \
      sshpass -p radxa ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/tmp/eyes_known_hosts -p {{ssh_port}} "$$h" \
      'timeout 20 ffmpeg -hide_banner -loglevel error -rtsp_transport tcp \
         -i "$${CAM_RTSP:-{{cam_rtsp}}}" -t 6 -vf scale=320:240 -f rawvideo -pix_fmt rgb24 - \
         | wc -c | awk "{print \"bytes:\", \$1, \"frames:\", \$1/230400}"'
