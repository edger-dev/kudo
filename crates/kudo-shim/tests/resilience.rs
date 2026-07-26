#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The shim survives a hub restart, and bounds how long it waits.
//!
//! implements: 1d90deaa708a39eb606d0190ebbbab5169902bb5fb78a8e76acff87904e91414
//! implements: deda3d32832baaaaeb5814a0f1a859f80451d081d5f8e70d1def0a3d989c9dfe

use std::sync::Arc;
use std::time::Duration;

use hub::{HubListener, ServedHub};
use hub_protocol::identity::{LinkLabel, NodeId};
use hub_protocol::{Connector, ConnectorKind};
use moco_job::JobRegistry;
use moco_job::wire::StartRequest;
use node::Connectors;
use node::reconnect::{Backoff, DialOutcome};

use kudo_node::JobConnector;
use kudo_shim::hub::HubClient;
use kudo_shim::jobs::{self, DEFAULT_JOB_CONNECTOR};
use kudo_shim::route::Target;

/// Serve a hub on `addr`, returning the task so a test can drop it.
fn serve_hub(listener: HubListener) -> tokio::task::JoinHandle<std::io::Result<()>> {
    let hub_state: ServedHub = ServedHub::new();
    tokio::spawn(async move { listener.serve(hub_state).await })
}

fn spawn_node(addr: &str, registry: Arc<JobRegistry>) -> tokio::task::JoinHandle<()> {
    let mut connectors = Connectors::new();
    connectors.register(Box::new(JobConnector::new(DEFAULT_JOB_CONNECTOR, registry)));
    let connectors = Arc::new(connectors);

    let cfg = node::NodeConfig::new(
        NodeId("alpha".to_string()),
        LinkLabel::stable(),
        Some(node::HubEndpoint(format!("tcp://{addr}"))),
        vec![Connector {
            id: DEFAULT_JOB_CONNECTOR.to_string(),
            kind: ConnectorKind::ProcessManager,
        }],
        "test".to_string(),
    )
    .expect("node config");

    tokio::spawn(async move {
        node::reconnect::run_with(
            &cfg,
            connectors,
            &Backoff {
                initial: Duration::from_millis(20),
                max: Duration::from_millis(100),
            },
            || {
                let hub = cfg.hub.clone();
                async move { node::tcp::dial(&hub).await.expect("valid address") }
            },
            tokio::time::sleep,
            |_: DialOutcome| true,
        )
        .await;
    })
}

async fn wait_for_node(client: &HubClient) -> bool {
    for _ in 0..100 {
        if client.topology().await.map(|t| t.nodes.len()).unwrap_or(0) == 1 {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// **A hub restart is not a session restart.** The shim re-dials and keeps
/// working, without the agent having to notice or restart anything.
#[tokio::test(flavor = "multi_thread")]
async fn the_shim_survives_a_hub_restart() {
    // Bind once so the port stays ours across the restart.
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    let hub_task = serve_hub(listener);

    let registry = Arc::new(JobRegistry::ungoverned().expect("registry"));
    let node_task = spawn_node(&addr, registry.clone());

    let client = HubClient::dial(&addr).await.expect("dial");
    assert!(wait_for_node(&client).await, "node should register");

    // ── the hub goes away ────────────────────────────────────────────────────
    hub_task.abort();
    node_task.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── and comes back on the same address ───────────────────────────────────
    let listener = HubListener::bind(addr.as_str())
        .await
        .expect("rebind the same address");
    let _hub_task = serve_hub(listener);
    let _node_task = spawn_node(&addr, registry.clone());

    // The *same* client, never re-dialed by the test, must work again.
    let mut recovered = false;
    for _ in 0..100 {
        if client.topology().await.map(|t| t.nodes.len()).unwrap_or(0) == 1 {
            recovered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        recovered,
        "the shim must re-dial a restarted hub on its own"
    );
}

/// A domain refusal is **not** retried — it reached someone and was answered.
/// Retrying it would at best repeat the answer and at worst act twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_domain_refusal_is_not_retried() {
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    let _hub = serve_hub(listener);
    let registry = Arc::new(JobRegistry::ungoverned().expect("registry"));
    let _node = spawn_node(&addr, registry.clone());

    let client = HubClient::dial(&addr).await.expect("dial");
    assert!(wait_for_node(&client).await);

    // An unstartable program: the engine refuses, and that refusal is final.
    let err = jobs::start(
        &client,
        Target::node("alpha"),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["definitely-not-a-real-program-xyz".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
        },
    )
    .await
    .expect_err("must be refused");

    // Exactly one attempt reached the engine — a retry would have made two.
    let listing = jobs::list(&client, Target::node("alpha"), DEFAULT_JOB_CONNECTOR)
        .await
        .expect("list");
    assert!(
        err.to_string()
            .contains("definitely-not-a-real-program-xyz"),
        "got: {err}"
    );
    assert!(
        listing.jobs.len() <= 1,
        "a refusal must not be retried into a second attempt, saw {} jobs",
        listing.jobs.len()
    );
}

/// **A timeout says the work may still be running.** Reporting it as a plain
/// failure is how a caller ends up starting the same work twice.
#[tokio::test(flavor = "multi_thread")]
async fn a_timeout_says_the_work_may_still_be_running() {
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    let _hub = serve_hub(listener);
    let registry = Arc::new(JobRegistry::ungoverned().expect("registry"));
    let _node = spawn_node(&addr, registry.clone());

    // A budget so small that any real round trip exceeds it.
    let client = HubClient::dial(&addr)
        .await
        .expect("dial")
        .with_timeout(Duration::from_millis(1));
    let err = jobs::list(&client, Target::node("alpha"), DEFAULT_JOB_CONNECTOR)
        .await
        .expect_err("a 1ms budget must expire");

    let message = err.to_string();
    assert!(
        message.contains("timeout") || message.contains("did not answer"),
        "must be reported as a timeout, got: {message}"
    );
    assert!(
        message.contains("may still be running"),
        "a timeout must not read as 'nothing happened', got: {message}"
    );
}

/// The bound is configurable, so a caller expecting a slow answer raises it
/// rather than retrying — which is what would duplicate work.
#[tokio::test(flavor = "multi_thread")]
async fn a_raised_budget_lets_a_normal_call_through() {
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    let _hub = serve_hub(listener);
    let registry = Arc::new(JobRegistry::ungoverned().expect("registry"));
    let _node = spawn_node(&addr, registry.clone());

    let client = HubClient::dial(&addr)
        .await
        .expect("dial")
        .with_timeout(Duration::from_secs(10));
    assert!(wait_for_node(&client).await);

    jobs::list(&client, Target::node("alpha"), DEFAULT_JOB_CONNECTOR)
        .await
        .expect("a normal call must complete inside a generous budget");
}
