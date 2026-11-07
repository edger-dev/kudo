//! Kimi Code's launch and hook protocol.
//!
//! implements: de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019@de754b32b496489dcc9c584766662cc6149cb90c40e2a5095fa9c4da64e78019
//! task: 49fe78f15b4ec6be28ab78c0e7f8b982b97433bb5320ef8f14616eb1f89b6c2e@f828fec21a1fe625c6d0ac74f9c07b313bf196a8a55c56962df5298161777f58

use std::path::Path;

use serde_json::Value;

use crate::harness::{
    HarnessCapabilities, HookReply, LaunchOptions, ShellCall,
};
use crate::{AgentHarness, LaunchPlan, Stop, WorkerSetup};

/// Kimi Code, using its own configuration and hook semantics.
pub struct KimiCode;

impl AgentHarness for KimiCode {
    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities::default()
    }

    fn launch(
        &self,
        _setup: &WorkerSetup,
        _cli: Option<&str>,
        _shim: &Path,
        _hub_addr: Option<&str>,
        _forward: &[(String, String)],
        options: LaunchOptions<'_>,
    ) -> Result<LaunchPlan, String> {
        Err(format!(
            "Kimi Code does not support {:?} workers yet",
            options.mode
        ))
    }

    fn user_prompt<'a>(&self, _input: &'a Value) -> Option<&'a str> {
        None
    }

    fn shell_call(&self, _input: &Value) -> Option<ShellCall> {
        None
    }

    fn session_start_reply(&self, _context: &str) -> HookReply {
        HookReply::quiet()
    }

    fn stop_reply(&self, _stop: &Stop) -> HookReply {
        HookReply::quiet()
    }

    fn post_tool_reply(&self, _event: &str, _context: &str) -> HookReply {
        HookReply::quiet()
    }
}
