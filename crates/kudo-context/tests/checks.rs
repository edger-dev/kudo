//! Due items, checks and their verdicts, and where a context wakes.
//!
//! implements: 716432a845eb22babf957938aaa9a9c0f3cd3bea3ebee24c89a5147a795e7e2b
//! implements: c11043084f97c61ff3f41e4d07fb2361aa871213021e9c15d0e4cee37c99bde1
//! implements: 4afb77c56fe120a92c8ecb4f01b9a8873e9a41fb8eecb450b9a60e1b1ec327c0

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use kudo_context::{Check, CheckClose, CheckVerdict, ContextStore, Error, Repo, Submission};

const T0: i64 = 1_792_500_000;
const TTL: i64 = 600;

fn store() -> ContextStore {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kudo-context-checks-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    ContextStore::init(&dir).unwrap()
}

fn key_check() -> Check {
    Check {
        what: "the R2 access key still works".into(),
        pass: "a list of the bucket succeeds".into(),
        window: "one attempt, now".into(),
    }
}

fn done(head: &str, close: Option<CheckClose>) -> Submission {
    Submission {
        head: head.into(),
        log: "looked".into(),
        check: close,
        ..Default::default()
    }
}

/// A due item is not ready before its time, and is from then on.
#[test]
fn a_due_item_waits_for_its_time() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    store
        .enqueue_due(&ctx, "later", Some(T0 + 100), T0)
        .unwrap();
    assert!(!store.has_ready(&ctx, T0 + 99).unwrap());
    assert_eq!(store.ready_count(&ctx, T0 + 99).unwrap(), 0);
    assert!(store.pull(&ctx, "l", T0 + 99, TTL).unwrap().is_none());
    assert_eq!(store.due_ready_count(&ctx, T0 + 99).unwrap(), 0);

    assert!(store.has_ready(&ctx, T0 + 100).unwrap());
    assert_eq!(store.due_ready_count(&ctx, T0 + 100).unwrap(), 1);
    let claim = store.pull(&ctx, "l", T0 + 100, TTL).unwrap().unwrap();
    assert_eq!(claim.item.ask, "later");
    assert_eq!(claim.item.due, Some(T0 + 100));
}

/// An ordinary item is ready at once and never counts as due: the waker
/// does not wake for it.
#[test]
fn an_ordinary_item_is_ready_at_once_and_not_due() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    store.enqueue(&ctx, "now", T0).unwrap();
    assert!(store.has_ready(&ctx, T0).unwrap());
    assert_eq!(store.due_ready_count(&ctx, T0).unwrap(), 0);
}

/// Items not yet due are listed with their time, soonest first.
#[test]
fn not_yet_due_items_are_listed_with_their_time() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    store.enqueue_due(&ctx, "b", Some(T0 + 200), T0).unwrap();
    store.enqueue_due(&ctx, "a", Some(T0 + 100), T0).unwrap();
    store.enqueue(&ctx, "now", T0).unwrap();
    let later: Vec<(String, i64)> = store
        .not_yet_due(&ctx, T0 + 10)
        .unwrap()
        .into_iter()
        .map(|i| (i.ask, i.due.unwrap()))
        .collect();
    assert_eq!(later, vec![("a".into(), T0 + 100), ("b".into(), T0 + 200)]);
}

/// A check carries what to look at, its pass condition and its window; its
/// claim hands them over.
#[test]
fn a_check_carries_its_condition_and_window() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    store
        .enqueue_check(&ctx, &key_check(), Some(T0), T0)
        .unwrap();
    let claim = store.pull(&ctx, "l", T0, TTL).unwrap().unwrap();
    assert_eq!(claim.item.check, Some(key_check()));
    assert_eq!(claim.item.ask, key_check().what);
}

/// A check's claim needs a verdict; an ordinary item's refuses one.
#[test]
fn a_check_needs_a_verdict_and_an_item_refuses_one() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    store.enqueue_check(&ctx, &key_check(), None, T0).unwrap();
    let claim = store.pull(&ctx, "l", T0, TTL).unwrap().unwrap();
    assert!(matches!(
        store.submit(&claim.id, &done("# lab\n", None), T0 + 1),
        Err(Error::CheckNeedsVerdict(_))
    ));

    let ctx2 = store.create_context("plain", "# plain\n", T0).unwrap();
    store.enqueue(&ctx2, "plain", T0).unwrap();
    let plain = store.pull(&ctx2, "l", T0, TTL).unwrap().unwrap();
    let pass = CheckClose {
        verdict: CheckVerdict::Pass,
        note: "fine".into(),
    };
    assert!(matches!(
        store.submit(&plain.id, &done("# plain\n", Some(pass)), T0 + 1),
        Err(Error::NotACheck(_))
    ));
}

/// Pass closes the check, and the log says so: the log is the check log.
#[test]
fn a_pass_closes_the_check_in_the_log() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    store.enqueue_check(&ctx, &key_check(), None, T0).unwrap();
    let claim = store.pull(&ctx, "l", T0, TTL).unwrap().unwrap();
    let close = CheckClose {
        verdict: CheckVerdict::Pass,
        note: "listed 3 objects".into(),
    };
    let submitted = store
        .submit(&claim.id, &done("# lab\nkey ok\n", Some(close)), T0 + 1)
        .unwrap();
    assert_eq!(submitted.requeued, None);
    assert!(!store.has_ready(&ctx, T0 + 2).unwrap());
    let log = store.log(&ctx).unwrap().join("\n");
    assert!(log.contains("verdict: pass"), "{log}");
    assert!(log.contains("listed 3 objects"), "{log}");
}

/// Snooze closes this item and enqueues the same check, due at the time
/// given; a snooze to a time not in the future is refused.
#[test]
fn a_snooze_requeues_the_check_due_later() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    store.enqueue_check(&ctx, &key_check(), None, T0).unwrap();
    let claim = store.pull(&ctx, "l", T0, TTL).unwrap().unwrap();

    let past = CheckClose {
        verdict: CheckVerdict::Snooze { until: T0 },
        note: "again".into(),
    };
    assert!(matches!(
        store.submit(&claim.id, &done("# lab\n", Some(past)), T0 + 1),
        Err(Error::InvalidCheck(_))
    ));

    let later = CheckClose {
        verdict: CheckVerdict::Snooze { until: T0 + 3600 },
        note: "the key rotates tonight".into(),
    };
    let submitted = store
        .submit(&claim.id, &done("# lab\n", Some(later)), T0 + 1)
        .unwrap();
    let again = submitted.requeued.expect("a snooze re-queues the check");
    assert!(!store.has_ready(&ctx, T0 + 3599).unwrap());
    let next = store.pull(&ctx, "l2", T0 + 3600, TTL).unwrap().unwrap();
    assert_eq!(next.item.id, again);
    assert_eq!(next.item.check, Some(key_check()));
    assert_eq!(next.item.snoozed_from, Some(claim.item.id.clone()));
    let log = store.log(&ctx).unwrap().join("\n");
    assert!(log.contains("verdict: snooze"), "{log}");
}

/// A context's wake node: declared (the latest wins), else its first repo's
/// node, else none (the waker's own).
#[test]
fn the_wake_node_is_declared_or_follows_the_repos() {
    let store = store();
    let ctx = store.create_context("lab", "# lab\n", T0).unwrap();
    assert_eq!(store.wake_node(&ctx).unwrap(), None);
    store
        .declare_repo(
            &ctx,
            &Repo {
                name: "flakes".into(),
                node: "pc".into(),
                path: PathBuf::from("/home/yj/.flakes"),
            },
            T0,
        )
        .unwrap();
    assert_eq!(store.wake_node(&ctx).unwrap().as_deref(), Some("pc"));
    store.declare_wake_node(&ctx, "yolo", T0 + 1).unwrap();
    assert_eq!(store.wake_node(&ctx).unwrap().as_deref(), Some("yolo"));
    store
        .declare_wake_node(&ctx, "edger::yolo", T0 + 2)
        .unwrap();
    assert_eq!(
        store.wake_node(&ctx).unwrap().as_deref(),
        Some("edger::yolo")
    );
}
