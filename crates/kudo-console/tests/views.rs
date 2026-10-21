//! What the console shows, read from the store: contexts with their live
//! workers, and the selected context's head and trace.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

#![cfg(feature = "server")]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_console::views;
use kudo_context::{ClaimId, ContextStore, TraceEvent};

const T0: i64 = 1_792_500_000;

fn store(name: &str) -> ContextStore {
    let dir = std::env::temp_dir().join(format!("kudo-console-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    ContextStore::init(&dir).unwrap()
}

#[test]
fn a_card_shows_counts_and_the_leases_still_running() {
    let store = store("cards");
    let ctx = store.create_context("carded", "# carded\n", T0).unwrap();
    store.enqueue(&ctx, "one", T0).unwrap();
    store
        .record_trace(&ctx, "lease-live", &TraceEvent::LeaseStart, T0)
        .unwrap();
    store
        .record_trace(
            &ctx,
            "lease-live",
            &TraceEvent::Pull(ClaimId("c1".into())),
            T0 + 1,
        )
        .unwrap();
    store
        .record_trace(&ctx, "lease-done", &TraceEvent::LeaseStart, T0)
        .unwrap();
    store
        .record_trace(
            &ctx,
            "lease-done",
            &TraceEvent::LeaseEnd("queue-empty".into()),
            T0 + 2,
        )
        .unwrap();

    let cards = views::contexts(&store, T0 + 3).unwrap();
    assert_eq!(cards.len(), 1);
    let card = &cards[0];
    assert_eq!(card.title, "carded");
    assert_eq!(card.ready, 1);
    let leases: Vec<&str> = card.workers.iter().map(|w| w.lease.as_str()).collect();
    assert_eq!(leases, ["lease-live"], "an ended lease is not a worker");
    assert_eq!(card.workers[0].last, "pull c1");
}

#[test]
fn the_detail_is_the_head_and_the_trace_newest_first() {
    let store = store("detail");
    let ctx = store
        .create_context("detailed", "# detailed\nstate: new\n", T0)
        .unwrap();
    store
        .record_trace(&ctx, "l", &TraceEvent::LeaseStart, T0)
        .unwrap();
    store
        .record_trace(&ctx, "l", &TraceEvent::LeaseEnd("budget".into()), T0 + 5)
        .unwrap();
    let detail = views::detail(&store, &ctx.0).unwrap();
    assert_eq!(detail.title, "detailed");
    assert!(detail.head.contains("state: new"));
    assert!(!detail.forked);
    let lines: Vec<&str> = detail.trace.iter().map(|t| t.line.as_str()).collect();
    assert_eq!(lines, ["lease-end budget", "lease-start"]);
}
