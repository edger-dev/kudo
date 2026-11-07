//! Conformance for the harness boundary and Claude's existing protocol.
//!
//! implements: de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019@de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::Path;

use kudo_context::ContextId;
use kudo_worker::harness::{HARNESS_ENV, HookReply, LaunchMode};
use kudo_worker::{
    AgentHarness, ClaudeCode, KICKOFF, LaunchOptions, Stop, WorkerSetup, harness_from,
};
use serde_json::Value;

fn setup() -> WorkerSetup {
    WorkerSetup {
        store_dir: "/state/contexts".into(),
        context: ContextId("ctx1".into()),
        lease: "lease-1".into(),
        claim_ttl: 600,
        max_turns: 5,
        idle_limit: 0,
    }
}

#[test]
fn claude_is_the_default_and_reports_its_actual_capabilities() {
    let harness = harness_from(|_| None).unwrap();
    let capabilities = harness.capabilities();
    assert!(capabilities.interactive);
    assert!(capabilities.headless);
    assert!(capabilities.caller_session_id);
    assert!(!capabilities.config_artifacts);
    assert!(capabilities.strict_mcp);
    assert!(capabilities.session_start_context);
    assert!(capabilities.post_tool_context);
}

#[test]
fn an_unknown_harness_is_refused_instead_of_substituting_claude() {
    let error = match harness_from(|key| {
        (key == HARNESS_ENV)
            .then(|| Some("opencode".to_owned()))
            .flatten()
    }) {
        Ok(_) => panic!("an unknown harness launched"),
        Err(error) => error,
    };
    assert!(error.contains("opencode"), "{error}");
}

#[test]
fn claude_launch_through_the_boundary_preserves_the_existing_plan() {
    let forward = [("KUDO_CONTEXT_MIRROR".to_owned(), "s3://b/p".to_owned())];
    let options = LaunchOptions {
        mode: LaunchMode::Interactive,
        bypass: true,
        strict_mcp: false,
        repo: Path::new("/tmp"),
    };
    let expected = kudo_worker::plan(
        &setup(),
        "claude",
        Path::new("/bin/kudo-shim"),
        Some("hub:15400"),
        &forward,
    )
    .bypassing()
    .for_node_mcp(false, Path::new("/tmp"));
    let plan = ClaudeCode
        .launch(
            &setup(),
            Some("claude"),
            Path::new("/bin/kudo-shim"),
            Some("hub:15400"),
            &forward,
            options,
        )
        .unwrap();
    assert_eq!(plan.argv, expected.argv);
    assert_eq!(plan.files, expected.files);
    let expected_env_len = expected.env.len() + 1;
    for entry in expected.env {
        assert!(plan.env.contains(&entry), "{entry:?} missing");
    }
    assert_eq!(plan.env.len(), expected_env_len, "{:?}", plan.env);
    assert_eq!(plan.argv.last().unwrap(), KICKOFF);
    assert!(
        plan.env
            .iter()
            .any(|(key, value)| key == HARNESS_ENV && value == "claude"),
        "{:?}",
        plan.env
    );

    let strict = ClaudeCode
        .launch(
            &setup(),
            None,
            Path::new("/bin/kudo-shim"),
            Some("hub:15400"),
            &forward,
            LaunchOptions {
                strict_mcp: true,
                ..options
            },
        )
        .unwrap();
    assert_eq!(strict.argv[0], "claude");
    assert!(strict.argv.iter().any(|arg| arg == "--strict-mcp-config"));
}

#[test]
fn claude_hook_replies_preserve_the_existing_wire_protocol() {
    let session = ClaudeCode.session_start_reply("the head");
    let session: Value = serde_json::from_str(&session.stdout).unwrap();
    assert_eq!(
        session["hookSpecificOutput"]["additionalContext"],
        "the head"
    );

    let next = ClaudeCode.stop_reply(&Stop::Block {
        reason: "next item".into(),
    });
    let next: Value = serde_json::from_str(&next.stdout).unwrap();
    assert_eq!(next["decision"], "block");
    assert_eq!(next["reason"], "next item");
    assert_eq!(
        ClaudeCode.stop_reply(&Stop::Allow(kudo_worker::LeaseEnd::QueueEmpty)),
        HookReply::quiet()
    );
    assert_eq!(ClaudeCode.stop_reply(&Stop::Wait), HookReply::quiet());

    let evidence = ClaudeCode.post_tool_reply("PostToolUse", "cite kino://abc");
    let evidence: Value = serde_json::from_str(&evidence.stdout).unwrap();
    assert_eq!(
        evidence["hookSpecificOutput"]["hookEventName"],
        "PostToolUse"
    );
    assert_eq!(
        evidence["hookSpecificOutput"]["additionalContext"],
        "cite kino://abc"
    );
}

#[test]
fn claude_normalizes_prompts_and_shell_payloads_inside_the_adapter() {
    let prompt = serde_json::json!({"prompt": "take over"});
    assert_eq!(ClaudeCode.user_prompt(&prompt), Some("take over"));

    let success = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "uptime"},
        "tool_response": {"stdout": "up", "stderr": "warning", "exit_code": 0}
    });
    let call = ClaudeCode.shell_call(&success).unwrap();
    assert_eq!(call.event, "PostToolUse");
    assert_eq!(call.command, "uptime");
    assert_eq!(call.output, "up\n[stderr]\nwarning");
    assert_eq!(call.exit, Some(0));

    let failure = serde_json::json!({
        "hook_event_name": "PostToolUseFailure",
        "tool_name": "Bash",
        "tool_input": {"command": "false"},
        "tool_response": "no output",
        "error": "Exit code 1"
    });
    let call = ClaudeCode.shell_call(&failure).unwrap();
    assert_eq!(call.event, "PostToolUseFailure");
    assert_eq!(call.output, "no output\n[error]\nExit code 1");

    let read = serde_json::json!({"tool_name": "Read"});
    assert!(ClaudeCode.shell_call(&read).is_none());
}
