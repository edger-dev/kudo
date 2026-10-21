//! The page: fetches through the server functions, holds the selection, and
//! hands the shell the layout value and the data.

use dioxus::prelude::*;

use crate::layout::{Layout, THREE_PANES};
use crate::selection::Selection;
use crate::shell::Shell;

/// The arrangement this console shows: change this value, not the components.
pub const LAYOUT: Layout = THREE_PANES;

/// Wait a while before trying the server again (the page only).
async fn pause(millis: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(millis).await;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = millis;
}

#[component]
pub fn App() -> Element {
    let selection = use_context_provider(|| Signal::new(Selection::default()));
    // Bumped on every long-poll answer: something moved, or a while passed.
    let mut tick = use_signal(|| 0u64);
    use_future(move || async move {
        let mut seen = 0u64;
        let mut backoff = 500u32;
        loop {
            match crate::api::wait_version(seen).await {
                Ok(v) => {
                    seen = v;
                    backoff = 500;
                    tick += 1;
                }
                // The server is down, or was rebuilt under this page: back
                // off rather than spin.
                Err(_) => {
                    pause(backoff).await;
                    backoff = (backoff * 2).min(30_000);
                }
            }
        }
    });
    // A pane being taken over has no events: its screen is re-read every
    // second while it is shown (in the page only).
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        loop {
            pause(1000).await;
            if selection.read().pane.is_some() {
                tick += 1;
            }
        }
    });
    let data = use_server_future(move || {
        let _ = tick();
        let (selected, pane) = {
            let s = selection.read();
            (s.context.clone(), s.pane.clone())
        };
        async move { crate::api::console_data(selected, pane).await }
    })?;
    let mut ask = use_signal(String::new);
    let read = data.read();
    let current = read
        .as_ref()
        .and_then(|d| d.as_ref().ok())
        .and_then(|d| d.selected.as_ref().map(|s| s.id.clone()));
    let error = read
        .as_ref()
        .and_then(|d| d.as_ref().ok())
        .and_then(|d| d.error.clone());
    rsx! {
        div { style: "font-family:monospace;padding:4px;",
            if let Some(context) = current.clone() {
                form {
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        let context = context.clone();
                        let text = ask();
                        async move {
                            if !text.trim().is_empty() && crate::api::enqueue(context, text).await.is_ok() {
                                ask.set(String::new());
                            }
                        }
                    },
                    input {
                        style: "width:60%;",
                        placeholder: "enqueue an ask under this context",
                        value: "{ask}",
                        oninput: move |e| ask.set(e.value()),
                    }
                    button { r#type: "submit", "enqueue" }
                }
            }
            if let Some(error) = error {
                p { style: "color:#a00;", "{error}" }
            }
        }
        match &*read {
            Some(Ok(d)) => rsx! { Shell { layout: LAYOUT, data: d.clone() } },
            Some(Err(e)) => rsx! { p { "{e}" } },
            None => rsx! { p { "loading" } },
        }
    }
}
