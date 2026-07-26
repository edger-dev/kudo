//! The kudo node daemon: offer the job substrate to the mesh.
//!
//! This is what a composition root is *for* — a runnable artifact assembled out
//! of a transport and an engine, neither of which is allowed to know about the
//! other. It reverse-dials the hub and reconnects forever, exactly as any node
//! does; the only thing that makes it *this* node is the connector it registers.
//!
//! Reads `NODE_ID`, `NODE_LINK` (default `stable`), `HUB_ADDR` (or argv[1]), and
//! `KUDO_JOB_CONNECTOR` (default `jobs-0`).

use std::sync::Arc;
use std::time::Duration;

use hub_protocol::{Connector as ConnectorDescriptor, ConnectorKind, LinkLabel, NodeId};
use moco_job::JobRegistry;
use node::reconnect::{Backoff, DialOutcome};
use node::{Connectors, HubEndpoint, NodeConfig};

use kudo_node::JobConnector;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let node = std::env::var("NODE_ID").unwrap_or_else(|_| {
        std::fs::read_to_string("/etc/hostname")
            .map(|h| h.trim().to_string())
            .unwrap_or_else(|_| "unnamed".to_string())
    });
    let link = std::env::var("NODE_LINK").unwrap_or_else(|_| LinkLabel::STABLE.to_string());
    let connector_id = std::env::var("KUDO_JOB_CONNECTOR").unwrap_or_else(|_| "jobs-0".to_string());

    let hub = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("HUB_ADDR").ok())
        .map(HubEndpoint);

    let config = NodeConfig::new(
        NodeId(node),
        LinkLabel(link),
        hub,
        vec![ConnectorDescriptor {
            id: connector_id.clone(),
            kind: ConnectorKind::ProcessManager,
        }],
        env!("CARGO_PKG_VERSION").to_string(),
    )?;
    // Fail fast on an address that could never be dialed, rather than retrying
    // forever against a typo.
    config.hub.dial_target()?;

    // The registry is created here and lives as long as the daemon. Jobs
    // outlive any one client, which is the property the whole substrate exists
    // to provide — so nothing here tears them down.
    let registry = Arc::new(JobRegistry::ungoverned()?);
    let mut connectors = Connectors::new();
    connectors.register(Box::new(JobConnector::new(
        connector_id.clone(),
        registry.clone(),
    )));
    let connectors = Arc::new(connectors);

    println!(
        "kudo-node {:?} link {:?} dialing hub at {} — job connector `{connector_id}`, state in {}",
        config.node,
        config.link,
        config.hub.0,
        registry.dir().display()
    );

    node::reconnect::run_with(
        &config,
        connectors,
        &Backoff {
            initial: Duration::from_millis(200),
            max: Duration::from_secs(5),
        },
        || {
            let hub = config.hub.clone();
            // The address was validated at startup, so this cannot fail on a
            // malformed target. Panicking anyway would take the daemon — and
            // every job it owns — down over a transient dial problem, which is
            // the opposite of what a supervisor is for.
            async move {
                match node::tcp::dial(&hub).await {
                    Ok(link) => link,
                    Err(e) => {
                        eprintln!("dial target became invalid: {e}");
                        std::process::exit(1)
                    }
                }
            }
        },
        tokio::time::sleep,
        |_: DialOutcome| true,
    )
    .await;

    Ok(())
}
