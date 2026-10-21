//! What the console server accepts: same-origin JSON only.

use dioxus::fullstack::HeaderMap;

/// Whether a request may reach a server function (stub).
pub fn allowed(headers: &HeaderMap) -> Result<(), String> {
    let _ = headers;
    Ok(())
}
