//! The agent shim binary: an MCP server on stdio.
//!
//! Launched as a child of the agent session, so its **working directory is the
//! session's project directory** — which is what makes workspace identity free
//! and unconfigurable-by-mistake. Nothing here computes what that directory
//! *means*; turning a cwd into a workspace is the engine's rule, and a second
//! face would need the same one.
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

use std::sync::Arc;

use rmcp::ServiceExt;
use rmcp::transport::io::stdio;

use kudo_worker::WorkerSetup;

use kudo_shim::{HUB_ADDR_ENV, HubClient, ShimServer, call_timeout_from_env, hub_addr_from_env};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `kudo-shim hook <event>`: the CLI's hooks. Answered from the context
    // store before anything dials the hub — a worker's loop must survive the
    // hub and the node restarting.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("hook") {
        std::process::exit(run_hook(
            args.get(2).map(String::as_str).unwrap_or_default(),
        ));
    }

    // Diagnostics go to stderr: stdout *is* the MCP transport, so a stray line
    // there corrupts the protocol rather than informing anyone.
    let Some(addr) = hub_addr_from_env() else {
        eprintln!(
            "{HUB_ADDR_ENV} is not set. Every node is reached through the hub and \
             none has a local listener, so there is no default to fall back to — \
             set {HUB_ADDR_ENV} to the hub's address (e.g. 127.0.0.1:15400)."
        );
        std::process::exit(2);
    };

    // Dial before serving. A hub that is not there is a startup failure, not a
    // degraded mode: with no node-local path, a shim that started anyway would
    // answer every tool call with the same failure while looking healthy.
    let hub = match HubClient::dial(&addr).await {
        Ok(hub) => Arc::new(match call_timeout_from_env() {
            Some(timeout) => hub.with_timeout(timeout),
            None => hub,
        }),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let (stdin, stdout) = stdio();
    let service = ShimServer::new(hub)
        .with_worker(WorkerSetup::from_env())
        .serve((stdin, stdout))
        .await?;
    service.waiting().await?;
    Ok(())
}

/// Run one hook: the event JSON on stdin, the decision on stdout.
fn run_hook(event: &str) -> i32 {
    use std::io::{Read, Write};

    let Some(setup) = WorkerSetup::from_env() else {
        // Not a worker session: a hook installed here has nothing to decide.
        return 0;
    };
    let mut stdin = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut stdin) {
        eprintln!("kudo-shim hook: reading the event: {e}");
        return 1;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    let reply = kudo_shim::hook::run(event, &stdin, &setup, now);
    if !reply.stdout.is_empty() {
        let mut out = std::io::stdout();
        let _ = out
            .write_all(reply.stdout.as_bytes())
            .and_then(|()| out.flush());
    }
    reply.code
}
