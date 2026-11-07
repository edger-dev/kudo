//! The boundary between the worker engine and one agent CLI.
//!
//! implements: de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019@de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019
//! task: 49fe78f15b4ec6be28ab78c0e7f8b982b97433bb5320ef8f14616eb1f89b6c2e@f828fec21a1fe625c6d0ac74f9c07b313bf196a8a55c56962df5298161777f58

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{LaunchPlan, Stop, WorkerSetup};

/// The environment variable selecting the agent harness.
pub const HARNESS_ENV: &str = "KUDO_AGENT_HARNESS";

/// What a harness can provide without imitating a different CLI.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HarnessCapabilities {
    pub interactive: bool,
    pub headless: bool,
    pub caller_session_id: bool,
    pub config_artifacts: bool,
    pub strict_mcp: bool,
    pub session_start_context: bool,
    pub post_tool_context: bool,
}

/// One configuration file a harness needs beside its launch command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchFile {
    pub path: PathBuf,
    pub contents: String,
}

impl LaunchFile {
    pub fn new(path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            contents: contents.into(),
        }
    }
}

/// The node and repository choices applied to a launch.
#[derive(Debug, Clone, Copy)]
pub struct LaunchOptions<'a> {
    pub bypass: bool,
    pub strict_mcp: bool,
    pub repo: &'a Path,
}

/// What a hook prints and how it exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReply {
    pub stdout: String,
    pub code: i32,
}

impl HookReply {
    pub fn quiet() -> Self {
        Self {
            stdout: String::new(),
            code: 0,
        }
    }

    pub fn json(value: &Value) -> Self {
        Self {
            stdout: value.to_string(),
            code: 0,
        }
    }

    /// A failure the CLI shows the human (exit code 1 is non-blocking).
    pub fn failed(message: impl std::fmt::Display) -> Self {
        Self {
            stdout: String::new(),
            code: 1,
        }
        .with_stderr(message)
    }

    pub fn with_stderr(self, message: impl std::fmt::Display) -> Self {
        eprintln!("kudo-shim hook: {message}");
        self
    }
}

/// CLI-specific launch and hook behavior, kept out of the worker engine.
pub trait AgentHarness {
    fn capabilities(&self) -> HarnessCapabilities;

    fn launch(
        &self,
        setup: &WorkerSetup,
        cli: &str,
        shim: &Path,
        hub_addr: Option<&str>,
        forward: &[(String, String)],
        options: LaunchOptions<'_>,
    ) -> LaunchPlan;

    fn session_start_reply(&self, context: &str) -> HookReply;

    fn stop_reply(&self, stop: &Stop) -> HookReply;

    fn post_tool_reply(&self, event: &str, context: &str) -> HookReply;
}

/// Claude Code's launch and hook protocol.
pub struct ClaudeCode;

impl AgentHarness for ClaudeCode {
    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities {
            interactive: true,
            headless: true,
            caller_session_id: true,
            config_artifacts: false,
            strict_mcp: true,
            session_start_context: true,
            post_tool_context: true,
        }
    }

    fn launch(
        &self,
        setup: &WorkerSetup,
        cli: &str,
        shim: &Path,
        hub_addr: Option<&str>,
        forward: &[(String, String)],
        options: LaunchOptions<'_>,
    ) -> LaunchPlan {
        let mut plan = crate::launch::plan(setup, cli, shim, hub_addr, forward);
        if options.bypass {
            plan = plan.bypassing();
        }
        plan.env.push((HARNESS_ENV.to_owned(), "claude".to_owned()));
        plan.for_node_mcp(options.strict_mcp, options.repo)
    }

    fn session_start_reply(&self, context: &str) -> HookReply {
        HookReply::json(&serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": context,
            }
        }))
    }

    fn stop_reply(&self, stop: &Stop) -> HookReply {
        match stop {
            Stop::Block { reason } => HookReply::json(&serde_json::json!({
                "decision": "block",
                "reason": reason,
            })),
            Stop::Allow(_) | Stop::Wait => HookReply::quiet(),
        }
    }

    fn post_tool_reply(&self, event: &str, context: &str) -> HookReply {
        HookReply::json(&serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": event,
                "additionalContext": context,
            }
        }))
    }
}

/// Select a harness explicitly; an absent choice preserves Claude as the
/// default during migration.
pub fn harness_from(
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Box<dyn AgentHarness>, String> {
    match lookup(HARNESS_ENV).as_deref() {
        None | Some("claude") => Ok(Box::new(ClaudeCode)),
        Some(other) => Err(format!(
            "unknown agent harness {other:?}: supported harnesses are claude"
        )),
    }
}
