//! The hook decisions: one item per turn, the open-claim guard, and the four
//! conditions that end a lease.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use kudo_context::{ContextId, ContextStore, Submission};
use kudo_worker::{LeaseBook, LeaseEnd, Stop, Worker, WorkerConfig};

const T0: i64 = 1_792_500_000;
const LEASE: &str = "lease-1";

fn scratch() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "kudo-worker-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    store: ContextStore,
    book: LeaseBook,
    ctx: ContextId,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = scratch();
        let store = ContextStore::init(&dir.join("store")).unwrap();
        let ctx = store
            .create_context("hooks", "# hooks\nstate: active — the loop\n", T0)
            .unwrap();
        Self {
            store,
            book: LeaseBook::new(&dir.join("leases")),
            ctx,
            dir,
        }
    }

    fn store_dir(&self) -> PathBuf {
        self.dir.join("store")
    }

    fn worker(&self, max_turns: u32) -> Worker<'_> {
        self.waiting_worker(max_turns, 0)
    }

    /// A worker that waits up to `idle_limit` seconds for work.
    fn waiting_worker(&self, max_turns: u32, idle_limit: i64) -> Worker<'_> {
        Worker::new(
            &self.store,
            &self.book,
            WorkerConfig {
                context: self.ctx.clone(),
                lease: LEASE.into(),
                claim_ttl: 600,
                max_turns,
                idle_limit,
            },
        )
    }

    /// Submit the claim this lease holds, as the model would through `submit`.
    fn submit_held(&self, now: i64) {
        let claim = self
            .store
            .held_by(&self.ctx, LEASE, now)
            .unwrap()
            .expect("a held claim");
        let sub = Submission {
            head: format!("head after {}\n", claim.item.ask),
            log: "done".into(),
            ..Default::default()
        };
        self.store.submit(&claim.id, &sub, now).unwrap();
    }
}

fn block_reason(stop: Stop) -> String {
    match stop {
        Stop::Block { reason } => reason,
        other => panic!("expected the stop to be blocked, got {other:?}"),
    }
}

#[test]
fn session_start_injects_the_head_and_the_worker_rules() {
    let f = Fixture::new();
    let text = f.worker(10).session_start(T0).unwrap();
    assert!(text.contains("state: active — the loop"), "{text}");
    assert!(text.contains(&f.ctx.0), "names the bound context: {text}");
    assert!(
        text.contains("submit"),
        "says how to hand work back: {text}"
    );
}

#[test]
fn two_items_of_one_context_are_two_turns_of_one_session() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "first item", T0).unwrap();
    f.store.enqueue(&f.ctx, "second item", T0).unwrap();
    let w = f.worker(10);

    let reason = block_reason(w.stop(T0).unwrap());
    let held = f.store.held_by(&f.ctx, LEASE, T0).unwrap().unwrap();
    assert!(reason.contains("first item"), "{reason}");
    assert!(
        reason.contains(&held.id.0),
        "hands over the claim id: {reason}"
    );
    f.submit_held(T0 + 1);

    let reason = block_reason(w.stop(T0 + 2).unwrap());
    assert!(reason.contains("second item"), "{reason}");
    f.submit_held(T0 + 3);

    assert_eq!(w.stop(T0 + 4).unwrap(), Stop::Allow(LeaseEnd::QueueEmpty));
    assert_eq!(
        f.store.head(&f.ctx).unwrap().body,
        "head after second item\n"
    );
}

#[test]
fn stop_with_an_open_claim_and_no_submission_is_refused() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "unfinished", T0).unwrap();
    let w = f.worker(10);
    let _ = block_reason(w.stop(T0).unwrap());

    let reason = block_reason(w.stop(T0 + 1).unwrap());
    assert!(reason.contains("submit or release"), "{reason}");
    // Still one claim: the guard does not hand over anything new.
    let held = f.store.held_by(&f.ctx, LEASE, T0 + 1).unwrap().unwrap();
    assert_eq!(held.attempt, 1);
}

#[test]
fn an_empty_queue_ends_the_lease() {
    let f = Fixture::new();
    assert_eq!(
        f.worker(10).stop(T0).unwrap(),
        Stop::Allow(LeaseEnd::QueueEmpty)
    );
}

#[test]
fn compaction_ends_the_lease_and_gives_the_item_back() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "interrupted", T0).unwrap();
    f.store.enqueue(&f.ctx, "waiting", T0).unwrap();
    let w = f.worker(10);
    let _ = block_reason(w.stop(T0).unwrap());

    w.pre_compact(T0 + 1).unwrap();
    assert_eq!(w.stop(T0 + 2).unwrap(), Stop::Allow(LeaseEnd::Compaction));
    // The interrupted item is ready again for the next lease, as attempt 2.
    let next = f
        .store
        .pull(&f.ctx, "lease-2", T0 + 3, 600)
        .unwrap()
        .unwrap();
    assert_eq!(next.item.ask, "interrupted");
    assert_eq!(next.attempt, 2);
}

#[test]
fn a_spent_turn_budget_ends_the_lease() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "one", T0).unwrap();
    f.store.enqueue(&f.ctx, "two", T0).unwrap();
    let w = f.worker(1);

    let _ = block_reason(w.stop(T0).unwrap());
    f.submit_held(T0 + 1);
    assert_eq!(w.stop(T0 + 2).unwrap(), Stop::Allow(LeaseEnd::Budget));
    assert!(
        f.store.has_ready(&f.ctx, T0 + 3).unwrap(),
        "item two waits for the next lease"
    );
}

#[test]
fn a_human_prompt_ends_the_lease_even_with_a_claim_open() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "taken over", T0).unwrap();
    let w = f.worker(10);
    let _ = block_reason(w.stop(T0).unwrap());

    w.user_prompt("hold on, let me look", T0 + 1).unwrap();
    assert_eq!(w.stop(T0 + 2).unwrap(), Stop::Allow(LeaseEnd::Takeover));
    assert_eq!(
        f.store.held_by(&f.ctx, LEASE, T0 + 2).unwrap(),
        None,
        "the claim was released"
    );
}

#[test]
fn an_ended_lease_stays_ended_for_a_fresh_worker_on_the_same_lease() {
    let f = Fixture::new();
    f.worker(10).user_prompt("mine now", T0).unwrap();
    f.store.enqueue(&f.ctx, "arrives later", T0 + 1).unwrap();
    // Hooks run as separate processes: the record must outlive the Worker.
    assert_eq!(
        f.worker(10).stop(T0 + 2).unwrap(),
        Stop::Allow(LeaseEnd::Takeover)
    );
}

#[test]
fn the_launchers_kickoff_prompt_is_not_a_takeover() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "first", T0).unwrap();
    let w = f.worker(10);
    w.user_prompt(kudo_worker::KICKOFF, T0).unwrap();
    let _ = block_reason(w.stop(T0 + 1).unwrap());
}

#[test]
fn a_guard_refused_three_times_gives_the_item_back_and_ends_the_lease() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "cannot be submitted", T0).unwrap();
    let w = f.worker(10);
    let _ = block_reason(w.stop(T0).unwrap()); // the handoff

    for n in 1..=3 {
        let reason = block_reason(w.stop(T0 + n).unwrap());
        assert!(
            reason.contains("submit or release"),
            "refusal {n}: {reason}"
        );
    }
    assert_eq!(w.stop(T0 + 4).unwrap(), Stop::Allow(LeaseEnd::Stuck));
    assert_eq!(
        f.store.held_by(&f.ctx, LEASE, T0 + 5).unwrap(),
        None,
        "released"
    );
    assert!(f.store.has_ready(&f.ctx, T0 + 5).unwrap());
}

#[test]
fn a_submit_between_refusals_resets_the_guard() {
    let f = Fixture::new();
    for ask in ["a", "b"] {
        f.store.enqueue(&f.ctx, ask, T0).unwrap();
    }
    let w = f.worker(10);
    let _ = block_reason(w.stop(T0).unwrap());
    let _ = block_reason(w.stop(T0 + 1).unwrap());
    let _ = block_reason(w.stop(T0 + 2).unwrap());
    f.submit_held(T0 + 3);
    let _ = block_reason(w.stop(T0 + 4).unwrap()); // hands over b
    for n in 5..=7 {
        let reason = block_reason(w.stop(T0 + n).unwrap());
        assert!(reason.contains("submit or release"), "{reason}");
    }
}

/// The claim records the head the worker is shown with it: a worker on its
/// second turn must see what another worker established meanwhile.
// implements: 3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab@aaddfc2e7ee067ea5780ce6ee2dfe4587bfb47b0c067b2670a31308c019a7e21
#[test]
fn the_handoff_carries_the_head_as_it_is_now() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "mine", T0).unwrap();
    f.store.enqueue(&f.ctx, "theirs", T0).unwrap();
    f.store.enqueue(&f.ctx, "next", T0).unwrap();
    let w = f.worker(10);
    let _ = block_reason(w.stop(T0).unwrap());
    f.submit_held(T0 + 1);

    // Another worker lands a head between this worker's turns.
    let theirs = f
        .store
        .pull(&f.ctx, "lease-other", T0 + 2, 600)
        .unwrap()
        .unwrap();
    let sub = Submission {
        head: "head with their finding\n".into(),
        log: "theirs".into(),
        ..Default::default()
    };
    f.store.submit(&theirs.id, &sub, T0 + 3).unwrap();

    let reason = block_reason(w.stop(T0 + 4).unwrap());
    assert!(reason.contains("next"), "{reason}");
    assert!(
        reason.contains("head with their finding"),
        "the head as it is now: {reason}"
    );
    let version = f.store.head(&f.ctx).unwrap().version;
    assert!(reason.contains(&version), "{reason}");
}

/// When the head is forked, a worker is shown every tip, not only the newest.
#[test]
fn a_forked_head_is_shown_whole() {
    let heads = [
        kudo_context::Head {
            version: "v-new".into(),
            body: "the newer tip\n".into(),
        },
        kudo_context::Head {
            version: "v-old".into(),
            body: "the older tip\n".into(),
        },
    ];
    let text = kudo_worker::render_heads(&heads);
    assert!(
        text.contains("the newer tip") && text.contains("the older tip"),
        "{text}"
    );
    assert!(text.contains("v-new") && text.contains("v-old"), "{text}");
    assert!(
        text.to_lowercase().contains("fork"),
        "says it is forked: {text}"
    );

    let one = kudo_worker::render_heads(&heads[..1]);
    assert!(!one.to_lowercase().contains("fork"), "{one}");
}

fn trace_of(f: &Fixture) -> Vec<kudo_context::TraceEvent> {
    f.store
        .trace(&f.ctx)
        .unwrap()
        .into_iter()
        .filter(|t| t.lease == LEASE)
        .map(|t| t.event)
        .collect()
}

/// The hooks write the loop's events as they happen.
// implements: a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098@a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098
#[test]
fn a_lease_writes_its_trace_start_handoff_submit_end() {
    use kudo_context::TraceEvent as T;
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "one", T0).unwrap();
    let w = f.worker(10);
    w.session_start(T0).unwrap();
    let _ = block_reason(w.stop(T0 + 1).unwrap());
    let claim = f.store.held_by(&f.ctx, LEASE, T0 + 1).unwrap().unwrap();
    let sub = Submission {
        head: "# hooks\ndone\n".into(),
        log: "done".into(),
        ..Default::default()
    };
    let done = w.submit(&claim.id, &sub, T0 + 2).unwrap();
    assert_eq!(w.stop(T0 + 3).unwrap(), Stop::Allow(LeaseEnd::QueueEmpty));

    assert_eq!(
        trace_of(&f),
        [
            T::LeaseStart,
            T::Handoff(claim.id.clone()),
            T::Submit {
                claim: claim.id,
                submission: done.id
            },
            T::LeaseEnd("queue-empty".into()),
        ]
    );
}

#[test]
fn refusals_a_stuck_release_and_a_takeover_are_traced() {
    use kudo_context::TraceEvent as T;
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "stuck", T0).unwrap();
    let w = f.worker(10);
    let _ = block_reason(w.stop(T0).unwrap());
    let claim = f.store.held_by(&f.ctx, LEASE, T0).unwrap().unwrap();
    for n in 1..=3 {
        let _ = block_reason(w.stop(T0 + n).unwrap());
    }
    assert_eq!(w.stop(T0 + 4).unwrap(), Stop::Allow(LeaseEnd::Stuck));
    assert_eq!(
        trace_of(&f),
        [
            T::Handoff(claim.id.clone()),
            T::Refused(claim.id.clone()),
            T::Refused(claim.id.clone()),
            T::Refused(claim.id.clone()),
            T::Release(claim.id.clone()),
            T::LeaseEnd("stuck".into()),
        ]
    );

    // A second lease on the same context: a human takes over mid-item.
    let g = Fixture::new();
    g.store.enqueue(&g.ctx, "mine now", T0).unwrap();
    let w = g.worker(10);
    let pulled = match w.pull(T0).unwrap() {
        kudo_worker::Pull::Claimed(c) => c,
        other => panic!("{other:?}"),
    };
    w.user_prompt("let me drive", T0 + 1).unwrap();
    assert_eq!(
        trace_of(&g),
        [
            T::Pull(pulled.id.clone()),
            T::Release(pulled.id),
            T::LeaseEnd("takeover".into()),
        ]
    );
}

/// SessionStart fires again on resume and after compaction: the lease starts
/// once, and an ended lease does not start again.
#[test]
fn a_lease_starts_once_and_not_again_after_it_ended() {
    use kudo_context::TraceEvent as T;
    let f = Fixture::new();
    let w = f.worker(10);
    w.session_start(T0).unwrap();
    w.session_start(T0 + 1).unwrap(); // resumed
    w.pre_compact(T0 + 2).unwrap();
    w.session_start(T0 + 3).unwrap(); // after the compaction
    assert_eq!(
        trace_of(&f),
        [T::LeaseStart, T::LeaseEnd("compaction".into())]
    );
}

/// A lease can only submit or release the claim it holds.
#[test]
fn another_leases_claim_cannot_be_submitted_or_released() {
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "theirs", T0).unwrap();
    let theirs = f
        .store
        .pull(&f.ctx, "lease-other", T0, 600)
        .unwrap()
        .unwrap();
    let w = f.worker(10);
    let sub = Submission {
        head: "# hooks\nstolen\n".into(),
        log: "stolen".into(),
        ..Default::default()
    };
    assert!(w.submit(&theirs.id, &sub, T0 + 1).is_err());
    assert!(w.release(&theirs.id, T0 + 1).is_err());
    assert!(
        f.store
            .held_by(&f.ctx, "lease-other", T0 + 1)
            .unwrap()
            .is_some(),
        "still theirs"
    );
    assert!(trace_of(&f).is_empty(), "nothing credited to this lease");
}

/// Every event the lease traces is handed to the observer too, in order.
// implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d
#[test]
fn the_observer_sees_what_the_trace_records() {
    use std::sync::{Arc, Mutex};
    let f = Fixture::new();
    f.store.enqueue(&f.ctx, "one", T0).unwrap();
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let w = f.worker(10).with_observer({
        let seen = seen.clone();
        Arc::new(
            move |ctx: &ContextId, lease: &str, event: &kudo_context::TraceEvent| {
                seen.lock()
                    .unwrap()
                    .push(format!("{}/{lease}/{}", ctx.0.len(), event.name()));
            },
        )
    });
    w.session_start(T0).unwrap();
    let _ = block_reason(w.stop(T0 + 1).unwrap());
    let names: Vec<String> = seen.lock().unwrap().clone();
    assert_eq!(
        names,
        [
            format!("64/{LEASE}/lease-start"),
            format!("64/{LEASE}/handoff")
        ]
    );
    let traced: Vec<&str> = trace_of(&f).iter().map(|e| e.name()).collect();
    assert_eq!(traced, ["lease-start", "handoff"]);
}

/// An event whose trace could not be written is not published: the channel
/// says only what the trace holds.
#[test]
fn an_event_the_trace_did_not_take_is_not_observed() {
    use std::sync::{Arc, Mutex};
    let f = Fixture::new();
    let seen: Arc<Mutex<u32>> = Arc::default();
    let w = f.worker(10).with_observer({
        let seen = seen.clone();
        Arc::new(
            move |_: &ContextId, _: &str, _: &kudo_context::TraceEvent| {
                *seen.lock().unwrap() += 1;
            },
        )
    });
    // The store's directory goes read-only: the trace write fails.
    let kinora = f.store_dir().join(".kinora");
    let ro = |mode| {
        use std::os::unix::fs::PermissionsExt;
        for sub in ["staged", "store"] {
            let _ =
                std::fs::set_permissions(kinora.join(sub), std::fs::Permissions::from_mode(mode));
        }
    };
    ro(0o555);
    let started = w.session_start(T0);
    ro(0o755);
    let _ = started;
    assert_eq!(
        *seen.lock().unwrap(),
        0,
        "nothing published that the trace lacks"
    );
}

// implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
#[test]
fn a_worker_with_nothing_ready_waits_and_takes_the_next_item_queued() {
    use kudo_context::TraceEvent as T;
    let f = Fixture::new();
    let w = f.waiting_worker(10, 3600);
    assert_eq!(w.stop(T0).unwrap(), Stop::Wait);
    assert_eq!(w.stop(T0 + 10).unwrap(), Stop::Wait);
    assert_eq!(trace_of(&f), [T::Wait], "the wait is traced once");

    f.store.enqueue(&f.ctx, "queued later", T0 + 20).unwrap();
    let reason = block_reason(w.stop(T0 + 30).unwrap());
    assert!(reason.contains("queued later"), "{reason}");

    // An item handed over resets the idle count.
    f.submit_held(T0 + 40);
    assert_eq!(w.stop(T0 + 50).unwrap(), Stop::Wait);
    assert_eq!(w.stop(T0 + 50 + 3599).unwrap(), Stop::Wait);
}

// implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
#[test]
fn a_worker_idle_past_its_limit_ends_its_lease() {
    let f = Fixture::new();
    let w = f.waiting_worker(10, 100);
    assert_eq!(w.stop(T0).unwrap(), Stop::Wait);
    assert_eq!(w.stop(T0 + 99).unwrap(), Stop::Wait);
    assert_eq!(w.stop(T0 + 100).unwrap(), Stop::Allow(LeaseEnd::QueueEmpty));
}

/// The hook's wait: an item queued while it waits is handed over.
// implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
#[test]
fn stop_waiting_hands_over_an_item_queued_mid_wait() {
    use std::cell::Cell;
    let f = Fixture::new();
    let w = f.waiting_worker(10, 3600);
    let clock = Cell::new(T0);
    let pauses = Cell::new(0);
    let stop = w
        .stop_waiting(
            || clock.get(),
            || {
                clock.set(clock.get() + 5);
                pauses.set(pauses.get() + 1);
                if pauses.get() == 3 {
                    f.store
                        .enqueue(&f.ctx, "arrived mid-wait", clock.get())
                        .unwrap();
                }
            },
            540,
        )
        .unwrap();
    assert!(block_reason(stop).contains("arrived mid-wait"));
    assert_eq!(pauses.get(), 3);
}

/// A window with nothing: the turn ends with a short idle reply, the wait
/// traced again so a live waiter never looks quiet.
// implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
#[test]
fn stop_waiting_ends_the_turn_when_the_window_passes() {
    use kudo_context::TraceEvent as T;
    use std::cell::Cell;
    let f = Fixture::new();
    let w = f.waiting_worker(10, 3600);
    let clock = Cell::new(T0);
    let stop = w
        .stop_waiting(|| clock.get(), || clock.set(clock.get() + 10), 30)
        .unwrap();
    let reason = block_reason(stop);
    assert!(reason.contains("Nothing is ready"), "{reason}");
    assert_eq!(trace_of(&f), [T::Wait, T::Wait]);
    assert!(clock.get() >= T0 + 30 && clock.get() < T0 + 60);
}

/// An item the model claims itself (the `pull` tool, in an idle turn) resets
/// the idle count like a handoff does.
// implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
#[test]
fn an_item_pulled_by_the_tool_resets_the_idle_count() {
    let f = Fixture::new();
    let w = f.waiting_worker(10, 100);
    assert_eq!(w.stop(T0).unwrap(), Stop::Wait);
    f.store.enqueue(&f.ctx, "pulled by hand", T0 + 50).unwrap();
    assert!(matches!(
        w.pull(T0 + 60).unwrap(),
        kudo_worker::Pull::Claimed(_)
    ));
    f.submit_held(T0 + 70);
    assert_eq!(
        w.stop(T0 + 120).unwrap(),
        Stop::Wait,
        "not ended: it just worked"
    );
}

/// A check's claim says it is a check: what passes it, the window it is
/// judged on, and that it closes with a verdict and a note.
// implements: c11043084f97c61ff3f41e4d07fb2361aa871213021e9c15d0e4cee37c99bde1
#[test]
fn a_checks_claim_names_its_condition_window_and_verdicts() {
    let g = Fixture::new();
    g.store
        .enqueue_check(
            &g.ctx,
            &kudo_context::Check {
                what: "the backup ran".into(),
                pass: "a snapshot newer than 26 hours".into(),
                window: "the last 26 hours".into(),
            },
            None,
            T0,
        )
        .unwrap();
    let w = g.worker(10);
    let claim = match w.pull(T0).unwrap() {
        kudo_worker::Pull::Claimed(c) => c,
        other => panic!("{other:?}"),
    };
    let text = w.claim_text(&claim).unwrap();
    for part in [
        "This is a check",
        "the backup ran",
        "a snapshot newer than 26 hours",
        "the last 26 hours",
        "`verdict`",
        "`pass`",
        "`fail`",
        "`snooze`",
        "snooze_until",
    ] {
        assert!(text.contains(part), "{part:?} missing from {text}");
    }
}

/// A due item is not handed over before its time: the Stop hook lets the
/// session wait (or end) instead.
// implements: 716432a845eb22babf957938aaa9a9c0f3cd3bea3ebee24c89a5147a795e7e2b@80a3c962223cc4b37ffceccfdfb83cb9d143fd3e956d4cbe19c7ec3e93774ffd
#[test]
fn a_due_item_is_not_handed_over_early() {
    let g = Fixture::new();
    g.store
        .enqueue_due(&g.ctx, "later", Some(T0 + 3600), T0)
        .unwrap();
    let w = g.worker(10);
    w.session_start(T0).unwrap();
    assert!(!matches!(w.stop(T0 + 1).unwrap(), Stop::Block { .. }));
}
