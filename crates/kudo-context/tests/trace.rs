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

/// Within a lease, its own order (seq) wins over the clock: a clock that
/// stepped back does not reorder a lease's events.
#[test]
fn a_lease_reads_in_its_own_order_when_the_clock_steps_back() {
    let (dir, store) = store("trace-clock");
    let ctx = store.create_context("traced", "# traced\n", T0).unwrap();
    store
        .record_trace(&ctx, "lease-a", &TraceEvent::LeaseStart, T0 + 5)
        .unwrap();
    store
        .record_trace(
            &ctx,
            "lease-a",
            &TraceEvent::LeaseEnd("budget".into()),
            T0 + 4,
        )
        .unwrap();
    let seqs: Vec<u64> = store.trace(&ctx).unwrap().iter().map(|t| t.seq).collect();
    assert_eq!(seqs, [1, 2]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An event travels as one line — the trace kino's own text — and reads back
/// as the same event.
// implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d
#[test]
fn an_event_round_trips_as_one_line() {
    use kudo_context::{ClaimId, SubmissionId};
    for event in [
        TraceEvent::LeaseStart,
        TraceEvent::Handoff(ClaimId("c1".into())),
        TraceEvent::Pull(ClaimId("c2".into())),
        TraceEvent::Refused(ClaimId("c3".into())),
        TraceEvent::Release(ClaimId("c4".into())),
        TraceEvent::Submit {
            claim: ClaimId("c5".into()),
            submission: SubmissionId("s5".into()),
        },
        TraceEvent::LeaseEnd("queue-empty".into()),
        // implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
        TraceEvent::Wait,
    ] {
        let line = event.to_line();
        assert!(!line.contains('\n'), "{line:?}");
        assert!(line.starts_with(event.name()), "{line:?}");
        assert_eq!(TraceEvent::from_line(&line), Some(event));
    }
    assert_eq!(TraceEvent::from_line("nonsense"), None);
}

/// A worker the console started in a node's pane is recorded with where it
/// runs, so the console can show that pane and type into it.
#[test]
fn a_pane_records_where_a_lease_runs() {
    let (dir, store) = store("panes");
    let ctx = store.create_context("paned", "# p\n", T0).unwrap();
    assert!(store.panes(&ctx).unwrap().is_empty());
    store
        .record_pane(&ctx, "lease-p", "yolo", "job-17", T0)
        .unwrap();
    let panes = store.panes(&ctx).unwrap();
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].lease, "lease-p");
    assert_eq!(panes[0].node, "yolo");
    assert_eq!(panes[0].job, "job-17");
    let other = store.create_context("other", "# o\n", T0).unwrap();
    assert!(store.panes(&other).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn panes_read_oldest_first() {
    let (dir, store) = store("panes-order");
    let ctx = store.create_context("p", "# p\n", T0).unwrap();
    for (n, job) in ["j1", "j2", "j3"].iter().enumerate() {
        store
            .record_pane(&ctx, "lease-x", "yolo", job, T0 + n as i64)
            .unwrap();
    }
    let jobs: Vec<String> = store
        .panes(&ctx)
        .unwrap()
        .into_iter()
        .map(|p| p.job)
        .collect();
    assert_eq!(jobs, ["j1", "j2", "j3"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A governed job a worker starts is traced in its context: where it ran,
/// its id (the pointer into the node's audit), whether it waited for a
/// verdict, what it ran — and, when the worker sees it end, how and under
/// which verdict.
// implements: e23a8b06aa9331b3f1ffd8df36be1392daf44e93ca20037b50cec6c3132f53e1@ff25c7f76a20fd286e5949164fa3e1573577d38145c38f29c0e8b0664d3179f1
#[test]
fn a_governed_job_is_traced_from_start_to_end() {
    let (dir, store) = store("trace-job");
    let ctx = store.create_context("jobs", "# jobs\n", T0).unwrap();
    let start = TraceEvent::JobStart {
        node: "pc".into(),
        job: "1792-1-1".into(),
        waiting: true,
        argv: "df -h /".into(),
    };
    let end = TraceEvent::JobEnd {
        node: "pc".into(),
        job: "1792-1-1".into(),
        status: "done:0".into(),
        verdict: "approved-once".into(),
    };
    store.record_trace(&ctx, "lease-j", &start, T0).unwrap();
    store.record_trace(&ctx, "lease-j", &end, T0 + 1).unwrap();
    let got: Vec<TraceEvent> = store
        .trace(&ctx)
        .unwrap()
        .into_iter()
        .map(|e| e.event)
        .collect();
    assert_eq!(got, vec![start.clone(), end.clone()]);
    assert_eq!(start.name(), "job-start");
    assert_eq!(end.name(), "job");
    for e in [&start, &end] {
        assert_eq!(TraceEvent::from_line(&e.to_line()).as_ref(), Some(e));
    }
    assert_eq!(start.to_line(), "job-start pc 1792-1-1 waiting df -h /");
    assert_eq!(end.to_line(), "job pc 1792-1-1 done:0 approved-once");
    let _ = std::fs::remove_dir_all(dir);
}

/// A shell call is recorded with its exit and a capped excerpt of its
/// output; a submission citing it is established, and one citing another
/// context's call is not.
// implements: 6061a14e7f9b9eb2c7a3bbc0f5ff4f3778ceb08d71d46f8de4515f62ef3a2a0b@fdf56a326ad7b945b7ee31bb9503ee3365f629c5bf46347c3a8c970398fa65bd
// implements: 4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9
#[test]
fn a_recorded_shell_call_is_evidence_in_its_context() {
    let (dir, store) = store("trace-toolcall");
    let ctx = store.create_context("calls", "# calls\n", T0).unwrap();
    let other = store.create_context("other", "# other\n", T0).unwrap();
    let long = format!("{}MIDDLE{}", "a".repeat(5000), "z".repeat(5000));
    let call = store
        .record_tool_call(&ctx, "lease-t", "df -h /", Some(0), &long, T0)
        .unwrap();
    let elsewhere = store
        .record_tool_call(&other, "lease-o", "uptime", Some(0), "up 3 days", T0)
        .unwrap();
    let events: Vec<TraceEvent> = store
        .trace(&ctx)
        .unwrap()
        .into_iter()
        .map(|e| e.event)
        .collect();
    assert_eq!(
        events,
        vec![TraceEvent::ToolCall {
            command: "df -h /".into(),
            exit: Some(0)
        }]
    );
    let body = store.kino_text(&call).unwrap();
    assert!(body.contains("df -h /") && body.contains("aaaa") && body.contains("zzzz"));
    assert!(!body.contains("MIDDLE"), "the middle is cut");
    assert!(body.len() < 4096 + 512, "capped: {}", body.len());

    store.enqueue(&ctx, "look", T0).unwrap();
    let claim = store.pull(&ctx, "lease-t", T0, 600).unwrap().unwrap();
    let sub = Submission {
        head: "# calls\nroot has room\n".into(),
        log: "looked".into(),
        established: vec![
            kudo_context::Established {
                claim: "root has room".into(),
                evidence: vec![call.clone()],
            },
            kudo_context::Established {
                claim: "borrowed".into(),
                evidence: vec![elsewhere.clone()],
            },
        ],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert_eq!(done.downgraded, vec!["borrowed".to_owned()]);
    let _ = std::fs::remove_dir_all(dir);
}

/// A job event's trace kino is evidence too: recording returns its id.
// implements: 6061a14e7f9b9eb2c7a3bbc0f5ff4f3778ceb08d71d46f8de4515f62ef3a2a0b@fdf56a326ad7b945b7ee31bb9503ee3365f629c5bf46347c3a8c970398fa65bd
#[test]
fn a_job_event_is_evidence_in_its_context() {
    let (dir, store) = store("trace-jobevidence");
    let ctx = store.create_context("jobs", "# jobs\n", T0).unwrap();
    let end = store
        .record_trace(
            &ctx,
            "lease-j",
            &TraceEvent::JobEnd {
                node: "pc".into(),
                job: "j".into(),
                status: "done:0".into(),
                verdict: "seed-allow".into(),
            },
            T0,
        )
        .unwrap();
    store.enqueue(&ctx, "look", T0).unwrap();
    let claim = store.pull(&ctx, "lease-j", T0, 600).unwrap().unwrap();
    let sub = Submission {
        head: "# jobs\nran\n".into(),
        log: "ran".into(),
        established: vec![kudo_context::Established {
            claim: "it ran".into(),
            evidence: vec![end],
        }],
        ..Default::default()
    };
    assert!(
        store
            .submit(&claim.id, &sub, T0 + 1)
            .unwrap()
            .downgraded
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A long command keeps at most 512 bytes in the trace line.
#[test]
fn a_long_command_is_capped_in_the_line() {
    let (dir, store) = store("trace-longcmd");
    let ctx = store.create_context("calls", "# calls\n", T0).unwrap();
    let cmd = format!("echo {}", "x".repeat(5000));
    store
        .record_tool_call(&ctx, "l", &cmd, None, "", T0)
        .unwrap();
    match &store.trace(&ctx).unwrap()[0].event {
        TraceEvent::ToolCall { command, exit } => {
            assert!(command.len() <= 512, "{}", command.len());
            assert_eq!(*exit, None);
        }
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(dir);
}
