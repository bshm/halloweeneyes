# halloween eyes — rewrite

projection of animated moving eyes onto windows.
a camera tracks motion in front of the house and makes the eyes follow.

## architecture

```
┌──────────────────────────────────────────────────────────┐
│  ip camera (rtsp)                                        │
└─────────────────────┬────────────────────────────────────┘
                      │ rtsp stream (ffmpeg subprocess)
┌─────────────────────▼────────────────────────────────────┐
│  detector  (rust binary)                                 │
│                                                          │
│  capture → motion detection → coordinate mapping         │
│                  ↓ (no motion for 2s)                    │
│             eye simulation (autonomous animation)        │
│                  ↓                                       │
│  axum websocket server  →  EyeState JSON broadcasts      │
└─────────────────────┬────────────────────────────────────┘
                      │ ws://host/ws
         ┌────────────┴───────────┐
         │                        │
┌────────▼──────┐        ┌────────▼──────┐
│  browser      │        │  browser      │
│  (left eye)   │        │  (right eye)  │
│  leptos+wgpu  │        │  leptos+wgpu  │
└───────────────┘        └───────────────┘
```

**detector** — runs on any linux/mac/windows host with ffmpeg and an ip camera.

**frontend** — wasm compiled from rust via trunk; renders in firefox and chrome
using the webgl2 backend of wgpu. the same code compiles natively for a future
tauri standalone application.

## prerequisites

| tool | install |
|---|---|
| rust 1.89+ | https://rustup.rs |
| wasm32 target | `rustup target add wasm32-unknown-unknown` |
| trunk | `cargo install trunk` |
| ffmpeg | `sudo apt install ffmpeg` / `brew install ffmpeg` |
| ip camera | any camera with an rtsp endpoint |

## build

### detector

```sh
cargo build --release -p detector
```

binary: `target/release/detector`

### frontend

```sh
cd crates/frontend
trunk build --release
```

output: `crates/frontend/dist/`

## run

### start the detector

```sh
./target/release/detector <bind-addr> [rtsp-url]
```

with camera:
```sh
./target/release/detector 0.0.0.0:8080 rtsp://admin:admin@192.168.1.33/live/h264
```

simulation-only (no camera required):
```sh
./target/release/detector 0.0.0.0:8080
```

the detector:
- opens the rtsp stream via ffmpeg
- runs the motion detection pipeline
- falls back to autonomous eye simulation after 2 s of no motion
- serves the frontend at `http://<bind-addr>/`
- broadcasts eye state over websocket at `ws://<bind-addr>/ws`

### open the renderer

open `http://<host>:8080` in firefox or chrome.
open on multiple screens / devices to render multiple eyes simultaneously — each
browser tab receives the same eye state from the websocket broadcast.

### development mode (frontend hot-reload)

```sh
# terminal 1 — run detector in simulation-only mode (or pass rtsp-url as second arg)
./target/release/detector 0.0.0.0:8080

# terminal 2 — trunk dev server with hot-reload
cd crates/frontend
trunk serve --port 8081
```

open `http://localhost:8081`.

## assets

`crates/frontend/public/` contains the eye imagery ported from the original project:

| file | size | description |
|---|---|---|
| `BG.png` | 2000×2000 px RGBA | background / eyelid mask |
| `IRIS.png` | 2000×2000 px RGBA | iris overlay |

the current wgpu shader renders a procedural eye; texture sampling from these
images is the next implementation step.

## project layout

```
Cargo.toml                       workspace root
crates/
  shared/                        EyeState + ServerMessage types
  detector/
    src/
      coordinate.rs              camera-to-eye polynomial mapping
      motion.rs                  frame-diff motion detection pipeline
      simulation.rs              autonomous eye/blink animation
      capture.rs                 async ffmpeg rtsp reader
      server.rs                  axum websocket broadcaster
      main.rs                    entry point
  frontend/
    public/
      BG.png                     eyelid mask (from original project)
      IRIS.png                   iris image (from original project)
    src/
      lib.rs                     leptos app component
      transport.rs               EyeStateSource trait (websocket impl)
      render.rs                  wgpu renderer (webgl2 / native)
      eye.wgsl                   wgsl shader
    index.html                   trunk entry point
    Trunk.toml                   trunk configuration
```

## tauri (future)

the `EyeStateSource` trait in `crates/frontend/src/transport.rs` abstracts the
transport. swap `WebSocketSource` for a native tauri ipc implementation and the
rendering code requires no changes.

## deployment (ROCK 4D, headless kiosk)

Two boards, one detector: `left-eye` runs the detector and talks to the camera,
`right-eye` only renders the state broadcast by the left eye.

```sh
# build detector (aarch64) + wasm frontend
just build

# check the image archive
just image-check

# write a card and prepare it
just flash-left  /dev/sdX                # detector + kiosk on this card
just flash-right /dev/sdX                # kiosk only, points at left-eye

# or step by step
just flash /dev/sdX
export WIFI_PSK=...                      # never stored in the repo
just card-left  /dev/sdX host=left-eye
just card-right /dev/sdX host=right-eye

# configure WiFi on a board that is already running
just wifi host=radxa@192.168.42.92 ssid=TODOFixLater psk=...

# push freshly built artefacts to a running board
just deploy left
just deploy right
just verify
just verify-camera
```

What lands on the card:

| piece | left-eye | right-eye |
|---|---|---|
| `eyes-detector.service` (camera + websocket) | yes | no |
| `eyes-kiosk.service` (`xinit` + Chromium kiosk) | yes | yes |
| `eyes-sshd.service` (survives `rsetup`) | yes | yes |
| kiosk URL | `http://127.0.0.1:8080/` | `http://192.168.42.92:8080/` |

Secrets: the WiFi key is templated (`deploy/wifi.nmconnection.tmpl`) and injected at
prepare time; the camera URL lives in `/opt/eyes/detector.env` (mode 600) and is read
from the environment, so it never appears in `ps`.

Full reproducible build/deploy record, timings and the failure modes found on the way:
[journal.md](journal.md).

### debug stream

The detector publishes an annotated view of what it sees; this is the tool for
tuning detection and calibration.

```ini
webRTC    http://<left-eye>:8889/debug        browser viewer
rtsp      rtsp://<left-eye>:8554/debug        mpv / ffprobe / VLC (see note)
```

Overlay per frame, fixed-width fields:

```
<local time>  t=<detector uptime>s
cam <fps>f  <frame age>ms r<restarts> c<clients> m<motion>
```

plus the motion bounding box (green), centroid (red cross), iso-lines of the
camera-to-eye mapping (`eye_x` blue, `eye_y` yellow at -1/0/+1) and an eye-space
panel in the upper-right with the current gaze point.

The stream survives a camera outage: after 500 ms without frames the detector
publishes status frames (black + overlay) and keeps reading the camera in the
background, so viewers never stall and no manual restart is needed. A raw frame
is also available at `http://<left-eye>:8080/frame.jpg`.

VLC on Debian bookworm has no RTSP client (only `satip`) and mediamtx's HLS
cookie check blocks its HLS path; use mpv or the browser. The MPEG-TS relay that
existed for VLC has been removed.

### calibration

`http://<left-eye>:8080/calib` serves the latest camera frame and an eye-space
pad. Click a camera point, click the gaze point it should produce, add the
sample; repeat at 5-9 spread-out points, then `fit & apply`. The solver fits the
5-coefficient per-axis polynomial by least squares and applies it live, so the
iso-lines move immediately. Fits are stored in `/opt/eyes/calibration.json`
(override with `EYES_CALIB_FILE`) and reloaded at startup; `reset` deletes the
file and restores the built-in polynomial.

### recording

Recordings run on the control PC, not on the boards, and survive the recording
client being killed by running as a detached user unit:

```bash
systemd-run --user --unit=eyes-rec --collect --   ffmpeg -rtsp_transport tcp -i rtsp://<left-eye>:8554/debug          -t 1800 -c copy -f segment -segment_time 300 -segment_format mpegts \
         -reset_timestamps 1 <out>/debug-%03d.ts
```

Segments are plain MPEG-TS and individually playable; `cat debug-00*.ts > full.ts`
also works.

## original project (removed from the tree)

https://github.com/bshm/halloweeneyes — c++/qt4/opencv, ~2016.
The legacy C++ sources are no longer present in this tree; they remain reachable in
the repository history.
