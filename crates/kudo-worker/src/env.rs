//! The environment a launcher hands a worker session.
//!
//! The launcher sets these on the agent CLI's process; its hooks and its MCP
//! server are children of that process, so all of them read the same values —
//! no session id needs to travel between them.

use std::path::PathBuf;

use kudo_context::ContextId;

/// The context store's directory.
pub const STORE_ENV: &str = "KUDO_CONTEXT_STORE";
/// The bound context's id.
pub const CONTEXT_ENV: &str = "KUDO_CONTEXT";
/// The lease id (also the CLI's session id).
pub const LEASE_ENV: &str = "KUDO_LEASE";
/// Seconds a claim stays live without a submit.
pub const CLAIM_TTL_ENV: &str = "KUDO_CLAIM_TTL";
/// Items the Stop hook may hand over before the lease ends.
pub const MAX_TURNS_ENV: &str = "KUDO_MAX_TURNS";

pub const DEFAULT_CLAIM_TTL: i64 = 1800;
pub const DEFAULT_MAX_TURNS: u32 = 20;

/// The first prompt the launcher sends. UserPromptSubmit sees it like any
/// prompt, and must not mistake it for a human taking over.
pub const KICKOFF: &str = "kudo: you are a worker; begin.";

/// Everything a worker session needs to find its context and its lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerSetup {
    pub store_dir: PathBuf,
    pub context: ContextId,
    pub lease: String,
    pub claim_ttl: i64,
    pub max_turns: u32,
}

impl WorkerSetup {
    /// Read a setup through `lookup` (e.g. `std::env::var(k).ok()`); `None`
    /// when the session is not a worker.
    pub fn from_lookup(_lookup: impl Fn(&str) -> Option<String>) -> Option<Self> {
        None
    }

    /// Read a setup from this process's environment.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// The variables to set on the agent CLI's process.
    pub fn to_env(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}
