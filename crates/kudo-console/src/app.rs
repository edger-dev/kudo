//! The page: fetches through the server functions, holds the selection (and
//! keeps it in the URL), the theme (and keeps it in this browser), and hands
//! the shell the layout value and the data.
//!
//! implements: c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410@c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410
//! implements: 2dd989d6ce0f4135c0d8d3ab6e99e065ed03fe3dbf1827d5621d289458cd4351@2dd989d6ce0f4135c0d8d3ab6e99e065ed03fe3dbf1827d5621d289458cd4351

use dioxus::prelude::*;

use crate::history::{History, Write};
use crate::layout::{Layout, THREE_PANES};
use crate::model::JobOutput;
use crate::selection::{Notice, Outcome, Selection};
use crate::shell::Shell;
use crate::theme::Theme;

/// The arrangement this console shows: change this value, not the components.
pub const LAYOUT: Layout = THREE_PANES;

/// The most of a job's output the page keeps.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "the follow is the page's")
)]
const OUTPUT_KEPT: usize = 64 * 1024;

/// Wait a while (the page only; at once elsewhere).
pub(crate) async fn pause(millis: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(millis).await;
    #[cfg(not(target_arch = "wasm32"))]
    let _ = millis;
}

/// Step back through the browser's history, when the entry behind is the
/// level being stepped out to; whether it did.
pub(crate) fn history_back() -> bool {
    let (Some(history), Some(selection)) = (
        try_consume_context::<Signal<History>>(),
        try_consume_context::<Signal<Selection>>(),
    ) else {
        return false;
    };
    browser_back(history, selection)
}

fn browser_back(history: Signal<History>, selection: Signal<Selection>) -> bool {
    if !history.peek().back_is_browser_back(&selection.peek()) {
        return false;
    }
    document::eval("history.back();");
    true
}

/// `text` with at most its last `OUTPUT_KEPT` bytes, cut at a character.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "the follow is the page's")
)]
fn keep_tail(text: &mut String) {
    if text.len() <= OUTPUT_KEPT {
        return;
    }
    let mut cut = text.len() - OUTPUT_KEPT;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    text.drain(..cut);
}

#[component]
pub fn App() -> Element {
    let mut selection = use_context_provider(|| Signal::new(Selection::default()));
    use_context_provider(|| Signal::new(Notice::default()));
    use_context_provider(|| Signal::new(Outcome::default()));
    let theme = use_context_provider(|| Signal::new(Theme::default()));
    let mut history = use_context_provider(|| Signal::new(History::default()));

    // The URL: read on load and on the browser's back and forward; written as
    // the selection changes — deeper pushes an entry, anything else rewrites
    // the current one. Each entry carries its index, so the page knows where
    // in its entries the browser is (the page only).
    use_future(move || async move {
        if cfg!(not(target_arch = "wasm32")) {
            return;
        }
        let mut follow = document::eval(
            r#"
            const here = () => [location.hash, history.state && Number.isInteger(history.state.kudo) ? history.state.kudo : null];
            dioxus.send(here());
            window.addEventListener("popstate", () => dioxus.send(here()));
            await new Promise(() => {});
            "#,
        );
        while let Ok((hash, index)) = follow.recv::<(String, Option<usize>)>().await {
            let next = Selection::from_fragment(&hash);
            if history.write().arrived(index, next.clone()) {
                document::eval("history.replaceState({kudo: 0}, '');");
            }
            if *selection.peek() != next {
                selection.set(next);
            }
        }
    });
    use_effect(move || {
        let now = selection.read().clone();
        let Some(write) = history.write().record(&now) else {
            return;
        };
        let fragment = now.fragment();
        // An empty fragment still has to clear the old one from the address.
        let url = if fragment.is_empty() {
            "location.pathname + location.search".to_owned()
        } else {
            format!("{fragment:?}")
        };
        let (call, index) = match write {
            Write::Push(i) => ("pushState", i),
            Write::Replace(i) => ("replaceState", i),
        };
        document::eval(&format!("history.{call}({{kudo: {index}}}, '', {url});"));
    });
    // Esc closes the tray wherever focus is (the page only).
    use_future(move || async move {
        if cfg!(not(target_arch = "wasm32")) {
            return;
        }
        let mut keys = document::eval(
            r#"
            document.addEventListener("keydown", (e) => { if (e.key === "Escape") dioxus.send(true); });
            await new Promise(() => {});
            "#,
        );
        while keys.recv::<bool>().await.is_ok() {
            if selection.peek().tray && !browser_back(history, selection) {
                selection.write().back();
            }
        }
    });

    // The theme: read from this browser once, kept there as it changes. A
    // refused or missing storage is the system's theme.
    let mut theme_read = use_signal(|| false);
    use_future(move || async move {
        if cfg!(not(target_arch = "wasm32")) {
            return;
        }
        let stored = document::eval(
            r#"try { return localStorage.getItem("kudo-theme") || ""; } catch (e) { return ""; }"#,
        )
        .join::<String>()
        .await
        .unwrap_or_default();
        let mut theme = theme;
        theme.set(Theme::from_stored(&stored));
        theme_read.set(true);
    });
    use_effect(move || {
        let now = theme();
        if !*theme_read.peek() {
            return;
        }
        document::eval(&format!(
            r#"try {{ localStorage.setItem("kudo-theme", "{}"); }} catch (e) {{}}"#,
            now.stored()
        ));
    });

    // The taken-over pane's screen, polled on its own beat: a cheap call, not
    // a re-read of the whole console.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(unused_mut, reason = "the poll is the page's")
    )]
    let mut screen_tick = use_signal(|| 0u64);
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
    // A pane being taken over, and a node's jobs, have no events: re-read
    // every second (the screen) or three (the jobs) while shown (in the page
    // only).
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(unused_mut, reason = "the poll is the page's")
    )]
    let mut jobs_tick = use_signal(|| 0u64);
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        for beat in 0u64.. {
            pause(1000).await;
            if selection.peek().pane.is_some() {
                screen_tick += 1;
            }
            if beat % 3 == 0 && selection.peek().node.is_some() {
                jobs_tick += 1;
            }
        }
    });
    // The selected job's output, followed: each read starts where the last
    // ended, until the job has ended (in the page only).
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(unused_mut, reason = "the follow is the page's")
    )]
    let mut output = use_signal(|| None::<JobOutput>);
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        loop {
            let (node, job) = {
                let s = selection.peek();
                (s.node.clone(), s.job.clone())
            };
            let (Some(node), Some(job)) = (node, job) else {
                if output.peek().is_some() {
                    output.set(None);
                }
                pause(500).await;
                continue;
            };
            let held = output
                .peek()
                .clone()
                .filter(|o| o.node == node && o.job == job);
            if held.as_ref().is_some_and(|o| !o.live) {
                pause(1000).await;
                continue;
            }
            let from = held.as_ref().map_or(0, |o| o.next_offset);
            let read = crate::api::job_output(node.clone(), job.clone(), from).await;
            // A job that cannot be read at all (gone, a stale link) says so,
            // rather than "reading" forever; a live one that fails a read
            // keeps what it had and tries again.
            if let (Err(e), None) = (&read, &held) {
                output.set(Some(JobOutput {
                    node,
                    job,
                    state: format!("could not be read: {e}"),
                    tone: "bad".to_owned(),
                    ..JobOutput::default()
                }));
                pause(1000).await;
                continue;
            }
            if let Ok(next) = read {
                let merged = match held {
                    Some(mut o) => {
                        if next.skipped > 0 {
                            o.text.push_str(&format!(
                                "\n[… {} bytes the node dropped …]\n",
                                next.skipped
                            ));
                        }
                        o.text.push_str(&next.text);
                        keep_tail(&mut o.text);
                        o.next_offset = next.next_offset;
                        o.skipped += next.skipped;
                        o.state = next.state;
                        o.tone = next.tone;
                        o.live = next.live;
                        o
                    }
                    None => {
                        let mut o = next;
                        keep_tail(&mut o.text);
                        o
                    }
                };
                // The selection may have moved while the read was out.
                let s = selection.peek();
                if s.node.as_deref() == Some(merged.node.as_str())
                    && s.job.as_deref() == Some(merged.job.as_str())
                {
                    drop(s);
                    output.set(Some(merged));
                }
            }
            pause(1000).await;
        }
    });

    let data = use_server_future(move || {
        let _ = tick();
        let selected = selection.read().context.clone();
        async move { crate::api::console_data(selected).await }
    })?;
    let terminal = use_resource(move || {
        let _ = screen_tick();
        let pane = selection.read().pane.clone();
        async move {
            match pane {
                Some(pane) => crate::api::pane_screen(pane).await.ok(),
                None => None,
            }
        }
    });
    let jobs = use_resource(move || {
        let _ = jobs_tick();
        let _ = tick();
        let node = selection.read().node.clone();
        async move {
            match node {
                Some(node) => crate::api::node_jobs(node).await.ok(),
                None => None,
            }
        }
    });
    let read = data.read();
    match &*read {
        Some(Ok(d)) => {
            let mut d = d.clone();
            d.terminal = terminal.read().clone().flatten();
            d.jobs = jobs.read().clone().flatten();
            d.output = output.read().clone();
            rsx! { Shell { layout: LAYOUT, data: d } }
        }
        Some(Err(e)) => rsx! { p { "{e}" } },
        None => rsx! { p { "loading" } },
    }
}
