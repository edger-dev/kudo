//! The shell: places panes by the layout value and renders each by its kind.
//! No pane fetches anything; each draws from the data it is handed.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@115b689ab32e5a9a4ace6a21d50e463c4927e1f41c42513e2b0de4a21166cd97

use dioxus::prelude::*;

use crate::layout::{Direction, Layout, PaneKind};
use crate::model::ConsoleData;
use crate::selection::{Notice, Selection};

/// The panes of `layout`, drawn from `data`.
#[component]
pub fn Shell(layout: Layout, data: ConsoleData) -> Element {
    let direction = match layout.direction {
        Direction::Row => "row",
        Direction::Column => "column",
    };
    rsx! {
        div {
            class: "kudo-shell",
            "data-direction": direction,
            style: "display:flex;flex-direction:{direction};height:100vh;font-family:monospace;",
            for pane in layout.panes.iter() {
                div {
                    key: "{pane.kind:?}",
                    "data-pane": pane_name(pane.kind),
                    style: "flex:{pane.grow};overflow:auto;padding:8px;border:1px solid #ccc;",
                    {render(pane.kind, &data)}
                }
            }
        }
    }
}

fn pane_name(kind: PaneKind) -> &'static str {
    match kind {
        PaneKind::Where => "where",
        PaneKind::Head => "head",
        PaneKind::Trace => "trace",
        PaneKind::Terminal => "terminal",
    }
}

/// The renderer for a pane's kind.
fn render(kind: PaneKind, data: &ConsoleData) -> Element {
    match kind {
        PaneKind::Where => rsx! { WherePane { data: data.clone() } },
        PaneKind::Head => rsx! { HeadPane { data: data.clone() } },
        PaneKind::Trace => rsx! { TracePane { data: data.clone() } },
        PaneKind::Terminal => rsx! { TerminalPane { data: data.clone() } },
    }
}

fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

#[component]
fn WherePane(data: ConsoleData) -> Element {
    let selection = try_consume_context::<Signal<Selection>>();
    rsx! {
        h3 { "nodes" }
        for node in data.nodes.iter() {
            div { key: "{node.id}", "{node.id} ({node.links.join(\", \")})" }
        }
        h3 { "contexts" }
        for card in data.contexts.iter() {
            div {
                key: "{card.id}",
                style: "cursor:pointer;margin-bottom:6px;",
                onclick: {
                    let id = card.id.clone();
                    move |_| {
                        if let Some(mut selection) = selection {
                            selection.write().context = Some(id.clone());
                        }
                    }
                },
                div { b { "{card.title}" } " {short(&card.id)}" }
                div { "{card.ready} ready, {card.submitted} submitted" }
                for worker in card.workers.iter() {
                    div { key: "{worker.lease}", style: "padding-left:1em;",
                        "worker {short(&worker.lease)}: {worker.last}"
                        if worker.quiet {
                            " (quiet — likely gone)"
                        }
                        if let Some(pane) = worker.pane.clone() {
                            button {
                                style: "margin-left:6px;",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    if let Some(mut selection) = selection {
                                        selection.write().pane = Some(pane.clone());
                                    }
                                },
                                "take over"
                            }
                        }
                    }
                }
                for node in data.nodes.iter().filter(|n| n.may_start) {
                    button {
                        key: "start-{node.id}",
                        style: "margin:2px;",
                        onclick: {
                            let context = card.id.clone();
                            let node = node.id.clone();
                            move |e: MouseEvent| {
                                e.stop_propagation();
                                let (context, node) = (context.clone(), node.clone());
                                async move {
                                    match crate::api::start_worker(context, node).await {
                                        Ok(pane) => {
                                            if let Some(mut selection) = selection {
                                                selection.write().pane = Some(pane);
                                            }
                                        }
                                        Err(e) => say(e.to_string()),
                                    }
                                }
                            }
                        },
                        "start a worker on {node.id}"
                    }
                }
            }
        }
    }
}

#[component]
fn HeadPane(data: ConsoleData) -> Element {
    match &data.selected {
        None => rsx! { p { "select a context" } },
        Some(detail) => rsx! {
            h3 { "{detail.title}" }
            if detail.forked {
                p { "the head is forked: the next submit folds the tips together" }
            }
            pre { "{detail.head}" }
        },
    }
}

#[component]
fn TracePane(data: ConsoleData) -> Element {
    match &data.selected {
        None => rsx! { p { "no trace" } },
        Some(detail) => rsx! {
            h3 { "trace" }
            for row in detail.trace.iter() {
                div { key: "{row.lease}-{row.seq}",
                    "{row.at}  {short(&row.lease)} #{row.seq}  {row.line}"
                }
            }
        },
    }
}

/// Say what failed, above the panes (when the page provides the notice).
fn say(text: String) {
    if let Some(mut notice) = try_consume_context::<Signal<Notice>>() {
        notice.set(Notice(Some(text)));
    }
}

/// Type `bytes` into a pane, in pieces a node takes whole; whether it all went.
async fn type_all(pane: crate::model::PaneRef, bytes: Vec<u8>) -> bool {
    for piece in bytes.chunks(4096) {
        if let Err(e) =
            crate::api::type_into(pane.node.clone(), pane.job.clone(), piece.to_vec()).await
        {
            say(format!("not typed: {e}"));
            return false;
        }
    }
    true
}

/// The keys a taken-over pane offers, as the bytes a terminal sends: what a
/// CLI's menus and prompts need beyond a typed line.
pub const KEYS: &[(&str, &[u8])] = &[
    ("↑", b"\x1b[A"),
    ("↓", b"\x1b[B"),
    ("←", b"\x1b[D"),
    ("→", b"\x1b[C"),
    ("Tab", b"\t"),
    ("Enter", b"\r"),
    ("Esc", b"\x1b"),
    ("Ctrl-C", b"\x03"),
];

/// The bytes for a key pressed in the empty line field: the arrows and
/// Escape go straight to the pane (a menu answers them); anything else types
/// into the field.
fn key_bytes(key: &Key) -> Option<&'static [u8]> {
    let label = match key {
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::Escape => "Esc",
        _ => return None,
    };
    KEYS.iter().find(|(l, _)| *l == label).map(|(_, b)| *b)
}

/// One send into a pane, queued: the bytes, and the typed line they carry
/// (put back in the field if refused).
struct Typed {
    pane: crate::model::PaneRef,
    bytes: Vec<u8>,
    line: Option<String>,
}

/// A worker's pane, taken over: its screen, and keys to type into it.
#[component]
fn TerminalPane(data: ConsoleData) -> Element {
    let selection = try_consume_context::<Signal<Selection>>();
    let Some(terminal) = data.terminal.clone() else {
        return rsx! { p { "take over a worker to see its pane" } };
    };
    let mut typed = use_signal(String::new);
    // One queue for everything typed into the pane: each send waits for the
    // one before, so quick keys (↓ ↓ Enter) reach the pane in order.
    let sends = use_coroutine(move |mut rx: UnboundedReceiver<Typed>| async move {
        use futures_util::StreamExt;
        while let Some(send) = rx.next().await {
            let went = type_all(send.pane, send.bytes).await;
            // A refused line is not lost: it goes back in the field.
            if let (false, Some(line)) = (went, send.line) {
                typed.set(line);
            }
        }
    });
    let pane = terminal.pane.clone();
    let key = move |bytes: &[u8]| {
        sends.send(Typed {
            pane: pane.clone(),
            bytes: bytes.to_vec(),
            line: None,
        });
    };
    let direct = key.clone();
    let line_pane = terminal.pane.clone();
    rsx! {
        h3 {
            "pane {terminal.pane.job} on {terminal.pane.node} "
            button {
                onclick: move |_| {
                    if let Some(mut selection) = selection {
                        selection.write().pane = None;
                    }
                },
                "close"
            }
        }
        pre { style: "background:#111;color:#ddd;padding:6px;white-space:pre;", "{terminal.screen}" }
        form {
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                let line = typed();
                let mut bytes = line.clone().into_bytes();
                bytes.push(b'\r');
                typed.set(String::new());
                sends.send(Typed {
                    pane: line_pane.clone(),
                    bytes,
                    line: Some(line),
                });
            },
            input {
                style: "width:60%;",
                placeholder: "type into the pane (Enter sends)",
                value: "{typed}",
                oninput: move |e| typed.set(e.value()),
                onkeydown: move |e: KeyboardEvent| {
                    let bytes = typed().is_empty().then(|| key_bytes(&e.key())).flatten();
                    if let Some(bytes) = bytes {
                        e.prevent_default();
                        direct(bytes);
                    }
                },
            }
            button { r#type: "submit", "send" }
        }
        div {
            for (label, bytes) in KEYS.iter().copied() {
                button {
                    key: "{label}",
                    onclick: {
                        let key = key.clone();
                        move |_| key(bytes)
                    },
                    "{label}"
                }
            }
        }
    }
}
