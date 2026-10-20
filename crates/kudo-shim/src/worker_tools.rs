//! The worker's MCP tools — `context_read`, `pull`, `submit`, `release` — as
//! plain functions over the context store. The server wraps them.

use kudo_context::Now;
use kudo_worker::WorkerSetup;

/// The bound context's head.
pub fn context_read(_setup: &WorkerSetup) -> Result<String, String> {
    Err("not built yet".into())
}

/// Claim the next item under the bound context for this lease.
pub fn pull(_setup: &WorkerSetup, _now: Now) -> Result<String, String> {
    Err("not built yet".into())
}

/// Submit the work for a claim: the full new head and a log entry.
pub fn submit(
    _setup: &WorkerSetup,
    _claim: &str,
    _head: &str,
    _log: &str,
    _now: Now,
) -> Result<String, String> {
    Err("not built yet".into())
}

/// Give a claim back unfinished.
pub fn release(_setup: &WorkerSetup, _claim: &str, _now: Now) -> Result<String, String> {
    Err("not built yet".into())
}
