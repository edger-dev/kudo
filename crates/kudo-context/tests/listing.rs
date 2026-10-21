//! Finding a context again: every context in the store, newest first, and a
//! context named by a short prefix of its id.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_context::{ContextId, ContextStore, Error};

const T0: i64 = 1_792_500_000;

fn store(name: &str) -> (std::path::PathBuf, ContextStore) {
    let dir = std::env::temp_dir().join(format!("kudo-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    (dir, store)
}

#[test]
fn contexts_lists_every_context_newest_first_with_its_title() {
    let (dir, store) = store("list");
    assert!(store.contexts().unwrap().is_empty());

    let old = store.create_context("the old one", "h\n", T0).unwrap();
    let new = store.create_context("the new one", "h\n", T0 + 60).unwrap();
    store
        .enqueue(&new, "an item is not a context", T0 + 61)
        .unwrap();

    let all = store.contexts().unwrap();
    let ids: Vec<&ContextId> = all.iter().map(|c| &c.id).collect();
    assert_eq!(ids, [&new, &old]);
    assert_eq!(all[0].title, "the new one");
    assert_eq!(all[1].title, "the old one");
    assert!(all[0].created > all[1].created, "{all:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_unique_prefix_names_a_context() {
    let (dir, store) = store("prefix");
    let ctx = store.create_context("only", "h\n", T0).unwrap();

    assert_eq!(store.resolve_context(&ctx.0).unwrap(), ctx);
    assert_eq!(store.resolve_context(&ctx.0[..6]).unwrap(), ctx);
    assert!(
        matches!(store.resolve_context(""), Err(Error::UnknownContext(_))),
        "an empty prefix names nothing, even in a store of one"
    );
    assert!(matches!(
        store.resolve_context("not-hex"),
        Err(Error::UnknownContext(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_prefix_two_contexts_share_is_refused() {
    let (dir, store) = store("ambiguous");
    // Seventeen ids over sixteen hex digits: two share a first character.
    let ids: Vec<ContextId> = (0..17)
        .map(|n| store.create_context(&format!("c{n}"), "h\n", T0).unwrap())
        .collect();
    let shared = ids
        .iter()
        .map(|c| &c.0[..1])
        .find(|p| ids.iter().filter(|c| c.0.starts_with(*p)).count() > 1)
        .unwrap();

    match store.resolve_context(shared) {
        Err(Error::AmbiguousContext(prefix, n)) => {
            assert_eq!(prefix, shared);
            assert!(n >= 2);
        }
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
