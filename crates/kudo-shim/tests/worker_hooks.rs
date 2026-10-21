//! The worker side of the shim: `kudo-shim hook <event>` and the worker's MCP
//! tools, over a real context store and no hub.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use kudo_context::{ContextStore, Established, Submission};
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

/// A submission with just a head and a log line.
fn plain(head: &str, log: &str, base: Option<Vec<String>>) -> Submission {
    Submission {
        head: head.into(),
        log: log.into(),
        base,
        ..Default::default()
    }
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
        &plain("# shim\nstate: done\n", "did the item", None),
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
    worker_tools::submit(&s, &claim.id.0, &plain("new head\n", "log", None), T0 + 1).unwrap();
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
    assert!(worker_tools::submit(&s, "no-such-claim", &plain("h", "l", None), T0).is_err());
}

#[test]
fn after_a_submit_pull_says_to_end_the_turn_and_the_stop_hands_over_the_next() {
    let (s, store) = setup();
    store.enqueue(&s.context, "first", T0).unwrap();
    store.enqueue(&s.context, "second", T0).unwrap();

    let _ = worker_tools::pull(&s, T0).unwrap();
    let claim = store.held_by(&s.context, &s.lease, T0).unwrap().unwrap();
    worker_tools::submit(&s, &claim.id.0, &plain("h1\n", "one", None), T0 + 1).unwrap();

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

#[test]
fn the_inbox_tool_files_a_line_under_the_bound_context() {
    let (s, store) = setup();
    let reply = worker_tools::inbox(&s, "the head template is too long", None, T0).unwrap();
    assert!(reply.contains("filed"), "{reply}");
    let entries = store.inbox(&s.context).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].lease, s.lease);
}

// implements: 3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab@aaddfc2e7ee067ea5780ce6ee2dfe4587bfb47b0c067b2670a31308c019a7e21
#[test]
fn a_submit_from_a_moved_head_hands_back_the_head_as_it_is_now() {
    let (s, store) = setup();
    store.enqueue(&s.context, "mine", T0).unwrap();
    store.enqueue(&s.context, "theirs", T0).unwrap();
    let _ = worker_tools::pull(&s, T0).unwrap();
    let mine = store.held_by(&s.context, &s.lease, T0).unwrap().unwrap();

    // Another worker takes the second item and submits first.
    let theirs = store.pull(&s.context, "lease-2", T0, 600).unwrap().unwrap();
    store
        .submit(
            &theirs.id,
            &Submission {
                head: "# shim\ntheirs landed\n".into(),
                log: "theirs".into(),
                ..Default::default()
            },
            T0 + 1,
        )
        .unwrap();
    let now = store.head(&s.context).unwrap();

    let refused = worker_tools::submit(
        &s,
        &mine.id.0,
        &plain("# shim\nmine\n", "mine", None),
        T0 + 2,
    )
    .unwrap_err();
    assert!(
        refused.contains("theirs landed"),
        "the head as it is: {refused}"
    );
    assert!(
        refused.contains(&now.version),
        "the version to build on: {refused}"
    );
    assert!(refused.contains("base"), "how to resubmit: {refused}");

    worker_tools::submit(
        &s,
        &mine.id.0,
        &plain(
            "# shim\ntheirs landed\nmine\n",
            "mine",
            Some(vec![now.version.clone()]),
        ),
        T0 + 3,
    )
    .unwrap();
    assert_eq!(
        store.head(&s.context).unwrap().body,
        "# shim\ntheirs landed\nmine\n"
    );
}

// implements: 4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9@4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9
#[test]
fn the_submit_tool_says_which_lines_landed_as_claimed() {
    let (s, store) = setup();
    store.enqueue(&s.context, "one", T0).unwrap();
    let _ = worker_tools::pull(&s, T0).unwrap();
    let claim = store.held_by(&s.context, &s.lease, T0).unwrap().unwrap();
    let sub = Submission {
        head: "# shim
checked
"
        .into(),
        log: "checked it".into(),
        established: vec![
            Established {
                claim: "backed by a page".into(),
                evidence: vec!["https://example.org/page".into()],
            },
            Established {
                claim: "backed by nothing".into(),
                evidence: vec![],
            },
        ],
        ..Default::default()
    };
    let reply = worker_tools::submit(&s, &claim.id.0, &sub, T0 + 1).unwrap();
    assert!(reply.contains("backed by nothing"), "{reply}");
    assert!(reply.contains("claimed"), "{reply}");
    assert!(!reply.contains("backed by a page"), "{reply}");
}
