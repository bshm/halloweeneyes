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

## original project

https://github.com/bshm/halloweeneyes — c++/qt4/opencv, ~2016
