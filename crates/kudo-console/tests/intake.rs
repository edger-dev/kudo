#![allow(clippy::unwrap_used)]

use kudo_console::intake::implementation_request;
use kudo_intake::ReceivedNote;

#[test]
fn start_implementation_builds_a_linked_serial_context() {
    let note = ReceivedNote {
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
