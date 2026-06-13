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

/// WebSocket-based source.  Connects on construction, reconnects are not
/// implemented yet (future work).
pub struct WebSocketSource {
    // Keep closures alive for the lifetime of the connection
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    _socket: WebSocket,
}

impl EyeStateSource for WebSocketSource {
    fn connect(url: String, sink: WriteSignal<EyeState>) -> Self {
        log!("connecting to {}", url);
        let socket = WebSocket::new(&url).expect("WebSocket::new failed");

        let on_open = Closure::wrap(Box::new(|_: web_sys::Event| {
            log!("websocket open");
        }) as Box<dyn FnMut(web_sys::Event)>);
        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget(); // ok to leak — fires once

        let msg_count = std::rc::Rc::new(std::cell::Cell::new(0u64));
        let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(txt) = e.data().as_string() {
                let n = msg_count.get();
                msg_count.set(n + 1);
                if n % 60 == 0 {
                    log!("ws msg #{}: {}", n, txt);
                }
                match serde_json::from_str::<ServerMessage>(&txt) {
                    Ok(ServerMessage::EyeUpdate(state)) => sink.set(state),
                    Err(e) => err!("json parse error: {} — raw: {}", e, txt),
                }
            } else {
                err!("non-string message received");
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        let on_error = Closure::wrap(Box::new(|e: ErrorEvent| {
            err!("ws error: {:?}", e.message());
        }) as Box<dyn FnMut(ErrorEvent)>);

        let on_close = Closure::wrap(Box::new(|c: CloseEvent| {
            log!("ws closed — code={} reason='{}'", c.code(), c.reason());
        }) as Box<dyn FnMut(CloseEvent)>);

        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        Self {
            _on_message: on_message,
            _on_error: on_error,
            _on_close: on_close,
            _socket: socket,
        }
    }
}
