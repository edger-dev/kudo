//! The waker: a context with an item that came due and no worker running gets
//! one, on its wake node; nothing else wakes.
//!
//! implements: 43643bef4da543e855bafcb2308401df3c7b2f1c5a4ef03ac69c06b7a4bd2d2a

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::sync::atomic::{AtomicU32, Ordering};

use kudo_context::{ContextId, ContextStore, TraceEvent};
use kudo_shim::waker::{Wake, due_contexts};

const T0: i64 = 1_792_500_000;
const QUIET: i64 = 3600;

fn store() -> ContextStore {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kudo-shim-waker-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    ContextStore::init(&dir).unwrap()
}

fn due_check(store: &ContextStore, title: &str, due: i64) -> ContextId {
    let ctx = store
        .create_context(title, &format!("# {title}\n"), T0)
        .unwrap();
    store
        .enqueue_check(
            &ctx,
            &kudo_context::Check {
                what: "the backup ran".into(),
                pass: "a snapshot newer than 26 hours".into(),
                window: "the last 26 hours".into(),
            },
            Some(due),
            T0,
        )
        .unwrap();
    ctx
}

fn wakes(store: &ContextStore, now: i64) -> Vec<Wake> {
    due_contexts(store, now, "home", QUIET).unwrap()
}

/// A due check with no worker wakes one, on the declared wake node.
#[test]
fn a_due_check_with_no_worker_wakes_one_on_its_node() {
    let store = store();
    let ctx = due_check(&store, "lab", T0 + 60);
    store.declare_wake_node(&ctx, "yolo", T0).unwrap();
    assert!(wakes(&store, T0 + 59).is_empty(), "not before its time");
    assert_eq!(
        wakes(&store, T0 + 60),
        vec![Wake {
            context: ctx,
            node: "yolo".into()
        }]
    );
}

/// With no wake node declared (and no repo), the waker's own node.
#[test]
fn without_a_wake_node_the_wakers_own() {
    let store = store();
    let ctx = due_check(&store, "lab", T0);
    assert_eq!(wakes(&store, T0)[0].node, "home");
    assert_eq!(wakes(&store, T0)[0].context, ctx);
}

/// An ordinary item never wakes anyone: it waits for a person.
#[test]
fn an_ordinary_item_does_not_wake() {
    let store = store();
    let ctx = store.create_context("plain", "# plain\n", T0).unwrap();
    store.enqueue(&ctx, "something", T0).unwrap();
    assert!(wakes(&store, T0 + 600).is_empty());
}

/// A live worker — traced, not ended, not quiet — means no wake; an ended or
/// quiet one does not stop it.
#[test]
fn a_live_worker_means_no_wake_an_ended_one_does_not() {
    let store = store();
    let ctx = due_check(&store, "lab", T0);
    store
        .record_trace(&ctx, "lease-1", &TraceEvent::LeaseStart, T0)
        .unwrap();
    assert!(wakes(&store, T0 + 10).is_empty(), "a live lease");
    assert_eq!(
        wakes(&store, T0 + QUIET + 10).len(),
        1,
        "a quiet lease is gone"
    );
    store
        .record_trace(
            &ctx,
            "lease-1",
            &TraceEvent::LeaseEnd("budget".into()),
            T0 + 20,
        )
        .unwrap();
    assert_eq!(wakes(&store, T0 + 30).len(), 1, "an ended lease");
}

/// A pane started in the last 15 minutes (its worker not traced yet) means no
/// wake: it is starting, maybe at a prompt.
#[test]
fn a_recent_pane_means_no_wake() {
    let store = store();
    let ctx = due_check(&store, "lab", T0);
    store
        .record_pane(&ctx, "lease-p", "yolo", "job-1", T0)
        .unwrap();
    assert!(wakes(&store, T0 + 60).is_empty());
    assert_eq!(wakes(&store, T0 + 15 * 60 + 1).len(), 1);
}

/// A context woken once is not woken again until its back-off passes: 15
/// minutes, doubling while its due item stays unanswered, up to 4 hours; it
/// resets once nothing is due there.
#[test]
fn a_woken_context_backs_off() {
    use kudo_shim::waker::Backoff;
    let ctx = ContextId("c".repeat(64));
    let mut backoff = Backoff::default();
    assert!(backoff.may_wake(&ctx, T0));
    backoff.woke(&ctx, T0);
    assert!(!backoff.may_wake(&ctx, T0 + 14 * 60));
    assert!(backoff.may_wake(&ctx, T0 + 15 * 60));
    backoff.woke(&ctx, T0 + 15 * 60);
    assert!(!backoff.may_wake(&ctx, T0 + 15 * 60 + 29 * 60), "doubled");
    assert!(backoff.may_wake(&ctx, T0 + 15 * 60 + 30 * 60));
    for n in 0..10 {
        backoff.woke(&ctx, T0 + n);
    }
    assert!(
        !backoff.may_wake(&ctx, T0 + 9 + 4 * 3600 - 1),
        "capped at 4 hours"
    );
    assert!(backoff.may_wake(&ctx, T0 + 9 + 4 * 3600));
    backoff.settled(&ctx);
    backoff.woke(&ctx, T0);
    assert!(
        backoff.may_wake(&ctx, T0 + 15 * 60),
        "reset after it settled"
    );
}

/// Panes that never traced and are past their grace: the waker stops them
/// before starting another, so they do not pile up.
#[test]
fn stale_panes_are_the_untraced_past_their_grace() {
    let store = store();
    let ctx = due_check(&store, "lab", T0);
    store
        .record_pane(&ctx, "lease-a", "yolo", "job-a", T0)
        .unwrap();
    store
        .record_pane(&ctx, "lease-b", "yolo", "job-b", T0)
        .unwrap();
    store
        .record_trace(&ctx, "lease-b", &TraceEvent::LeaseStart, T0 + 5)
        .unwrap();
    let stale = kudo_shim::waker::stale_panes(&store, &ctx, T0 + 15 * 60 + 1).unwrap();
    let jobs: Vec<&str> = stale.iter().map(|p| p.job.as_str()).collect();
    assert_eq!(jobs, vec!["job-a"]);
    assert!(
        kudo_shim::waker::stale_panes(&store, &ctx, T0 + 60)
            .unwrap()
            .is_empty()
    );
}

/// Without a repo or a declaration, the first repo's node is the default.
#[test]
fn the_first_repos_node_is_the_default() {
    let store = store();
    let ctx = due_check(&store, "lab", T0);
    store
        .declare_repo(
            &ctx,
            &kudo_context::Repo {
                name: "flakes".into(),
                node: "pc".into(),
                path: "/home/yj/.flakes".into(),
            },
            T0,
        )
        .unwrap();
    assert_eq!(wakes(&store, T0)[0].node, "pc");
}
