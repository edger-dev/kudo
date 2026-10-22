//! The launch plan: the CLI started with the worker's hooks and MCP server on
//! its command line, under the lease's session id, with the kickoff prompt.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::{Path, PathBuf};

use kudo_context::ContextId;
use kudo_worker::{KICKOFF, WorkerSetup, new_lease_id, plan};
use serde_json::Value;

fn setup(lease: &str) -> WorkerSetup {
    WorkerSetup {
        store_dir: PathBuf::from("/state/contexts"),
        context: ContextId("ctx1".into()),
        lease: lease.into(),
        claim_ttl: 600,
        max_turns: 5,
        idle_limit: 0,
    }
}

fn flag<'a>(argv: &'a [String], name: &str) -> &'a str {
    let i = argv
        .iter()
        .position(|a| a == name)
        .unwrap_or_else(|| panic!("{name} in {argv:?}"));
    &argv[i + 1]
}

#[test]
fn the_cli_starts_under_the_lease_with_the_kickoff_prompt() {
    let lease = new_lease_id().unwrap();
    let p = plan(
        &setup(&lease),
        "claude",
        Path::new("/bin/kudo-shim"),
        Some("10.100.0.1:15400"),
        &[],
    );
    assert_eq!(p.argv[0], "claude");
    assert_eq!(flag(&p.argv, "--session-id"), lease);
    assert_eq!(p.argv.last().unwrap(), KICKOFF);
}

#[test]
fn the_settings_install_all_four_hooks_through_the_shim() {
    let p = plan(
        &setup("l"),
        "claude",
        Path::new("/opt/kudo shim/kudo-shim"),
        Some("h:1"),
        &[],
    );
    let settings: Value = serde_json::from_str(flag(&p.argv, "--settings")).unwrap();
    for (event, arg) in [
        ("SessionStart", "session-start"),
        ("Stop", "stop"),
        ("PreCompact", "pre-compact"),
        ("UserPromptSubmit", "user-prompt-submit"),
    ] {
        let command = settings["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(settings["hooks"][event][0]["hooks"][0]["type"], "command");
        // A path with a space survives the shell the CLI runs hooks through.
        assert_eq!(
            command,
            format!("'/opt/kudo shim/kudo-shim' hook {arg}"),
            "{event}"
        );
    }
    // The Stop hook waits for work: its timeout covers the wait window.
    // implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
    let timeout = settings["hooks"]["Stop"][0]["hooks"][0]["timeout"]
        .as_i64()
        .unwrap_or(0);
    assert!(
        timeout >= kudo_worker::env::WAIT_WINDOW + 30,
        "Stop hook timeout {timeout}s"
    );
}

#[test]
fn the_mcp_config_runs_the_shim_with_the_worker_environment() {
    let p = plan(
        &setup("lease-x"),
        "claude",
        Path::new("/bin/kudo-shim"),
        Some("h:1"),
        &[],
    );
    let mcp: Value = serde_json::from_str(flag(&p.argv, "--mcp-config")).unwrap();
    let server = &mcp["mcpServers"]["kudo"];
    assert_eq!(server["command"], "/bin/kudo-shim");
    assert_eq!(server["env"]["KUDO_HUB_ADDR"], "h:1");
    assert_eq!(server["env"]["KUDO_CONTEXT"], "ctx1");
    assert_eq!(server["env"]["KUDO_LEASE"], "lease-x");
}

#[test]
fn the_process_environment_carries_the_setup_for_the_hooks() {
    let p = plan(
        &setup("lease-x"),
        "claude",
        Path::new("/bin/kudo-shim"),
        Some("h:1"),
        &[],
    );
    let env: std::collections::HashMap<_, _> = p.env.into_iter().collect();
    let back = WorkerSetup::from_lookup(|k| env.get(k).cloned()).unwrap();
    assert_eq!(back, setup("lease-x"));
}

#[test]
fn lease_ids_are_distinct_v4_uuids() {
    let a = new_lease_id().unwrap();
    let b = new_lease_id().unwrap();
    assert_ne!(a, b);
    let groups: Vec<&str> = a.split('-').collect();
    assert_eq!(
        groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
        vec![8, 4, 4, 4, 12],
        "{a}"
    );
    assert!(groups[2].starts_with('4'), "version 4: {a}");
    assert!(
        matches!(&groups[3][..1], "8" | "9" | "a" | "b"),
        "RFC 4122 variant: {a}"
    );
    assert!(a.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
}

/// Without a hub the worker still launches: no hub address is passed, and the
/// shim serves the worker tools on its own.
// implements: 482bd420c2a2251c6d5c2fa311e8a14b2bb0898cf84cedfbef88fe6ba8a119ae@482bd420c2a2251c6d5c2fa311e8a14b2bb0898cf84cedfbef88fe6ba8a119ae
#[test]
fn a_worker_launches_without_a_hub() {
    let p = plan(
        &setup("l"),
        "claude",
        Path::new("/bin/kudo-shim"),
        None,
        &[],
    );
    let mcp: Value = serde_json::from_str(flag(&p.argv, "--mcp-config")).unwrap();
    assert!(
        mcp["mcpServers"]["kudo"]["env"]
            .get("KUDO_HUB_ADDR")
            .is_none()
    );
    assert!(p.env.iter().all(|(k, _)| k != "KUDO_HUB_ADDR"));
}

/// The context store's mirror reaches the worker's MCP server and its hooks
/// whatever the agent CLI passes on of its own environment.
#[test]
fn the_mirror_settings_are_forwarded_to_the_shim() {
    let forward = [
        ("KUDO_CONTEXT_MIRROR".to_owned(), "s3://b/p".to_owned()),
        (
            "AWS_ENDPOINT".to_owned(),
            "http://127.0.0.1:9100".to_owned(),
        ),
    ];
    let p = plan(
        &setup("l"),
        "claude",
        Path::new("/bin/kudo-shim"),
        None,
        &forward,
    );
    let mcp: Value = serde_json::from_str(flag(&p.argv, "--mcp-config")).unwrap();
    let env = &mcp["mcpServers"]["kudo"]["env"];
    assert_eq!(env["KUDO_CONTEXT_MIRROR"], "s3://b/p");
    assert_eq!(env["AWS_ENDPOINT"], "http://127.0.0.1:9100");
    assert!(p.env.contains(&forward[0]));
}

/// A node that says so runs its workers' CLI in bypass-permissions mode; any
/// other node, or any other value, leaves the CLI asking.
// implements: 1cc106dd91b0b36d535c15bec7fd446c3a46384519220c689786266e721661c8@e46433161767390901da276f230249c0a35964412899db7ad9bf2335db511061
#[test]
fn a_worker_bypasses_permissions_only_where_its_node_says_so() {
    use kudo_worker::env::{BYPASS_ENV, bypass_from};
    let p = plan(
        &setup("l"),
        "claude",
        Path::new("/bin/kudo-shim"),
        None,
        &[],
    );
    assert!(
        !p.argv.iter().any(|a| a == "--permission-mode"),
        "{:?}",
        p.argv
    );
    let bypassing = p.clone().bypassing();
    assert_eq!(
        flag(&bypassing.argv, "--permission-mode"),
        "bypassPermissions"
    );
    assert_eq!(bypassing.argv[0], "claude");
    assert_eq!(bypassing.argv.last().unwrap(), KICKOFF);

    assert_eq!(BYPASS_ENV, "KUDO_WORKER_BYPASS");
    let given = |v: Option<&str>| {
        bypass_from(|k| (k == BYPASS_ENV).then(|| v.map(str::to_owned)).flatten())
    };
    assert!(given(Some("1")));
    assert!(!given(None));
    assert!(!given(Some("0")));
    assert!(!given(Some("yes")));
}

/// The launch pre-answers the CLI's first-run prompts: project MCP servers in
/// the repository are not offered (the worker's config is strict), and tips
/// and non-essential traffic (promotions) are off.
// implements: 14129413411817d90fddd2bfff863c0f9a05dac9bef672a7aac7270676b2a697
#[test]
fn the_launch_preanswers_first_run_prompts() {
    let p = plan(
        &setup("lease-q"),
        "claude",
        Path::new("/bin/kudo-shim"),
        None,
        &[],
    );
    assert!(
        p.argv.iter().any(|a| a == "--strict-mcp-config"),
        "{:?}",
        p.argv
    );
    let settings: Value = serde_json::from_str(flag(&p.argv, "--settings")).unwrap();
    assert_eq!(settings["spinnerTipsEnabled"], Value::Bool(false));
    assert!(
        p.env
            .iter()
            .any(|(k, v)| k == "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC" && v == "1"),
        "{:?}",
        p.env
    );
    assert_eq!(p.argv.last().unwrap(), KICKOFF, "the kickoff stays last");
}

/// Shell calls are recorded as they return: a PostToolUse hook, for Bash
/// only, through the shim.
// implements: 6061a14e7f9b9eb2c7a3bbc0f5ff4f3778ceb08d71d46f8de4515f62ef3a2a0b
#[test]
fn shell_calls_are_recorded_by_a_post_tool_use_hook() {
    let p = plan(
        &setup("l"),
        "claude",
        Path::new("/bin/kudo-shim"),
        None,
        &[],
    );
    let settings: Value = serde_json::from_str(flag(&p.argv, "--settings")).unwrap();
    let entry = &settings["hooks"]["PostToolUse"][0];
    assert_eq!(entry["matcher"], "Bash");
    assert_eq!(
        entry["hooks"][0]["command"],
        "/bin/kudo-shim hook post-tool-use"
    );
}
