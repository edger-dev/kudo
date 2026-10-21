//! The environment a launcher hands a worker session.
//!
//! The launcher sets these on the agent CLI's process; its hooks and its MCP
//! server are children of that process, so all of them read the same values —
//! no session id needs to travel between them.

use std::path::PathBuf;

use kudo_context::ContextId;

/// The context store's directory.
pub const STORE_ENV: &str = "KUDO_CONTEXT_STORE";
/// The context store's mirror, as a URL (`s3://bucket/prefix`, `file:///dir`).
pub const MIRROR_ENV: &str = "KUDO_CONTEXT_MIRROR";
/// The bound context's id.
pub const CONTEXT_ENV: &str = "KUDO_CONTEXT";
/// The lease id (also the CLI's session id).
pub const LEASE_ENV: &str = "KUDO_LEASE";
/// A lease a launcher picked for the worker it starts (the console, to record
/// the pane). Its own variable: every shell inside a worker session inherits
/// that session's KUDO_LEASE, which a nested launch must never reuse.
pub const GIVEN_LEASE_ENV: &str = "KUDO_GIVEN_LEASE";
/// Seconds a claim stays live without a submit.
pub const CLAIM_TTL_ENV: &str = "KUDO_CLAIM_TTL";
/// Items the Stop hook may hand over before the lease ends.
pub const MAX_TURNS_ENV: &str = "KUDO_MAX_TURNS";

/// Set to `1` in a node's own environment: its workers run the agent CLI in
/// bypass-permissions mode. The node's opt-in; the console never sends it (on
/// an ungoverned node a start request's env could — see the spec).
// implements: 1cc106dd91b0b36d535c15bec7fd446c3a46384519220c689786266e721661c8@e46433161767390901da276f230249c0a35964412899db7ad9bf2335db511061
pub const BYPASS_ENV: &str = "KUDO_WORKER_BYPASS";

/// Whether this node's workers bypass permissions: only `1` says so.
pub fn bypass_from(lookup: impl Fn(&str) -> Option<String>) -> bool {
    lookup(BYPASS_ENV).is_some_and(|v| v.trim() == "1")
}

/// Seconds a lease may wait in a row with nothing handed over before it
/// ends; `0` ends it as soon as nothing is ready.
// implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@a7051e3e59de5e20f03e2117248f5af341ed55a104af661aca2026f28c4abc99
pub const IDLE_LIMIT_ENV: &str = "KUDO_IDLE_LIMIT";
pub const DEFAULT_IDLE_LIMIT: i64 = 7200;
/// Seconds one Stop hook call waits for work: inside the hook's timeout.
pub const WAIT_WINDOW: i64 = 540;
/// Seconds between looks at the store while waiting.
pub const WAIT_POLL: u64 = 3;
/// Seconds between mirror syncs while waiting: the local looks stay cheap,
/// and an item queued on another machine arrives within this.
pub const WAIT_SYNC: u64 = 30;

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
    pub idle_limit: i64,
}

impl WorkerSetup {
    /// Read a setup through `lookup` (e.g. `std::env::var(k).ok()`); `None`
    /// when the session is not a worker.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Option<Self> {
        Some(Self {
            store_dir: PathBuf::from(lookup(STORE_ENV)?),
            context: ContextId(lookup(CONTEXT_ENV)?),
            lease: lookup(LEASE_ENV)?,
            claim_ttl: lookup(CLAIM_TTL_ENV)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_CLAIM_TTL),
            max_turns: lookup(MAX_TURNS_ENV)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_MAX_TURNS),
            idle_limit: lookup(IDLE_LIMIT_ENV)
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_IDLE_LIMIT),
        })
    }

    /// Read a setup from this process's environment.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// The variables to set on the agent CLI's process.
    pub fn to_env(&self) -> Vec<(String, String)> {
        [
            (STORE_ENV, self.store_dir.display().to_string()),
            (CONTEXT_ENV, self.context.0.clone()),
            (LEASE_ENV, self.lease.clone()),
            (CLAIM_TTL_ENV, self.claim_ttl.to_string()),
            (MAX_TURNS_ENV, self.max_turns.to_string()),
            (IDLE_LIMIT_ENV, self.idle_limit.to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
    }

    /// The lease records' directory, beside the store.
    pub fn leases_dir(&self) -> PathBuf {
        self.store_dir.join("leases")
    }

    /// The worker config this setup describes.
    pub fn config(&self) -> crate::WorkerConfig {
        crate::WorkerConfig {
            context: self.context.clone(),
            lease: self.lease.clone(),
            claim_ttl: self.claim_ttl,
            max_turns: self.max_turns,
            idle_limit: self.idle_limit,
        }
    }
}
