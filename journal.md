# left-eye deployment journal (ROCK 4D, halloween eyes kiosk)

Reproducible record of the working setup. Board: Radxa ROCK 4D (RK3576),
Debian 12 KDE image, headless over WiFi, boots straight into the eye kiosk.

## 1. Image and card

```
image   radxa-rk3576_bookworm_kde_b3.output_512.img.xz
sha256  156d93dc01f6406fd0e60b480bbc7b8dfd9cfc5a97dc96476e8677ff66c0ba7d
base    Debian 12 bookworm, kernel 6.1.84-12-rk2410-nocsf
```

Flash (card = USB reader, verify device first with `lsblk`):

```bash
xz -t radxa-rk3576_bookworm_kde_b3.output_512.img.xz
xz -dc radxa-rk3576_bookworm_kde_b3.output_512.img.xz \
  | sudo dd of=/dev/sdX bs=4M conv=fsync
# image GPT ends at 8 GB; move backup header so the card can grow
sudo sgdisk -e /dev/sdX
```

Card layout after flash: `p1 config 16M vfat`, `p2 efi 300M vfat`, `p3 rootfs ext4`.
Rootfs auto-grows to full card on first boot (verified: 7.2G -> 58G).

Notes:
- Host `e2fsck`/`resize2fs` (1.46.5) cannot touch this filesystem (`FEATURE_C12`);
  use the board's own tools (bookworm 1.47) or `growpart`/`resize2fs` on the board.
- First flash ran 2–20 MiB/s; a slow card is normal, but ext4 corruption afterwards
  was not — see §9.

## 2. Headless access (before first boot, offline on the card)

Rootfs mounted at `/mnt/eye`:

```bash
sudo mount /dev/sdX3 /mnt/eye
# ssh enabled (image ships it disabled by rsetup; see §8)
sudo ln -sf /lib/systemd/system/ssh.service \
  /mnt/eye/etc/systemd/system/multi-user.target.wants/ssh.service
sudo install -m 644 10-headless.conf \
  /mnt/eye/etc/ssh/sshd_config.d/10-headless.conf
# passwordless sudo
echo 'radxa ALL=(ALL:ALL) NOPASSWD: ALL' | \
  sudo tee /mnt/eye/etc/sudoers.d/90-radxa-nopasswd >/dev/null
sudo chmod 440 /mnt/eye/etc/sudoers.d/90-radxa-nopasswd
# hostname
echo left-eye | sudo tee /mnt/eye/etc/hostname
sudo sed -i 's/^127\.0\.1\.1.*/127.0.1.1\tleft-eye/' /mnt/eye/etc/hosts
sudo umount /mnt/eye
```

`10-headless.conf`:

```
PasswordAuthentication yes
KbdInteractiveAuthentication no
PermitRootLogin no
PubkeyAuthentication yes
```

Default credential: `radxa` / `radxa` (yescrypt hash verified byte-exact).

## 3. WiFi

Profile `/etc/NetworkManager/system-connections/TODOFixLater.nmconnection`, mode 600:

```ini
[connection]
id=TODOFixLater
type=wifi
autoconnect=true
autoconnect-retries=0
[wifi]
mode=infrastructure
ssid=TODOFixLater
[wifi-security]
key-mgmt=wpa-psk
psk=<redacted>
[ipv4]
method=auto
```

- DHCP, not static. Router reserved the address; board came up as `192.168.42.92`.
- Wired `end0` autoconnect is off (`nmcli con mod 'Wired connection 1'
  connection.autoconnect no`) so the board never depends on a cable.
- DNS from DHCP = `192.168.42.253` (FRITZ!Box 7490). `fritz.box` resolves.

Measured WiFi-only boot: **25.3 s** from reboot to port 22 open (24.9 s from link down).

## 4. Runtime dependencies

```bash
sudo apt-get update
sudo apt-get install -y ffmpeg      # required: detector spawns ffmpeg
```

`cage`/`grim` were installed while testing Wayland, not needed in the final setup.

## 5. Build (on x86, cross-compile)

```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install -y gcc-aarch64-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release -p detector --target aarch64-unknown-linux-gnu
(cd crates/frontend && trunk build --release)
```

## 6. Deploy layout on the board

Detector serves `ServeDir::new("crates/frontend/dist")` — a **relative** path, so the
working directory must be the parent of `crates/`.

```
/opt/eyes/detector
/opt/eyes/detector.env                 (mode 600, root)
/opt/eyes/kiosk.sh                     (mode 755)
/opt/eyes/crates/frontend/dist/...     (trunk output, owned by uid 1000)
```

`/opt/eyes/detector.env`:

```
RTSP_URL=rtsp://admin:admin@192.168.42.115:554/live/mpeg4
```

## 7. Services

`/etc/systemd/system/eyes-detector.service`:

```ini
[Unit]
Description=halloween eyes detector
After=network-online.target
Wants=network-online.target

[Service]
User=radxa
WorkingDirectory=/opt/eyes
EnvironmentFile=/opt/eyes/detector.env
Environment=RUST_LOG=detector=info
ExecStart=/opt/eyes/detector 0.0.0.0:8080 ${RTSP_URL}
Restart=always
RestartSec=2

[Install]
WantedBy=multi-user.target
```

`/etc/systemd/system/eyes-kiosk.service` — X11 kiosk (Chromium has **no Wayland ozone
backend** in this Debian build, so `cage` is impossible):

```ini
[Unit]
Description=halloween eyes kiosk (xinit + chromium)
After=eyes-detector.service systemd-user-sessions.service
Wants=eyes-detector.service
Conflicts=getty@tty1.service gdm.service

[Service]
User=radxa
PAMName=login
TTYPath=/dev/tty1
StandardInput=tty
StandardOutput=journal
StandardError=journal
TTYReset=yes
TTYVHangup=yes
TTYVTDisallocate=yes
Environment=XDG_RUNTIME_DIR=/run/user/1000
Environment=HOME=/home/radxa
ExecStart=/usr/bin/xinit /opt/eyes/kiosk.sh -- :0 vt1 -keeptty -nolisten tcp -nocursor
Restart=always
RestartSec=3

[Install]
WantedBy=graphical.target
```

`/opt/eyes/kiosk.sh`:

```sh
#!/bin/sh
xset s off
xset -dpms
xset s noblank
xrandr --output HDMI-1 --mode 1920x1080 >/dev/null 2>&1
exec /usr/bin/chromium \
  --kiosk --start-fullscreen --window-position=0,0 --window-size=1920,1080 \
  --force-device-scale-factor=1 \
  http://127.0.0.1:8080/ \
  --no-first-run --noerrdialogs --disable-infobars \
  --disable-session-crashed-bubble --password-store=basic \
  --check-for-update-interval=31536000 \
  --disable-features=Translate,MediaRouter,GlobalMediaControls,WebAppInstallation \
  --remote-debugging-port=9222 --remote-allow-origins=*
```

`--app=` was dropped on purpose: it adds a Chromium app-menu glyph in the lower right.
The page's own fullscreen button (`#fullscreen-btn`) was removed from
`crates/frontend/index.html` for the same reason.

Enable:

```bash
sudo systemctl enable eyes-detector eyes-kiosk
```

## 8. Display manager and rsetup

Two traps found on this image:

1. `sddm` is the active display manager (the KDE image also ships gdm). It holds the
   seat/DRM, so the kiosk fails until it is off:
   ```bash
   sudo systemctl disable --now sddm
   sudo systemctl mask sddm
   sudo rm -f /etc/systemd/system/display-manager.service
   ```
2. `rsetup.service` runs `/usr/bin/rsetup __on_boot` at every boot and **disables
   ssh.service** (`Removed .../multi-user.target.wants/ssh.service`,
   `ssh.service is a disabled or a static unit not running`). A normal
   `systemctl enable ssh` therefore does not survive a reboot.

Workaround: an independent unit name that rsetup does not touch,
`/etc/systemd/system/eyes-sshd.service`:

```ini
[Unit]
Description=eyes sshd (independent of rsetup)
After=network.target

[Service]
ExecStart=/usr/sbin/sshd -D -e -p 22 -p 2222
ExecReload=/bin/kill -HUP $MAINPID
KillMode=process
Restart=on-failure
RestartSec=2
RuntimeDirectory=sshd
RuntimeDirectoryMode=0755

[Install]
WantedBy=multi-user.target
```

`RuntimeDirectory=sshd` is required, otherwise `sshd` fails with
`Missing privilege separation directory: /run/sshd`.
Port 2222 kept as fallback.

## 9. Incident: filesystem corruption after first deploy

Symptom: after reboot the board stayed on the console, no network on either interface.

Evidence from the card journal (read with newer `journalctl` in a container, host tooling
could not parse the journal format):

```
EXT4-fs error (device mmcblk1p3): __ext4_find_entry:1693: inode #261406
  checksumming directory block 0
EXT4-fs (mmcblk1p3): error count since last fsck: 2605
systemd: Failed to open "/etc/systemd/system": Bad message
```

`e2fsck -fn` reported garbage extent trees across many inodes. Recovery: reflash the
image, re-apply §2–§7 offline, boot. Rootfs regrew to 58G by itself.

Reading card journals from an x86 host with older systemd:

```bash
docker run --rm -v /mnt/eye/var/log/journal:/j:ro debian:bookworm-slim \
  bash -c "apt-get update -qq && apt-get install -y -qq systemd && \
           journalctl --directory=/j --list-boots"
```

## 10. Verification

```bash
curl -sI http://127.0.0.1:8080/            # 200
systemctl is-active eyes-detector eyes-kiosk
ss -ltn | grep -E ':(22|8080|9222)'
```

Rendering proof over CDP through an SSH tunnel
(`ssh -L 19222:127.0.0.1:9222 -p 2222 radxa@192.168.42.92`):

```
Page.captureScreenshot                 -> 1919x1079 PNG
WebGL2 renderer                        -> ANGLE (Mesa, Mali-G52 r1 (Panfrost), OpenGL ES 3.1)
```

Chromium binds remote debugging to 127.0.0.1 only; the tunnel is mandatory.

## 11. Camera

- AVTech AVN812 (`00:0E:53:EB:F4:C4`), firmware `1062-1028-1041-1008`, EOL.
- Stream used: `rtsp://admin:admin@192.168.42.115:554/live/mpeg4` (1280x1024).
- It needs the Ethernet cable; the board uses WiFi only, so one cable is enough.
- With the camera absent the detector logs `ffmpeg read error: early eof, restarting`
  and the eye falls back to autonomous simulation — expected, not a fault.
- Camera NTP: `fritz.box` via router DNS `192.168.42.253`, DST off (firmware applies
  DST rules one hour early; offset must be changed manually in spring/autumn).

## 12. Measurements

| metric | value |
|---|---|
| kernel boot | 2.96 s |
| userspace -> graphical.target | 10.80 s (target 10.61 s) |
| reboot -> sshd :22 (WiFi) | 21.3 s |
| reboot -> detector :8080 | 24.1 s |
| kiosk restart -> CDP page | 1.9 s |
| kiosk restart -> canvas ready | +0.1 s |
| boot -> CDP page (tunnel) | 33.7 s |
| page render | 60.7 fps at 1919x1079 |
| GPU | Mali-G52 r1, Panfrost, Mesa 24.2, 500 MHz (max 900) |
| SoC temp | 46–47 °C |
| CPU idle | 90 % (9 % user, 2 % sys) |
| RAM | 623 MiB / 3919 MiB |
| mmc IO | ~1.5 MB read / 3 s, 0 write during idle |
| image write | ~8.5 MiB/s sustained, 7.7 GiB |

Both "eyes visible after boot" and "eyes move" were verified by frame diffs over CDP
(3 distinct captures). Note: Chromium binds CDP to `127.0.0.1`, so remote probes to
`192.168.42.92:9222` always time out — use the SSH tunnel.


## 13. Fixed technical debt

- **Camera outage no longer freezes the eye.** `run_camera` now drives the simulation from
  a 33 ms `tokio::select!` ticker independent of frame arrival, so after
  `simulation_timeout` (2 s) the autonomous animation continues with no frames.
  Verified with a blackhole route: frames kept changing during the cut and after restore.
- **Frame-stall watchdog**: if no frame arrives for 10 s, ffmpeg is killed, reaped
  (`child.wait()`) and restarted after 2 s. Observed live on a camera power cycle:
  `no frame for 10.03 s — restarting ffmpeg`, then a fresh RTSP open and normal motion,
  with no service restart.
- **WebSocket auto-reconnect** in the frontend (`transport.rs`): on close it reconnects
  after 1 s and re-attaches handlers. No page reload needed after a detector restart.
- **Credentials out of `ps`**: detector reads `RTSP_URL` from the environment when no
  CLI argument is given; the service now starts as `detector 0.0.0.0:8080`.
  Verified: `/proc` shows no RTSP URL.

## 14. Synchronised playback

Detector stamps every broadcast with `t_ms` (process monotonic ms) and exposes
`GET /time` returning the same clock. Each client measures its offset to that clock
over the WebSocket/HTTP path (5 samples, minimum RTT wins), then renders the state
produced `PLAYBACK_DELAY_MS = 100` ms ago, linearly interpolating between the two
straddling samples. Because the delay is identical on both boards and the timeline
is the detector's, transit jitter no longer shifts the phase.

Measured phase error left vs right (30 paired reads of the displayed `t_ms`,
projected onto a common host clock):

```
median -5.9 ms   mean -1.3 ms   p95 |delta| 9.6 ms   max 9.3 ms
```

Clock backing: chrony against `fritz.box` on both boards, ~0.14 ms relative error.
`window.eyeDebug` exposes `{t_ms, look_x, look_y, blink}` for measurement.

## 15. Open items

- Automated setup script (idempotent) for card prep + deploy + verification.
- Remove the Chromium remote-debugging port from the kiosk once verification is done.
- Detector still exits if `ffmpeg` is missing (`Restart=always` → start limit).
- `ffmpeg` software-decodes MPEG4 at ~49 % CPU on one core; H.264 stream or lower fps
  would cut that.
- Right-eye uses the same card image with `hostnamectl set-hostname right-eye`, no
  `eyes-detector`, and a kiosk URL pointing at `http://192.168.42.92:8080/`.
- `detector.env` holds camera credentials; `ps` still exposes them in `ExecStart`
  expansion.
