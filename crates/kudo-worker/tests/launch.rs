//! The launch plan: the CLI started with the worker's hooks and MCP server on
//! its command line, under the lease's session id, with the kickoff prompt.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af

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
        "10.100.0.1:15400",
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
        "h:1",
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
}

#[test]
fn the_mcp_config_runs_the_shim_with_the_worker_environment() {
    let p = plan(
        &setup("lease-x"),
        "claude",
        Path::new("/bin/kudo-shim"),
        "h:1",
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
        "h:1",
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
