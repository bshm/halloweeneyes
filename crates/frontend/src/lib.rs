//! Halloween Eyes — browser frontend.
//!
//! Compiled to WASM via `trunk`.  Connects to the detector's WebSocket
//! endpoint, receives [`shared::ServerMessage::EyeUpdate`] JSON frames, and
//! renders the animated eye using wgpu (WebGL2 backend in browser,
//! native Vulkan/Metal/DX12 when embedded in a future Tauri shell).
//!
//! # Transport abstraction
//!
//! [`EyeStateSource`] is a trait that abstracts where eye-state updates come
//! from.  The [`WebSocketSource`] implementation is used in the browser build.
//! A native channel-based implementation can be plugged in for Tauri without
//! touching the rendering code.

mod render;
mod transport;

use leptos::prelude::*;
use shared::EyeState;
use transport::{EyeStateSource, WebSocketSource};
use wasm_bindgen::JsCast;

pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    let (eye_state, set_eye_state) = signal(EyeState::default_open());

    // Derive WebSocket URL from current page origin
    let ws_url = {
        let location = web_sys::window()
            .expect("no window")
            .location();
        let host = location.host().expect("no host");
        format!("ws://{host}/ws")
    };

    let _source = WebSocketSource::connect(ws_url, set_eye_state);

    view! {
        <EyeCanvas eye_state=eye_state />
    }
}

#[component]
fn EyeCanvas(eye_state: ReadSignal<EyeState>) -> impl IntoView {
    use leptos::html::Canvas;

    let canvas_ref = NodeRef::<Canvas>::new();

    // Initialise the wgpu renderer once the canvas is mounted
    Effect::new(move |_| {
        if let Some(canvas) = canvas_ref.get() {
            let html_canvas = canvas.unchecked_into::<web_sys::HtmlCanvasElement>();
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = render::init_renderer(html_canvas).await {
                    web_sys::console::error_1(&format!("renderer init failed: {e}").into());
                }
            });
        }
    });

    // Re-render whenever eye state changes
    Effect::new(move |_| {
        let state = eye_state.get();
        render::update_eye_state(state);
    });

    view! {
        <canvas
            node_ref=canvas_ref
            id="eye-canvas"
            width="800"
            height="600"
            style="background:#000;display:block;margin:auto;"
        />
    }
}
