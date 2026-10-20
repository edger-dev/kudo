//! How a worker session is started: the agent CLI with the worker's hooks and
//! the kudo MCP server passed on the command line, so a repo needs no setup of
//! its own and the repo's own settings are never touched.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af

use std::io::Read;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::{KICKOFF, WorkerSetup};

/// The command line and environment that start one worker session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The hooks a worker installs, as (CLI event name, `kudo-shim hook` argument).
const HOOKS: [(&str, &str); 4] = [
    ("SessionStart", "session-start"),
    ("Stop", "stop"),
    ("PreCompact", "pre-compact"),
    ("UserPromptSubmit", "user-prompt-submit"),
];

/// Plan a worker session: `cli` is the agent CLI, `shim` the kudo-shim binary
/// the hooks and the MCP server run as, `hub_addr` the hub the MCP server dials.
pub fn plan(setup: &WorkerSetup, cli: &str, shim: &Path, hub_addr: &str) -> LaunchPlan {
    let shim_str = shim.display().to_string();

    let hooks: Map<String, Value> = HOOKS
        .iter()
        .map(|(event, arg)| {
            let command = format!("{} hook {arg}", shell_quote(&shim_str));
            (
                (*event).to_owned(),
                json!([{ "hooks": [{ "type": "command", "command": command }] }]),
            )
        })
        .collect();
    let settings = json!({ "hooks": hooks });

    let mut server_env: Map<String, Value> = setup
        .to_env()
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();
    server_env.insert(
        "KUDO_HUB_ADDR".to_owned(),
        Value::String(hub_addr.to_owned()),
    );
    let mcp = json!({
        "mcpServers": { "kudo": { "command": shim_str, "args": [], "env": server_env } }
    });

    let mut env = setup.to_env();
    env.push(("KUDO_HUB_ADDR".to_owned(), hub_addr.to_owned()));

    LaunchPlan {
        argv: vec![
            cli.to_owned(),
            "--settings".to_owned(),
            settings.to_string(),
            "--mcp-config".to_owned(),
            mcp.to_string(),
            "--session-id".to_owned(),
            setup.lease.clone(),
            KICKOFF.to_owned(),
        ],
        env,
    }
}

/// Quote for the POSIX shell the CLI runs hook commands through: bare when
/// safe, single-quoted otherwise.
fn shell_quote(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "/._-+:=@".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// A fresh lease id: a random UUID (v4), since the CLI takes it as its session id.
pub fn new_lease_id() -> std::io::Result<String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}
