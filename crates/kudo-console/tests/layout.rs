//! The arrangement is a value: the same shell, the same panes and data, two
//! layout values, two arrangements — no component changes.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use dioxus::prelude::*;
use kudo_console::layout::{Direction, Layout, PaneKind, STACKED, THREE_PANES};
use kudo_console::model::{ConsoleData, ContextCard, ContextDetail, NodeView, TraceRow, WorkerView};
use kudo_console::shell::Shell;

fn sample() -> ConsoleData {
    ConsoleData {
        nodes: vec![NodeView {
            id: "yolo".into(),
            links: vec!["stable".into()],
        }],
        contexts: vec![ContextCard {
            id: "c".repeat(64),
            title: "first kudo".into(),
            ready: 1,
            submitted: 2,
            workers: vec![WorkerView {
                lease: "lease-1".into(),
                last: "pull 1234".into(),
            }],
        }],
        selected: Some(ContextDetail {
            id: "c".repeat(64),
            title: "first kudo".into(),
            head: "# first kudo\nstate: active\n".into(),
            forked: false,
            trace: vec![TraceRow {
                at: "2026-10-21T00:00:00Z".into(),
                lease: "lease-1".into(),
                seq: 1,
                line: "lease-start".into(),
            }],
        }),
    }
}

fn render(layout: Layout) -> String {
    let data = sample();
    dioxus_ssr::render_element(rsx! { Shell { layout: layout, data: data } })
}

/// The order the panes appear in the rendered page.
fn pane_order(html: &str) -> Vec<&'static str> {
    let mut found: Vec<(usize, &'static str)> = ["where", "head", "trace"]
        .into_iter()
        .filter_map(|p| html.find(&format!("data-pane=\"{p}\"")).map(|i| (i, p)))
        .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

#[test]
fn three_panes_put_where_head_and_trace_side_by_side() {
    let html = render(THREE_PANES);
    assert_eq!(pane_order(&html), ["where", "head", "trace"], "{html}");
    assert!(html.contains("data-direction=\"row\""), "{html}");
    assert!(html.contains("first kudo") && html.contains("state: active"), "{html}");
    assert!(html.contains("lease-start"), "{html}");
}

#[test]
fn a_second_arrangement_is_only_a_different_value() {
    assert_eq!(STACKED.direction, Direction::Column);
    let kinds = |l: &Layout| {
        let mut k: Vec<PaneKind> = l.panes.iter().map(|p| p.kind).collect();
        k.sort();
        k
    };
    assert_eq!(kinds(&STACKED), kinds(&THREE_PANES), "the same panes");
    let html = render(STACKED);
    assert!(html.contains("data-direction=\"column\""), "{html}");
    assert_ne!(pane_order(&html), pane_order(&render(THREE_PANES)), "a different arrangement");
}
