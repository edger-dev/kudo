//! The page: fetches through the server functions, holds the selection, and
//! hands the shell the layout value and the data.

use dioxus::prelude::*;

use crate::layout::{Layout, THREE_PANES};
use crate::selection::Selection;
use crate::shell::Shell;

/// The arrangement this console shows: change this value, not the components.
pub const LAYOUT: Layout = THREE_PANES;

#[component]
pub fn App() -> Element {
    let selection = use_context_provider(|| Signal::new(Selection::default()));
    let mut version = use_signal(|| 0u64);
    // Long-poll: the server answers when the hub says something moved.
    use_future(move || async move {
        let mut seen = 0u64;
        loop {
            match crate::api::wait_version(seen).await {
                Ok(v) => {
                    seen = v;
                    version.set(v);
                }
                Err(_) => seen = 0,
            }
        }
    });
    let data = use_resource(move || {
        let _ = version();
        let selected = selection.read().context.clone();
        async move { crate::api::console_data(selected).await }
    });
    let mut ask = use_signal(String::new);
    let current = data
        .read()
        .as_ref()
        .and_then(|d| d.as_ref().ok())
        .and_then(|d| d.selected.as_ref().map(|s| s.id.clone()));
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
        }
        match &*data.read() {
            Some(Ok(d)) => rsx! { Shell { layout: LAYOUT, data: d.clone() } },
            Some(Err(e)) => rsx! { p { "{e}" } },
            None => rsx! { p { "loading" } },
        }
    }
}
