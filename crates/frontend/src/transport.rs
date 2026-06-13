//! Transport abstraction for eye-state updates.
//!
//! [`EyeStateSource`] is the trait that must be implemented by any transport.
//! The WebSocket implementation connects to the detector server and deserialises
//! incoming JSON into [`shared::EyeState`] signals.

use leptos::prelude::{Set, WriteSignal};
use shared::{EyeState, ServerMessage};
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

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
        let socket = WebSocket::new(&url).expect("WebSocket::new failed");

        let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(txt) = e.data().as_string() {
                if let Ok(ServerMessage::EyeUpdate(state)) = serde_json::from_str::<ServerMessage>(&txt) {
                    sink.set(state);
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        let on_error = Closure::wrap(Box::new(|e: ErrorEvent| {
            web_sys::console::error_1(&format!("ws error: {:?}", e.message()).into());
        }) as Box<dyn FnMut(ErrorEvent)>);

        let on_close = Closure::wrap(Box::new(|_: CloseEvent| {
            web_sys::console::log_1(&"ws closed".into());
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
