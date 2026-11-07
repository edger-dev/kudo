//! Kimi Code harness conformance, from real 2.5.2 payloads.
//!
//! implements: de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019@de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::Path;

use kudo_context::ContextId;
use kudo_worker::harness::{HARNESS_ENV, LaunchMode};
use kudo_worker::{
    AgentHarness, KICKOFF, KimiCode, LaunchOptions, Stop, WorkerSetup, harness_from,
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
fn kimi_is_selected_explicitly_and_reports_headless_only_at_this_phase() {
    let harness = harness_from(|key| {
        (key == HARNESS_ENV)
            .then(|| Some("kimi".to_owned()))
            .flatten()
    })
    .unwrap();
    let capabilities = harness.capabilities();
    assert!(!capabilities.interactive);
    assert!(capabilities.headless);
    assert!(!capabilities.caller_session_id);
    assert!(capabilities.config_artifacts);
    assert!(!capabilities.strict_mcp);
    assert!(!capabilities.session_start_context);
    assert!(!capabilities.post_tool_context);
}

#[test]
fn kimi_headless_launch_uses_print_mode_and_a_kudo_plugin() {
    let forward = [
        ("KUDO_CONTEXT_MIRROR".to_owned(), "s3://b/p".to_owned()),
        ("AWS_REGION".to_owned(), "auto".to_owned()),
    ];
    let plan = KimiCode
        .launch(
            &setup(),
            None,
            Path::new("/bin/kudo-shim"),
            Some("hub:15400"),
            &forward,
            LaunchOptions {
                mode: LaunchMode::Headless,
                bypass: true,
                strict_mcp: false,
                repo: Path::new("/tmp"),
            },
        )
        .unwrap();
    assert_eq!(plan.argv[0], "kimi");
    assert!(plan.argv.iter().any(|arg| arg == "-p"));
    assert_eq!(plan.argv.last().unwrap(), KICKOFF);
    assert!(!plan.argv.iter().any(|arg| arg == "--settings"));
    assert!(!plan.argv.iter().any(|arg| arg == "--mcp-config"));
    assert!(!plan.argv.iter().any(|arg| arg == "--session-id"));
    assert!(
        plan.env
            .iter()
            .any(|(key, value)| key == HARNESS_ENV && value == "kimi")
    );

    assert_eq!(plan.plugins.len(), 1);
    let plugin = &plan.plugins[0];
    assert_eq!(plugin.id, "kudo");
    assert_eq!(plugin.files.len(), 1);
    let manifest: Value = serde_json::from_str(&plugin.files[0].contents).unwrap();
    let server = &manifest["mcpServers"]["kudo"];
    assert_eq!(server["command"], "/bin/kudo-shim");
    assert_eq!(server["env"]["KUDO_CONTEXT"], "ctx1");
    assert_eq!(server["env"]["KUDO_LEASE"], "lease-1");
    assert_eq!(server["env"]["KUDO_HUB_ADDR"], "hub:15400");
    assert_eq!(server["env"]["KUDO_CONTEXT_MIRROR"], "s3://b/p");
    assert_eq!(server["env"]["AWS_REGION"], "auto");
    assert_eq!(server["env"][HARNESS_ENV], "kimi");
    for event in [
        "SessionStart",
        "Stop",
        "PreCompact",
        "UserPromptSubmit",
        "PostToolUse",
        "PostToolUseFailure",
    ] {
        assert!(
            manifest["hooks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|hook| hook["event"] == event),
            "missing {event}: {manifest}"
        );
    }
}

#[test]
fn kimi_interactive_and_strict_mcp_are_refused_until_implemented() {
    for (mode, strict_mcp) in [
        (LaunchMode::Interactive, false),
        (LaunchMode::Headless, true),
    ] {
        let error = KimiCode
            .launch(
                &setup(),
                None,
                Path::new("/bin/kudo-shim"),
                None,
                &[],
                LaunchOptions {
                    mode,
                    bypass: false,
                    strict_mcp,
                    repo: Path::new("/tmp"),
                },
            )
            .unwrap_err();
        assert!(!error.is_empty());
    }
}

#[test]
fn kimi_uses_plain_prompt_context_and_exit_two_for_stop() {
    let startup = KimiCode.user_prompt_reply(Some("the head"));
    assert_eq!(startup.stdout, "the head");
    assert_eq!(startup.code, 0);
    assert!(KimiCode.user_prompt_reply(None).stdout.is_empty());
    assert!(KimiCode.session_start_reply("ignored").stdout.is_empty());

    let next = KimiCode.stop_reply(&Stop::Block {
        reason: "next item".into(),
    });
    assert_eq!(next.code, 2);
    assert!(next.stdout.is_empty());
    assert_eq!(KimiCode.stop_reply(&Stop::Wait).code, 0);
}

#[test]
fn kimi_normalizes_content_array_prompts_and_tool_output() {
    let prompt = serde_json::json!({
        "prompt": [{"type": "text", "text": "take over"}]
    });
    assert_eq!(KimiCode.user_prompt(&prompt), Some("take over"));

    let call = KimiCode
        .shell_call(&serde_json::json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "printf PROBE"},
            "tool_output": "PROBE"
        }))
        .unwrap();
    assert_eq!(call.command, "printf PROBE");
    assert_eq!(call.output, "PROBE");
    assert_eq!(call.event, "PostToolUse");

    let failed = KimiCode
        .shell_call(&serde_json::json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_input": {"command": "false"},
            "tool_output": "failed",
            "error": "Exit code 1"
        }))
        .unwrap();
    assert_eq!(failed.output, "failed\n[error]\nExit code 1");
}
