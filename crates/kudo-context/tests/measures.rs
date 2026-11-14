//! Dogfood measures are one projection over the context's records.
//!
//! implements: 94d1e02f634b9ddb6affcbf9079e5ca6467f286ec501de792189f6bf42e40da5

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_context::{ContextStore, Established, Submission, TraceEvent};

const T0: i64 = 1_792_500_000;

#[test]
fn measures_cover_the_loop_reclaims_downgrades_and_inbox() {
    let dir = std::env::temp_dir().join(format!("kudo-context-measures-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = ContextStore::init(&dir).unwrap();
    let ctx = store.create_context("dogfood", "# dogfood\n", T0).unwrap();
    let first = store.enqueue(&ctx, "first", T0).unwrap();
    store.enqueue(&ctx, "blocked later", T0).unwrap();

    let trace = |lease: &str, event: TraceEvent, at| {
        store.record_trace(&ctx, lease, &event, at).unwrap();
    };
    trace("lease-a", TraceEvent::LeaseStart, T0);
    let claim = store.pull(&ctx, "lease-a", T0, 600).unwrap().unwrap();
    trace("lease-a", TraceEvent::Pull(claim.id.clone()), T0);
    store.release(&claim.id, T0 + 1).unwrap();
    trace("lease-a", TraceEvent::Release(claim.id), T0 + 1);
    trace(
        "lease-a",
        TraceEvent::LeaseEnd("queue-empty".into()),
        T0 + 2,
    );

    trace("lease-b", TraceEvent::LeaseStart, T0 + 3);
    let claim = store.pull(&ctx, "lease-b", T0 + 3, 600).unwrap().unwrap();
    assert_eq!(claim.item.id, first);
    assert_eq!(claim.attempt, 2);
    trace("lease-b", TraceEvent::Pull(claim.id.clone()), T0 + 3);
    let submission = Submission {
        head: "after first\n".into(),
        log: "first".into(),
        established: vec![Established {
            claim: "unbacked".into(),
            evidence: Vec::new(),
        }],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &submission, T0 + 4).unwrap();
    trace(
        "lease-b",
        TraceEvent::Submit {
            claim: claim.id,
            submission: done.id,
        },
        T0 + 4,
    );

    let blocked = store.pull(&ctx, "lease-b", T0 + 5, 600).unwrap().unwrap();
    trace("lease-b", TraceEvent::Pull(blocked.id.clone()), T0 + 5);
    let mut attempt = Submission {
        head: "not landed\n".into(),
        log: "blocked".into(),
        ..Default::default()
    };
    attempt.blocked_on = vec![first];
    store.submit(&blocked.id, &attempt, T0 + 6).unwrap();
    trace(
        "lease-b",
        TraceEvent::Submit {
            claim: blocked.id,
            submission: kudo_context::SubmissionId("blocked-submission".into()),
        },
        T0 + 6,
    );
    trace("lease-b", TraceEvent::LeaseEnd("takeover".into()), T0 + 7);
    store
        .file_inbox(&ctx, "lease-b", "rough edge", None, T0 + 8)
        .unwrap();

    let got = store.measures(&ctx).unwrap();
    assert_eq!(got.leases, 2);
    assert_eq!(got.claims, 3);
    assert_eq!(got.submit_attempts, 2);
    assert_eq!(got.completed, 1);
    assert_eq!(got.blocked, 1);
    assert_eq!(got.releases, 1);
    assert_eq!(got.reclaims, 1);
    assert_eq!(
        got.lease_ends,
        vec![("queue-empty".into(), 1), ("takeover".into(), 1)]
    );
    assert_eq!(got.downgraded_submissions, Some(1));
    assert_eq!(got.inbox_entries, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
