//! Two workers, one context: a submit from a moved head is refused with the
//! head as it is now, and goes through once the worker builds on it.
//!
//! implements: 3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab@aaddfc2e7ee067ea5780ce6ee2dfe4587bfb47b0c067b2670a31308c019a7e21

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_context::{ContextId, ContextStore, Error, Submission};

const T0: i64 = 1_792_500_000;
const TTL: i64 = 600;

fn store_with_context(name: &str) -> (std::path::PathBuf, ContextStore, ContextId) {
    let dir = std::env::temp_dir().join(format!("kudo-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let ctx = store
        .create_context("two", "# two\nstate: new\n", T0)
        .unwrap();
    (dir, store, ctx)
}

fn sub(head: &str, base: Option<Vec<String>>) -> Submission {
    Submission {
        head: head.into(),
        log: "log".into(),
        base,
        ..Default::default()
    }
}

#[test]
fn a_claim_records_the_head_it_started_from() {
    let (dir, store, ctx) = store_with_context("base");
    store.enqueue(&ctx, "one", T0).unwrap();
    let claim = store.pull(&ctx, "lease-a", T0, TTL).unwrap().unwrap();
    assert_eq!(claim.base, vec![store.head(&ctx).unwrap().version]);
    let held = store.held_by(&ctx, "lease-a", T0).unwrap().unwrap();
    assert_eq!(held.base, claim.base);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_second_submit_from_one_head_is_refused_then_lands_on_the_new_head() {
    let (dir, store, ctx) = store_with_context("moved");
    store.enqueue(&ctx, "one", T0).unwrap();
    store.enqueue(&ctx, "two", T0).unwrap();
    let a = store.pull(&ctx, "lease-a", T0, TTL).unwrap().unwrap();
    let b = store.pull(&ctx, "lease-b", T0, TTL).unwrap().unwrap();
    assert_eq!(a.base, b.base);

    store
        .submit(&a.id, &sub("# two\nA was here\n", None), T0 + 1)
        .unwrap();
    let after_a = store.head(&ctx).unwrap();

    match store.submit(&b.id, &sub("# two\nB was here\n", None), T0 + 2) {
        Err(Error::HeadMoved { claim, current }) => {
            assert_eq!(claim, b.id.0);
            assert_eq!(current, vec![after_a.version.clone()]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        store.head(&ctx).unwrap(),
        after_a,
        "the refused submit wrote nothing"
    );
    assert!(
        store.held_by(&ctx, "lease-b", T0 + 2).unwrap().is_some(),
        "the claim stays live"
    );

    store
        .submit(
            &b.id,
            &sub(
                "# two\nA was here\nB was here\n",
                Some(vec![after_a.version]),
            ),
            T0 + 3,
        )
        .unwrap();
    let history = store.head_history(&ctx).unwrap();
    assert_eq!(history.len(), 3, "{history:?}");
    assert_eq!(history[2].body, "# two\nA was here\nB was here\n");
    assert_eq!(store.log(&ctx).unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_base_that_is_not_the_head_is_refused() {
    let (dir, store, ctx) = store_with_context("stale-base");
    store.enqueue(&ctx, "one", T0).unwrap();
    let claim = store.pull(&ctx, "lease-a", T0, TTL).unwrap().unwrap();
    let refused = store.submit(
        &claim.id,
        &sub("# two\nx\n", Some(vec!["0".repeat(64)])),
        T0 + 1,
    );
    assert!(
        matches!(refused, Err(Error::HeadMoved { .. })),
        "{refused:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn heads_lists_the_current_head_alone_when_there_is_no_fork() {
    let (dir, store, ctx) = store_with_context("heads");
    let heads = store.heads(&ctx).unwrap();
    assert_eq!(heads, vec![store.head(&ctx).unwrap()]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A version's hash is its content's: a head that goes back to an earlier
/// version's bytes must not make the history a loop.
#[test]
fn a_head_that_returns_to_earlier_bytes_is_still_the_head() {
    let (dir, store, ctx) = store_with_context("undo");
    let first = store.head(&ctx).unwrap().body;
    for (n, body) in ["# two\nchanged\n", first.as_str()].into_iter().enumerate() {
        store.enqueue(&ctx, &format!("item {n}"), T0).unwrap();
        let claim = store.pull(&ctx, "lease-a", T0, TTL).unwrap().unwrap();
        store
            .submit(&claim.id, &sub(body, None), T0 + 1 + n as i64)
            .unwrap();
    }
    assert_eq!(store.head(&ctx).unwrap().body, first);
    assert_eq!(store.heads(&ctx).unwrap().len(), 1);
    assert_eq!(store.head_history(&ctx).unwrap().len(), 3);

    store.enqueue(&ctx, "after the undo", T0).unwrap();
    let claim = store.pull(&ctx, "lease-a", T0 + 5, TTL).unwrap().unwrap();
    store
        .submit(&claim.id, &sub("# two\nonward\n", None), T0 + 6)
        .unwrap();
    assert_eq!(store.head(&ctx).unwrap().body, "# two\nonward\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_head_version_reads_by_its_hash() {
    let (dir, store, ctx) = store_with_context("by-hash");
    let head = store.head(&ctx).unwrap();
    assert_eq!(store.head_version(&head.version).unwrap(), head);
    let _ = std::fs::remove_dir_all(&dir);
}
