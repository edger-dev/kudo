//! The shell: places panes by the layout value and renders each by its kind.
//! No pane fetches anything; each draws from the data it is handed.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@115b689ab32e5a9a4ace6a21d50e463c4927e1f41c42513e2b0de4a21166cd97

use dioxus::prelude::*;

use crate::layout::{Direction, Layout, PaneKind};
use crate::model::ConsoleData;
use crate::selection::Selection;

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
                for node in data.nodes.iter() {
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
                                    let _ = crate::api::start_worker(context, node).await;
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

/// A worker's pane, taken over: its screen, and keys to type into it.
#[component]
fn TerminalPane(data: ConsoleData) -> Element {
    let Some(terminal) = data.terminal.clone() else {
        return rsx! { p { "take over a worker to see its pane" } };
    };
    let mut typed = use_signal(String::new);
    let pane = terminal.pane.clone();
    let send = move |bytes: Vec<u8>| {
        let pane = pane.clone();
        async move {
            let _ = crate::api::type_into(pane.node, pane.job, bytes).await;
        }
    };
    let (enter, ctrl_c, escape) = (send.clone(), send.clone(), send.clone());
    rsx! {
        h3 { "pane {terminal.pane.job} on {terminal.pane.node}" }
        pre { style: "background:#111;color:#ddd;padding:6px;white-space:pre;", "{terminal.screen}" }
        form {
            onsubmit: move |e: FormEvent| {
                e.prevent_default();
                let mut line = typed().into_bytes();
                line.push(b'\r');
                typed.set(String::new());
                send(line)
            },
            input {
                style: "width:60%;",
                placeholder: "type into the pane (Enter sends)",
                value: "{typed}",
                oninput: move |e| typed.set(e.value()),
            }
            button { r#type: "submit", "send" }
        }
        button { onclick: move |_| enter(vec![b'\r']), "Enter" }
        button { onclick: move |_| ctrl_c(vec![0x03]), "Ctrl-C" }
        button { onclick: move |_| escape(vec![0x1b]), "Esc" }
    }
}
