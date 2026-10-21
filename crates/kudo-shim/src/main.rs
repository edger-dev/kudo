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

use kudo_shim::startup::{Startup, after_dial_failure, plan_startup};
use kudo_shim::{HubClient, ShimServer, call_timeout_from_env, hub_addr_from_env};

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

    // The local commands: drive a context by hand, start a worker here.
    if let Some(cmd @ ("context" | "worker")) = args.get(1).map(String::as_str) {
        let Some(store) = kudo_shim::cli::default_store_dir() else {
            eprintln!("cannot find a place for the context store: set KUDO_CONTEXT_STORE");
            std::process::exit(2);
        };
        if cmd == "worker" {
            let (ctx, repo) = match &args[2..] {
                [ctx] => (ctx, None),
                [ctx, flag, repo] if flag == "--repo" => (ctx, Some(repo.as_str())),
                _ => {
                    eprintln!("usage: kudo-shim worker <context> [--repo <name>]");
                    std::process::exit(2);
                }
            };
            eprintln!("{}", kudo_shim::cli::exec_worker(&store, ctx, repo));
            std::process::exit(1);
        }
        match kudo_shim::cli::context(&store, &args[2..], unix_now()) {
            Ok(text) => print!("{text}"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
        return Ok(());
    }

    // Diagnostics go to stderr: stdout *is* the MCP transport, so a stray line
    // there corrupts the protocol rather than informing anyone.
    //
    // A worker session needs the context store, not the hub: it starts without
    // one (its job tools say why). A session that is only job tools needs the
    // hub, and refuses to start rather than look healthy while failing every
    // call.
    let worker = WorkerSetup::from_env();
    let server = match plan_startup(hub_addr_from_env(), worker.is_some()) {
        Startup::Dial(addr) => match HubClient::dial(&addr).await {
            Ok(hub) => ShimServer::new(Arc::new(match call_timeout_from_env() {
                Some(timeout) => hub.with_timeout(timeout),
                None => hub,
            })),
            Err(e) => match after_dial_failure(&e, worker.is_some()) {
                Startup::WithoutHub(why) => {
                    eprintln!("kudo-shim: {why}; serving the worker tools without it");
                    ShimServer::without_hub(why)
                }
                _ => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            },
        },
        Startup::WithoutHub(why) => ShimServer::without_hub(why),
        Startup::Refuse(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };

    let (stdin, stdout) = stdio();
    let service = server.with_worker(worker).serve((stdin, stdout)).await?;
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
    let reply = kudo_shim::hook::run(event, &stdin, &setup, unix_now());
    if !reply.stdout.is_empty() {
        let mut out = std::io::stdout();
        let _ = out
            .write_all(reply.stdout.as_bytes())
            .and_then(|()| out.flush());
    }
    reply.code
}

/// Seconds since the Unix epoch.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
