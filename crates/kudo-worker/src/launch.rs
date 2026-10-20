//! How a worker session is started: the agent CLI with the worker's hooks and
//! the kudo MCP server passed on the command line, so a repo needs no setup of
//! its own.

use std::path::Path;

use crate::WorkerSetup;

/// The command line and environment that start one worker session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Plan a worker session: `cli` is the agent CLI, `shim` the kudo-shim binary
/// the hooks and the MCP server run as, `hub_addr` the hub the MCP server dials.
pub fn plan(_setup: &WorkerSetup, _cli: &str, _shim: &Path, _hub_addr: &str) -> LaunchPlan {
    LaunchPlan {
        argv: Vec::new(),
        env: Vec::new(),
    }
}

/// A fresh lease id: a random UUID (v4), since the CLI takes it as its session id.
pub fn new_lease_id() -> std::io::Result<String> {
    Ok(String::new())
}
