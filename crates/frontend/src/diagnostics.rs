//! Shared diagnostic logging for the WASM frontend.
//!
//! `log_remote(msg)` writes to `console.log` AND fires a fire-and-forget
//! POST to the detector's `/log` endpoint so that frontend log lines appear
//! interleaved with backend tracing output in the same terminal session.

use wasm_bindgen::JsValue;

/// Write `msg` to `console.log` and POST it to the detector's `/log` endpoint.
///
/// The HTTP POST is fire-and-forget: the returned Promise is intentionally
/// dropped.  This keeps callers synchronous and avoids async overhead on hot
/// paths (throttled in callers).
pub fn log_remote(msg: &str) {
    // 1. Browser console
    web_sys::console::log_1(&JsValue::from_str(msg));

    // 2. POST to backend /log (fire-and-forget)
    let window = match web_sys::window() {
        Some(w) => w,
        None => return, // not in a browser context
    };

    let mut opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(web_sys::RequestMode::SameOrigin);
    opts.set_body(&wasm_bindgen::JsValue::from_str(msg));

    let request = match web_sys::Request::new_with_str_and_init("/log", &opts) {
        Ok(r) => r,
        Err(_) => return,
    };

    // Set Content-Type header
    let headers = request.headers();
    let _ = headers.set("Content-Type", "text/plain; charset=utf-8");

    // Drop the Promise — browser fires the request regardless
    let _ = window.fetch_with_request(&request);
}

/// Convenience macro — prepends a `[prefix]` tag and calls `log_remote`.
#[macro_export]
macro_rules! diag {
    ($prefix:literal, $($arg:tt)*) => {
        $crate::diagnostics::log_remote(&format!(concat!("[", $prefix, "] {}"), format!($($arg)*)))
    };
}
