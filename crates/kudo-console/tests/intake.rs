#![allow(clippy::unwrap_used)]

use kudo_console::intake::{apply_implementation, implementation_request};
use kudo_console::model::IntakeNoteView;

#[test]
fn start_implementation_builds_a_linked_serial_context() {
    let note = IntakeNoteView {
        note_id: "note-1".into(),
        kino_id: "abc123".into(),
        title: "Improve workers".into(),
        created_at: "2026-11-23T12:00:00Z".into(),
        received_at: "2026-11-23T12:01:00Z".into(),
        unreviewed: true,
        body: "Make implementation automatic".into(),
        author: "YJ Park".into(),
        machine: "work".into(),
        session: "session-1".into(),
    };
    let request = implementation_request(&note, "yolo");
    assert_eq!(request.node, "yolo");
    assert!(request.title.contains("Improve workers"));
    assert!(request.head.contains("abc123"));
    assert!(request.head.contains("Make implementation automatic"));
    assert!(request.ask.contains("small"));
    assert!(request.ask.contains("serial"));
}

#[test]
fn apply_implementation_is_idempotent_and_wakes_the_serial_queue() {
    let dir = std::env::temp_dir().join(format!("kudo-console-intake-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = kudo_context::ContextStore::init(&dir).unwrap();
    let note = IntakeNoteView {
        note_id: "note-1".into(),
        kino_id: "abc123".into(),
        title: "Improve workers".into(),
        created_at: "2026-11-23T12:00:00Z".into(),
        received_at: "2026-11-23T12:01:00Z".into(),
        unreviewed: true,
        body: "Make implementation automatic".into(),
        author: "YJ Park".into(),
        machine: "work".into(),
        session: "session-1".into(),
    };
    let first = apply_implementation(&store, &note, "yolo", 100).unwrap();
    let second = apply_implementation(&store, &note, "yolo", 101).unwrap();
    assert_eq!(first, second);
    let context = kudo_context::ContextId(first);
    assert_eq!(store.wake_node(&context).unwrap().as_deref(), Some("yolo"));
    assert!(store.head(&context).unwrap().body.contains("abc123"));
    assert_eq!(store.ready_count(&context, 102).unwrap(), 1);
}
