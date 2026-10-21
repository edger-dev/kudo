//! What the console server accepts: same-origin JSON only.
//!
//! The console has no login (v1 runs by hand, bound to localhost). What it
//! must refuse is a request another web page could make the person's browser
//! send: a form post or a no-cors fetch can carry only simple content types,
//! and always says where it came from.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

use dioxus::fullstack::{HeaderMap, Method};

/// Whether a request may reach a server function. A page load (GET or HEAD,
/// in which the page's own render runs them) writes nothing another site
/// could aim; anything else must be JSON, and not from another origin.
pub fn allowed(method: &Method, headers: &HeaderMap) -> Result<(), String> {
    if method == Method::GET || method == Method::HEAD {
        return Ok(());
    }
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if !get("content-type").is_some_and(|ct| ct.starts_with("application/json")) {
        return Err("the console takes JSON only".to_owned());
    }
    if get("sec-fetch-site").is_some_and(|site| site != "same-origin" && site != "none") {
        return Err("the console refuses requests from another site".to_owned());
    }
    if let Some(origin) = get("origin") {
        let host = get("host").unwrap_or_default();
        let same = origin == format!("http://{host}") || origin == format!("https://{host}");
        if !same {
            return Err(format!("the console refuses requests from {origin}"));
        }
    }
    Ok(())
}
