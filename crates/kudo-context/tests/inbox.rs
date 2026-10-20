//! The inbox: a rough edge filed in one line from inside the work, with where
//! it came from.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_context::ContextStore;

const T0: i64 = 1_792_500_000;

#[test]
fn an_entry_keeps_its_line_body_and_provenance_and_lists_oldest_first() {
    let dir = std::env::temp_dir().join(format!("kudo-context-inbox-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let ctx = store.create_context("t", "h\n", T0).unwrap();

    store
        .file_inbox(
            &ctx,
            "lease-a",
            "could not resolve kino X",
            Some("tried twice"),
            T0,
        )
        .unwrap();
    store
        .file_inbox(&ctx, "lease-a", "could not resolve kino X", None, T0 + 1)
        .unwrap();

    let entries = store.inbox(&ctx).unwrap();
    assert_eq!(entries.len(), 2, "the same line twice is two entries");
    assert_eq!(entries[0].line, "could not resolve kino X");
    assert_eq!(entries[0].body.as_deref(), Some("tried twice"));
    assert_eq!(entries[0].lease, "lease-a");
    assert_eq!(entries[1].body, None);
    let _ = std::fs::remove_dir_all(&dir);
}
