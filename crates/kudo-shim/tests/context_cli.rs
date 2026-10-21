//! `kudo-shim context new | enqueue | show`: making and driving a context by
//! hand, before the console exists.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_shim::cli;

const T0: i64 = 1_792_500_000;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn new_creates_the_store_and_prints_the_context_id() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let id = cli::context(&dir, &args(&["new", "dogfood"]), T0).unwrap();
    let id = id.trim();
    assert_eq!(id.len(), 64, "a kino id: {id}");

    cli::context(&dir, &args(&["enqueue", id, "write the first test"]), T0).unwrap();
    cli::context(&dir, &args(&["enqueue", id, "make it pass"]), T0).unwrap();
    let shown = cli::context(&dir, &args(&["show", id]), T0).unwrap();
    assert!(shown.contains("# dogfood"), "{shown}");
    assert!(shown.contains("2 ready"), "{shown}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_command_says_how_to_use_it() {
    let dir = std::env::temp_dir();
    let err = cli::context(&dir, &args(&["frobnicate"]), T0).unwrap_err();
    assert!(
        err.contains("new")
            && err.contains("enqueue")
            && err.contains("show")
            && err.contains("list")
            && err.contains("repo add"),
        "{err}"
    );
}

#[test]
fn list_shows_each_context_with_its_counts_and_a_prefix_names_one() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-list-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let listed = cli::context(&dir, &args(&["list"]), T0).unwrap();
    assert!(listed.contains("no contexts"), "{listed}");

    let a = cli::context(&dir, &args(&["new", "first", "one"]), T0).unwrap();
    let b = cli::context(&dir, &args(&["new", "second"]), T0 + 60).unwrap();
    let (a, b) = (a.trim(), b.trim());

    // A short prefix stands for the full id.
    cli::context(&dir, &args(&["enqueue", &b[..8], "an ask"]), T0 + 61).unwrap();
    let shown = cli::context(&dir, &args(&["show", &b[..8]]), T0 + 62).unwrap();
    assert!(shown.contains("1 ready"), "{shown}");

    let listed = cli::context(&dir, &args(&["list"]), T0 + 62).unwrap();
    let lines: Vec<&str> = listed.lines().collect();
    assert_eq!(lines.len(), 2, "{listed}");
    assert!(lines[0].starts_with(&b[..12]), "newest first: {listed}");
    assert!(
        lines[0].contains("second") && lines[0].contains("1 ready"),
        "{listed}"
    );
    assert!(
        lines[1].starts_with(&a[..12]) && lines[1].contains("first one"),
        "{listed}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// implements: b53b7abd832f2cd0e58e5084f5733af5d56f762c52cd6fd404913911920a6de9@b53b7abd832f2cd0e58e5084f5733af5d56f762c52cd6fd404913911920a6de9
#[test]
fn repo_add_declares_a_repo_and_show_lists_it() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-repo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = cli::context(&dir, &args(&["new", "spans"]), T0).unwrap();
    let ctx = ctx.trim();

    let said = cli::context(
        &dir,
        &args(&["repo", "add", &ctx[..8], "kudo", "/w/kudo", "--node", "n1"]),
        T0,
    )
    .unwrap();
    assert!(said.contains("kudo") && said.contains("n1"), "{said}");

    let shown = cli::context(&dir, &args(&["show", ctx]), T0).unwrap();
    assert!(shown.contains("repo kudo @ n1: /w/kudo"), "{shown}");

    let bad = cli::context(
        &dir,
        &args(&["repo", "add", ctx, "kudo", "rel/path", "--node", "n1"]),
        T0,
    );
    assert!(bad.is_err(), "{bad:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_worker_starts_in_a_declared_repo_on_this_node() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-wdir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = cli::context(&dir, &args(&["new", "spans"]), T0).unwrap();
    let ctx = ctx.trim();
    let repo = dir.join("kudo-repo");
    std::fs::create_dir_all(&repo).unwrap();
    cli::context(
        &dir,
        &args(&[
            "repo",
            "add",
            ctx,
            "kudo",
            repo.to_str().unwrap(),
            "--node",
            "n1",
        ]),
        T0,
    )
    .unwrap();

    assert_eq!(
        cli::worker_dir(&dir, ctx, Some("kudo"), "n1").unwrap(),
        Some(repo.clone())
    );
    assert_eq!(cli::worker_dir(&dir, ctx, None, "n1").unwrap(), None);
    let err = cli::worker_dir(&dir, ctx, Some("kudo"), "n2").unwrap_err();
    assert!(err.contains("repo add"), "says how to declare it: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}

// implements: a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098@a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098
#[test]
fn trace_lists_the_loop_events_and_why_names_what_wrote_the_head() {
    use kudo_context::{ContextId, ContextStore, Submission, TraceEvent};
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-trace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = cli::context(&dir, &args(&["new", "traced"]), T0).unwrap();
    let ctx = ctx.trim().to_owned();

    let why = cli::context(&dir, &args(&["why", &ctx]), T0).unwrap();
    assert!(why.contains("no submission"), "the first head: {why}");

    let store = ContextStore::open(&dir).unwrap();
    let id = ContextId(ctx.clone());
    store.enqueue(&id, "the ask", T0).unwrap();
    let claim = store.pull(&id, "lease-abc", T0, 600).unwrap().unwrap();
    store
        .record_trace(&id, "lease-abc", &TraceEvent::Pull(claim.id.clone()), T0)
        .unwrap();
    let sub = Submission {
        head: "# traced\nanswered\n".into(),
        log: "answered".into(),
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();

    let trace = cli::context(&dir, &args(&["trace", &ctx[..8]]), T0 + 2).unwrap();
    assert!(
        trace.contains("lease-ab") && trace.contains("pull"),
        "{trace}"
    );
    assert!(trace.contains(&claim.id.0[..12]), "{trace}");

    let why = cli::context(&dir, &args(&["why", &ctx]), T0 + 2).unwrap();
    for part in [&done.id.0[..12], &claim.id.0[..12], "the ask", "lease-abc"] {
        assert!(why.contains(part), "{part} in {why}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_declared_repo_missing_on_this_node_is_named_in_the_error() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-gone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = cli::context(&dir, &args(&["new", "gone"]), T0).unwrap();
    let ctx = ctx.trim();
    cli::context(
        &dir,
        &args(&[
            "repo",
            "add",
            ctx,
            "ghost",
            "/no/such/dir/anywhere",
            "--node",
            "n1",
        ]),
        T0,
    )
    .unwrap();
    let err = cli::worker_dir(&dir, ctx, Some("ghost"), "n1").unwrap_err();
    assert!(
        err.contains("ghost") && err.contains("/no/such/dir/anywhere"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// With a mirror, a machine that has never held the store opens it and
/// finds what another machine wrote.
// implements: fc473f56b1b8bdbae4a31e54e7281c4aeed3c08bd03804b256ca37e1937fe66f@61632ce8ff602a5a85acd48cc9208e1ca388d7a696ce46d8459e27b85426538e
#[test]
fn a_mirrored_store_opens_on_a_new_machine_with_what_the_other_wrote() {
    let root = std::env::temp_dir().join(format!("kudo-shim-cli-mirror-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let url = format!("file://{}", root.join("bucket").display());

    let pc = cli::open_store(&root.join("pc"), Some(&url), true).unwrap();
    let ctx = pc.create_context("crosses machines", "# x\n", T0).unwrap();

    let edger = cli::open_store(&root.join("edger"), Some(&url), true).unwrap();
    let seen: Vec<_> = edger
        .contexts()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(seen, std::slice::from_ref(&ctx));

    assert!(
        cli::open_store(&root.join("nowhere"), None, true).is_err(),
        "without a mirror, a missing store is an error"
    );
    assert!(
        cli::open_store(&root.join("typo"), Some(&url), false).is_err(),
        "a hook never creates a store, mirror or not"
    );
    assert!(!root.join("typo").exists());
    let _ = std::fs::remove_dir_all(&root);
}

/// `context list` on a machine that never held the store, with a mirror,
/// lists what the other machine created.
#[test]
fn list_on_a_new_machine_with_a_mirror_shows_the_other_machines_contexts() {
    let root =
        std::env::temp_dir().join(format!("kudo-shim-cli-list-mirror-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let url = format!("file://{}", root.join("bucket").display());
    let made = cli::context_with(
        &root.join("pc"),
        Some(&url),
        None,
        &args(&["new", "made on pc"]),
        T0,
    )
    .unwrap();
    let listed =
        cli::context_with(&root.join("edger"), Some(&url), None, &args(&["list"]), T0).unwrap();
    assert!(listed.starts_with(&made.trim()[..12]), "{listed}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_empty_ask_is_refused() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = cli::context(&dir, &args(&["new", "e"]), T0).unwrap();
    assert!(cli::context(&dir, &args(&["enqueue", ctx.trim(), "   "]), T0).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
