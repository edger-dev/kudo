//! The worker side of the shim: `kudo-shim hook <event>` and the worker's MCP
//! tools, over a real context store and no hub.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use kudo_context::ContextStore;
use kudo_shim::hook::{self, HookReply};
use kudo_shim::worker_tools;
use kudo_worker::{KICKOFF, WorkerSetup};

const T0: i64 = 1_792_500_000;

fn setup() -> (WorkerSetup, ContextStore) {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "kudo-shim-worker-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let context = store
        .create_context("shim", "# shim\nstate: active\n", T0)
        .unwrap();
    let setup = WorkerSetup {
        store_dir: dir,
        context,
        lease: "lease-1".into(),
        claim_ttl: 600,
        max_turns: 10,
    };
    (setup, store)
}

/// The JSON the CLI sends a hook: the fields the hooks read, plus noise.
fn event(name: &str, extra: &str) -> String {
    format!(
        r#"{{"session_id":"lease-1","transcript_path":"/tmp/t.jsonl","cwd":"/w","hook_event_name":"{name}"{extra}}}"#
    )
}

fn json(reply: &HookReply) -> serde_json::Value {
    assert_eq!(reply.code, 0, "{reply:?}");
    serde_json::from_str(&reply.stdout).unwrap_or_else(|e| panic!("{e}: {:?}", reply.stdout))
}

#[test]
fn session_start_adds_the_head_as_context() {
    let (s, _store) = setup();
    let out = json(&hook::run(
        "session-start",
        &event("SessionStart", r#","source":"startup""#),
        &s,
        T0,
    ));
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SessionStart");
    let text = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(text.contains("state: active"), "{text}");
}

#[test]
fn stop_blocks_with_the_next_item_and_allows_when_the_queue_is_empty() {
    let (s, store) = setup();
    store.enqueue(&s.context, "the item", T0).unwrap();

    let out = json(&hook::run(
        "stop",
        &event("Stop", r#","stop_hook_active":false"#),
        &s,
        T0,
    ));
    assert_eq!(out["decision"], "block");
    assert!(
        out["reason"].as_str().unwrap().contains("the item"),
        "{out}"
    );

    let claim = store.held_by(&s.context, &s.lease, T0).unwrap().unwrap();
    worker_tools::submit(
        &s,
        &claim.id.0,
        "# shim\nstate: done\n",
        "did the item",
        T0 + 1,
    )
    .unwrap();

    let allow = hook::run(
        "stop",
        &event("Stop", r#","stop_hook_active":true"#),
        &s,
        T0 + 2,
    );
    assert_eq!(
        allow,
        HookReply {
            stdout: String::new(),
            code: 0
        }
    );
}

#[test]
fn the_kickoff_prompt_keeps_the_lease_and_a_human_prompt_ends_it() {
    let (s, store) = setup();
    store.enqueue(&s.context, "one", T0).unwrap();
    let kick = format!(r#","prompt":{}"#, serde_json::to_string(KICKOFF).unwrap());
    let _ = hook::run(
        "user-prompt-submit",
        &event("UserPromptSubmit", &kick),
        &s,
        T0,
    );
    assert_eq!(
        json(&hook::run("stop", &event("Stop", ""), &s, T0 + 1))["decision"],
        "block"
    );

    let human = r#","prompt":"wait, let me drive""#;
    let _ = hook::run(
        "user-prompt-submit",
        &event("UserPromptSubmit", human),
        &s,
        T0 + 2,
    );
    let after = hook::run("stop", &event("Stop", ""), &s, T0 + 3);
    assert_eq!(after.stdout, "", "the lease ended; the stop is allowed");
}

#[test]
fn pre_compact_ends_the_lease() {
    let (s, store) = setup();
    store.enqueue(&s.context, "one", T0).unwrap();
    let _ = hook::run(
        "pre-compact",
        &event("PreCompact", r#","trigger":"auto""#),
        &s,
        T0,
    );
    assert_eq!(hook::run("stop", &event("Stop", ""), &s, T0 + 1).stdout, "");
}

#[test]
fn an_unknown_event_is_refused_loudly() {
    let (s, _store) = setup();
    let reply = hook::run("post-tool-use", "{}", &s, T0);
    assert_ne!(reply.code, 0);
}

#[test]
fn the_tools_pull_submit_and_release() {
    let (s, store) = setup();
    store.enqueue(&s.context, "pulled by tool", T0).unwrap();
    store.enqueue(&s.context, "released by tool", T0).unwrap();

    assert!(
        worker_tools::context_read(&s)
            .unwrap()
            .contains("state: active")
    );

    let pulled = worker_tools::pull(&s, T0).unwrap();
    assert!(pulled.contains("pulled by tool"), "{pulled}");
    let claim = store.held_by(&s.context, &s.lease, T0).unwrap().unwrap();
    assert!(pulled.contains(&claim.id.0), "names the claim: {pulled}");
    worker_tools::submit(&s, &claim.id.0, "new head\n", "log", T0 + 1).unwrap();
    assert_eq!(store.head(&s.context).unwrap().body, "new head\n");

    // The next item comes with the Stop handoff, not from a second pull.
    let _ = hook::run("stop", &event("Stop", ""), &s, T0 + 2);
    let second = store
        .held_by(&s.context, &s.lease, T0 + 2)
        .unwrap()
        .unwrap();
    worker_tools::release(&s, &second.id.0, T0 + 3).unwrap();
    assert!(store.has_ready(&s.context, T0 + 4).unwrap());

    assert!(
        worker_tools::pull(&s, T0 + 5)
            .unwrap()
            .contains("released by tool")
    );
}

#[test]
fn pull_on_an_empty_queue_says_so_and_a_bad_claim_is_an_error() {
    let (s, _store) = setup();
    assert!(
        worker_tools::pull(&s, T0)
            .unwrap()
            .to_lowercase()
            .contains("nothing")
    );
    assert!(worker_tools::submit(&s, "no-such-claim", "h", "l", T0).is_err());
}

#[test]
fn after_a_submit_pull_says_to_end_the_turn_and_the_stop_hands_over_the_next() {
    let (s, store) = setup();
    store.enqueue(&s.context, "first", T0).unwrap();
    store.enqueue(&s.context, "second", T0).unwrap();

    let _ = worker_tools::pull(&s, T0).unwrap();
    let claim = store.held_by(&s.context, &s.lease, T0).unwrap().unwrap();
    worker_tools::submit(&s, &claim.id.0, "h1\n", "one", T0 + 1).unwrap();

    let again = worker_tools::pull(&s, T0 + 2).unwrap();
    assert!(again.contains("end your turn"), "{again}");
    assert_eq!(
        store.held_by(&s.context, &s.lease, T0 + 2).unwrap(),
        None,
        "nothing claimed"
    );

    let out = json(&hook::run("stop", &event("Stop", ""), &s, T0 + 3));
    assert!(out["reason"].as_str().unwrap().contains("second"), "{out}");
}
