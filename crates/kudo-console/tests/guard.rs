//! The console server refuses what a browser on another site could send: a
//! request that is not JSON, or that comes from another origin.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

#![cfg(feature = "server")]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use dioxus::fullstack::{HeaderMap, HeaderValue};
use kudo_console::guard::allowed;

fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.insert(*k, HeaderValue::from_static(v));
    }
    h
}

#[test]
fn same_origin_json_is_allowed() {
    assert!(
        allowed(&headers(&[
            ("content-type", "application/json"),
            ("host", "127.0.0.1:8099"),
            ("origin", "http://127.0.0.1:8099"),
        ]))
        .is_ok()
    );
    // A tool on this machine sends no Origin at all.
    assert!(
        allowed(&headers(&[
            ("content-type", "application/json"),
            ("host", "127.0.0.1:8099")
        ]))
        .is_ok()
    );
}

#[test]
fn what_another_site_could_send_is_refused() {
    // A form or no-cors fetch can only send these without a preflight.
    for ct in [
        "text/plain",
        "application/x-www-form-urlencoded",
        "multipart/form-data",
    ] {
        assert!(
            allowed(&headers(&[
                ("content-type", ct),
                ("host", "127.0.0.1:8099")
            ]))
            .is_err(),
            "{ct}"
        );
    }
    assert!(
        allowed(&headers(&[("host", "127.0.0.1:8099")])).is_err(),
        "no content type"
    );
    assert!(
        allowed(&headers(&[
            ("content-type", "application/json"),
            ("host", "127.0.0.1:8099"),
            ("origin", "https://evil.example"),
        ]))
        .is_err()
    );
    assert!(
        allowed(&headers(&[
            ("content-type", "application/json"),
            ("host", "127.0.0.1:8099"),
            ("sec-fetch-site", "cross-site"),
        ]))
        .is_err()
    );
}
