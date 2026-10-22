//! What is selected: one value that decides what a phone shows, what back
//! does, and what the URL says.
//!
//! implements: 77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90@77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90
//! implements: c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410@c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410

use kudo_console::layout::PaneKind;
use kudo_console::model::PaneRef;
use kudo_console::selection::{Selection, Tab};

fn ctx() -> String {
    "c".repeat(64)
}

/// Nothing selected shows the list; each selection drills one level deeper,
/// and back steps out one level at a time.
#[test]
fn back_steps_out_one_level_at_a_time() {
    let mut s = Selection::default();
    assert_eq!(s.focus(), PaneKind::Where);
    assert_eq!(s.depth(), 0);

    s.select_context(ctx());
    assert_eq!(s.focus(), PaneKind::Head);
    s.tab = Tab::Trace;
    assert_eq!(s.focus(), PaneKind::Trace);
    s.pane = Some(PaneRef {
        node: "yolo".into(),
        job: "j-1".into(),
    });
    assert_eq!(s.focus(), PaneKind::Terminal);
    s.tray = true;
    let deepest = s.depth();

    s.back();
    assert!(!s.tray, "back closes the tray first");
    assert!(s.depth() < deepest);
    s.back();
    assert_eq!(s.pane, None);
    assert_eq!(s.focus(), PaneKind::Trace);
    s.back();
    assert_eq!(s, Selection::default(), "out of the context, to the list");
    s.back();
    assert_eq!(s, Selection::default(), "back at the top stays there");
}

/// A node is the other view: its jobs, then one job's output.
#[test]
fn a_node_drills_to_its_jobs_then_a_job() {
    let mut s = Selection::default();
    s.select_context(ctx());
    s.select_node("edger::yolo".into());
    assert_eq!(s.context, None, "one view at a time");
    assert_eq!(s.focus(), PaneKind::Jobs);
    s.select_job("1792500000000-7-1".into());
    assert_eq!(s.focus(), PaneKind::Job);
    s.back();
    assert_eq!(s.focus(), PaneKind::Jobs);
    s.select_context(ctx());
    assert_eq!(s.node, None);
    assert_eq!(s.job, None);
}

/// What each view shows beside the list: a context's head and trace, or a
/// node's jobs and the selected job; a taken-over pane beside either.
#[test]
fn panes_shown_follow_the_view() {
    let mut s = Selection::default();
    assert!(s.shows(PaneKind::Where));
    assert!(!s.shows(PaneKind::Head));
    s.select_context(ctx());
    assert!(s.shows(PaneKind::Head) && s.shows(PaneKind::Trace));
    assert!(!s.shows(PaneKind::Jobs));
    s.select_node("pc".into());
    assert!(s.shows(PaneKind::Jobs));
    assert!(!s.shows(PaneKind::Head) && !s.shows(PaneKind::Job));
    s.select_job("j".into());
    assert!(s.shows(PaneKind::Job));
    assert!(!s.shows(PaneKind::Terminal));
}

/// Every place round-trips through the fragment, node names with `::`
/// included.
#[test]
fn the_fragment_round_trips() {
    let mut s = Selection::default();
    assert_eq!(s.fragment(), "");
    s.select_context(ctx());
    s.tab = Tab::Trace;
    s.pane = Some(PaneRef {
        node: "edger::yolo".into(),
        job: "1792500000000-7-1".into(),
    });
    s.tray = true;
    let text = s.fragment();
    assert!(text.starts_with('#'), "{text}");
    assert_eq!(Selection::from_fragment(&text), s, "{text}");

    let mut n = Selection::default();
    n.select_node("odd name&=#/%".into());
    n.select_job("1792500000000-7-2".into());
    assert_eq!(Selection::from_fragment(&n.fragment()), n, "{}", n.fragment());
}

/// A fragment that does not parse, or names a job without its node, selects
/// nothing it cannot stand behind.
#[test]
fn a_stale_or_broken_fragment_selects_nothing() {
    assert_eq!(Selection::from_fragment(""), Selection::default());
    assert_eq!(Selection::from_fragment("#"), Selection::default());
    assert_eq!(Selection::from_fragment("#garbage"), Selection::default());
    assert_eq!(Selection::from_fragment("#zz=1&q"), Selection::default());
    assert_eq!(
        Selection::from_fragment("#j=1792500000000-7-1"),
        Selection::default(),
        "a job needs its node"
    );
    assert_eq!(
        Selection::from_fragment("#p=nonode"),
        Selection::default(),
        "a pane is node/job"
    );
    assert_eq!(
        Selection::from_fragment("#c=%zz"),
        Selection::default(),
        "a broken escape"
    );
}

/// Drilling in deepens the selection (the page pushes history); a sibling
/// change does not (the page replaces the entry).
#[test]
fn depth_tells_a_push_from_a_replace() {
    let mut a = Selection::default();
    a.select_context(ctx());
    let mut b = a.clone();
    b.tab = Tab::Trace;
    assert_eq!(a.depth(), b.depth(), "head ↔ trace is a sibling change");
    let mut c = b.clone();
    c.pane = Some(PaneRef {
        node: "yolo".into(),
        job: "j".into(),
    });
    assert!(c.depth() > b.depth());
}
