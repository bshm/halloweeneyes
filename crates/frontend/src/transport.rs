//! Transport abstraction for eye-state updates.
//!
//! [`EyeStateSource`] is the trait that must be implemented by any transport.
//! The WebSocket implementation connects to the detector server and deserialises
//! incoming JSON into [`shared::EyeState`] signals.

use leptos::prelude::{Set, WriteSignal};
use shared::{EyeState, ServerMessage};
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

macro_rules! log {
    ($($arg:tt)*) => {
        crate::diagnostics::log_remote(&format!("[eye-transport] {}", format!($($arg)*)))
    };
}
macro_rules! err {
    ($($arg:tt)*) => {
        crate::diagnostics::log_remote(&format!("[eye-transport][ERR] {}", format!($($arg)*)))
    };
}

/// Abstraction over the source of eye-state updates.
///
/// Implementations must keep themselves alive (e.g. by holding event listener
/// closures in `JsValue` fields) for as long as updates should arrive.
pub trait EyeStateSource {
    /// Connect / start the source.  Eye-state updates are pushed to `sink`.
    fn connect(url: String, sink: WriteSignal<EyeState>) -> Self
    where
        Self: Sized;
}

/// WebSocket-based source. Reconnects automatically with a 1 s backoff.
pub struct WebSocketSource {
    // First socket is kept alive here; reconnects leak their own handles.
    _socket: Option<WebSocket>,
}

/// Wire handlers onto a fresh socket and keep it alive by leaking its closures.
fn attach(url: String, sink: WriteSignal<EyeState>) -> Option<WebSocket> {
    let socket = match WebSocket::new(&url) {
        Ok(s) => s,
        Err(e) => {
            err!("WebSocket::new failed: {:?}", e);
            schedule_reconnect(url, sink);
            return None;
        }
    };

    log!("connecting to {}", url);

    let on_open = Closure::wrap(Box::new(|_: web_sys::Event| {
        log!("websocket open");
        probe_clock_offset();
    }) as Box<dyn FnMut(web_sys::Event)>);
    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    on_open.forget();

    let msg_count = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Some(txt) = e.data().as_string() {
            let n = msg_count.get();
            msg_count.set(n + 1);
            if n % 60 == 0 {
                log!("ws msg #{}: {}", n, txt);
            }
            match serde_json::from_str::<ServerMessage>(&txt) {
                Ok(ServerMessage::EyeUpdate(state)) => {
                    // Timestamped path: the renderer plays it back on the
                    // shared server timeline.
                    crate::render::push_state(state);
                    sink.set(state);
                }
                Err(e) => err!("json parse error: {} — raw: {}", e, txt),
            }
        } else {
            err!("non-string message received");
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    let on_error = Closure::wrap(Box::new(|e: ErrorEvent| {
        err!("ws error: {:?}", e.message());
    }) as Box<dyn FnMut(ErrorEvent)>);
    socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    on_error.forget();

    let retry_url = url.clone();
    let on_close = Closure::wrap(Box::new(move |c: CloseEvent| {
        log!("ws closed — code={} reason='{}', reconnecting", c.code(), c.reason());
        schedule_reconnect(retry_url.clone(), sink);
    }) as Box<dyn FnMut(CloseEvent)>);
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    on_close.forget();

    Some(socket)
}

fn perf_now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

/// One `GET /time` round trip, returning the detector clock in ms.
async fn fetch_time_ms() -> Option<f64> {
    use wasm_bindgen::JsCast;
    let window = web_sys::window()?;
    let resp_val = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str("/time")).await.ok()?;
    let resp: web_sys::Response = resp_val.dyn_into().ok()?;
    let text_val = wasm_bindgen_futures::JsFuture::from(resp.text().ok()?).await.ok()?;
    let text = text_val.as_string()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("t_ms")?.as_f64()
}

/// Estimate the detector clock offset over the same network path the state
/// messages use; the minimum-RTT sample wins.
fn probe_clock_offset() {
    wasm_bindgen_futures::spawn_local(async move {
        let mut best: Option<(f64, f64)> = None; // (rtt, offset)
        for _ in 0..5 {
            let t0 = perf_now();
            if let Some(t_ms) = fetch_time_ms().await {
                let t1 = perf_now();
                let rtt = t1 - t0;
                let offset = t_ms - (t0 + t1) / 2.0;
                if best.map(|(r, _)| rtt < r).unwrap_or(true) {
                    best = Some((rtt, offset));
                }
            }
        }
        if let Some((rtt, offset)) = best {
            log!("clock offset {:.1} ms (min rtt {:.1} ms)", offset, rtt);
            crate::render::set_clock_offset_ms(offset);
        } else {
            err!("clock offset probe failed");
        }
    });
}

/// Reconnect after a short delay, without needing a page reload.
fn schedule_reconnect(url: String, sink: WriteSignal<EyeState>) {
    let cb = Closure::wrap(Box::new(move || {
        let _ = attach(url.clone(), sink);
    }) as Box<dyn FnMut()>);
    if let Some(w) = web_sys::window() {
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
            cb.as_ref().unchecked_ref(),
            1000,
        );
    }
    cb.forget();
}

impl EyeStateSource for WebSocketSource {
    fn connect(url: String, sink: WriteSignal<EyeState>) -> Self {
        Self {
            _socket: attach(url, sink),
        }
    }
}
