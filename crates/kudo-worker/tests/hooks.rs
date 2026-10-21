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
        }
    }

    fn worker(&self, max_turns: u32) -> Worker<'_> {
        Worker::new(
            &self.store,
            &self.book,
            WorkerConfig {
                context: self.ctx.clone(),
                lease: LEASE.into(),
                claim_ttl: 600,
                max_turns,
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
