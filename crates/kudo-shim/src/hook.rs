//! `kudo-shim hook <event>`: the agent CLI's hooks, answered from the context
//! store with no hub and no node on the path.
//!
//! The CLI runs a hook as a command, feeds it the event as JSON on stdin and
//! reads its decision from stdout. Each event maps to one worker decision.

use kudo_context::Now;
use kudo_worker::WorkerSetup;

/// What a hook prints and how it exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReply {
    pub stdout: String,
    pub code: i32,
}

/// Answer one hook event (`session-start`, `stop`, `pre-compact`,
/// `user-prompt-submit`) for the worker `setup` describes.
pub fn run(_event: &str, _stdin: &str, _setup: &WorkerSetup, _now: Now) -> HookReply {
    HookReply {
        stdout: String::new(),
        code: 0,
    }
}
