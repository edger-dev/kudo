//! The loop's own trace: write-once events per lease, and the forward chain
//! from a head version to the submission, claim, item and lease behind it.
//!
//! implements: a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098@a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_context::{ContextStore, Submission, TraceEvent};

const T0: i64 = 1_792_500_000;

fn store(name: &str) -> (std::path::PathBuf, ContextStore) {
    let dir = std::env::temp_dir().join(format!("kudo-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    (dir, store)
}

#[test]
fn a_lease_trace_reads_in_order_and_a_context_trace_holds_every_lease() {
    let (dir, store) = store("trace-order");
    let ctx = store.create_context("traced", "# traced\n", T0).unwrap();
    store.enqueue(&ctx, "one", T0).unwrap();
    let claim = store.pull(&ctx, "lease-a", T0, 600).unwrap().unwrap();
    let sub = Submission {
        head: "# traced\ndone\n".into(),
        log: "done".into(),
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 2).unwrap();

    let events = [
        TraceEvent::LeaseStart,
        TraceEvent::Handoff(claim.id.clone()),
        TraceEvent::Submit {
            claim: claim.id.clone(),
            submission: done.id.clone(),
        },
        TraceEvent::LeaseEnd("queue-empty".into()),
    ];
    for (n, e) in events.iter().enumerate() {
        store
            .record_trace(&ctx, "lease-a", e, T0 + n as i64)
            .unwrap();
    }
    // The same event twice in one lease is two entries.
    store
        .record_trace(&ctx, "lease-b", &TraceEvent::LeaseStart, T0 + 1)
        .unwrap();
    store
        .record_trace(&ctx, "lease-b", &TraceEvent::LeaseStart, T0 + 1)
        .unwrap();

    let all = store.trace(&ctx).unwrap();
    assert_eq!(all.len(), 6, "{all:?}");
    let a: Vec<&TraceEvent> = all
        .iter()
        .filter(|t| t.lease == "lease-a")
        .map(|t| &t.event)
        .collect();
    assert_eq!(a, events.iter().collect::<Vec<_>>());
    let seqs: Vec<u64> = all
        .iter()
        .filter(|t| t.lease == "lease-a")
        .map(|t| t.seq)
        .collect();
    assert_eq!(seqs, [1, 2, 3, 4]);

    let other = store.create_context("other", "# other\n", T0).unwrap();
    assert!(store.trace(&other).unwrap().is_empty(), "per context");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_head_version_traces_forward_to_its_submission_claim_item_and_lease() {
    let (dir, store) = store("trace-forward");
    let ctx = store.create_context("traced", "# traced\n", T0).unwrap();
    let first = store.head(&ctx).unwrap().version;
    assert_eq!(
        store.provenance(&ctx, &first).unwrap(),
        None,
        "the first head"
    );

    store.enqueue(&ctx, "the ask behind it", T0).unwrap();
    let claim = store.pull(&ctx, "lease-a", T0, 600).unwrap().unwrap();
    let sub = Submission {
        head: "# traced\nanswered\n".into(),
        log: "answered".into(),
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    let head = store.head(&ctx).unwrap().version;

    let p = store.provenance(&ctx, &head).unwrap().unwrap();
    assert_eq!(p.submission, done.id);
    assert_eq!(p.claim, claim.id);
    assert_eq!(p.item.ask, "the ask behind it");
    assert_eq!(p.lease, "lease-a");
    assert_eq!(p.attempt, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
