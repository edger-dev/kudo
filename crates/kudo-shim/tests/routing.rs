#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Phase 2 tests — routed calls against a **real** mesh.
//!
//! These run an actual hub and an actual node serving hub's reference files
//! connector, rather than a mock of the far side. The point of the exercise is
//! to prove the shim talks to the thing that really exists; a mock would only
//! prove it talks to our idea of it.
//!
//! The files connector is a **test target only**. No file tool ships on the
//! agent surface — the shim's job is the job substrate, and exposing files here
//! would put a capability on the agent's surface that nothing asked for.
//!
//! implements: 7c16a4ca840ff15005c5e90cca00d3d34e639ee346aadb8807863e860366118b

use std::sync::Arc;
use std::time::Duration;

use hub::{HubListener, ServedHub};
use hub_protocol::identity::{LinkLabel, NodeId};
use hub_protocol::{Connector, ConnectorKind, RoutedCall};
use node::reconnect::{Backoff, DialOutcome};
use node::{Connectors, FilesConnector};

use kudo_shim::hub::HubClient;
use kudo_shim::route::{RouteFailure, Target};

/// A live hub with one node attached, serving a files connector over `root`.
struct Mesh {
    addr: String,
    _hub: tokio::task::JoinHandle<std::io::Result<()>>,
    _node: tokio::task::JoinHandle<()>,
}

async fn mesh(root: &std::path::Path) -> Mesh {
    let hub_state: ServedHub = ServedHub::new();
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    let hub_task = tokio::spawn({
        let hub_state = hub_state.clone();
        async move { listener.serve(hub_state).await }
    });

    let cfg = node::NodeConfig::new(
        NodeId("alpha".to_string()),
        LinkLabel::stable(),
        Some(node::HubEndpoint(format!("tcp://{addr}"))),
        vec![Connector {
            id: "files-0".to_string(),
            kind: ConnectorKind::Files,
        }],
        "test".to_string(),
    )
    .expect("node config");

    let mut connectors = Connectors::new();
    connectors.register(Box::new(FilesConnector::new("files-0", root)));
    let connectors = Arc::new(connectors);

    let node_task = tokio::spawn({
        let cfg = cfg.clone();
        async move {
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
        }
    });

    Mesh {
        addr,
        _hub: hub_task,
        _node: node_task,
    }
}

/// Wait for the node to finish registering, so a test failure means the thing
/// under test broke rather than that we raced startup.
async fn client_with_node(mesh: &Mesh) -> HubClient {
    let client = HubClient::dial(&mesh.addr).await.expect("dial hub");
    for _ in 0..100 {
        if client.topology().await.map(|t| t.nodes.len()).unwrap_or(0) == 1 {
            return client;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the node never registered");
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kudo-shim-routing-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create root");
    dir
}

/// A routed call reaches a real connector and comes back with its bytes.
#[tokio::test(flavor = "multi_thread")]
async fn a_routed_call_reaches_a_real_connector() {
    let root = temp_root("reaches");
    std::fs::write(root.join("hello.txt"), "hi").unwrap();
    let mesh = mesh(&root).await;
    let client = client_with_node(&mesh).await;

    let reply = client
        .route(
            Target::node("alpha"),
            RoutedCall {
                connector: "files-0".to_string(),
                method: "list".to_string(),
                payload: br#"{"path":"."}"#.to_vec(),
            },
        )
        .await
        .expect("the call should reach the connector");

    let body = String::from_utf8_lossy(&reply.payload);
    assert!(
        body.contains("hello.txt"),
        "the connector's own bytes should come back, got: {body}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A link that is not connected falls back to `stable`, so naming an absent link
/// is not an error — the degenerate single-link setup just works.
#[tokio::test(flavor = "multi_thread")]
async fn an_absent_link_falls_back_to_stable() {
    let root = temp_root("fallback");
    std::fs::write(root.join("hello.txt"), "hi").unwrap();
    let mesh = mesh(&root).await;
    let client = client_with_node(&mesh).await;

    let reply = client
        .route(
            // Only `stable` is connected; ask for `dev` anyway.
            Target::link("alpha", "dev"),
            RoutedCall {
                connector: "files-0".to_string(),
                method: "list".to_string(),
                payload: br#"{"path":"."}"#.to_vec(),
            },
        )
        .await
        .expect("an absent link should fall back to stable, not fail");

    assert!(String::from_utf8_lossy(&reply.payload).contains("hello.txt"));

    let _ = std::fs::remove_dir_all(&root);
}

/// **The boundary is preserved.** An unknown node is the hub failing to deliver;
/// an unknown method is the connector refusing. A consumer must be able to tell
/// them apart, because they warrant different reactions — refresh discovery
/// versus surface the refusal and do not retry.
#[tokio::test(flavor = "multi_thread")]
async fn hub_and_node_failures_stay_distinguishable() {
    let root = temp_root("boundary");
    let mesh = mesh(&root).await;
    let client = client_with_node(&mesh).await;

    let call = |connector: &str, method: &str| RoutedCall {
        connector: connector.to_string(),
        method: method.to_string(),
        payload: br#"{"path":"."}"#.to_vec(),
    };

    // No such node: the hub could not deliver.
    let err = client
        .route(Target::node("nowhere"), call("files-0", "list"))
        .await
        .expect_err("an unknown node must fail");
    assert!(
        matches!(err.failure(), Some(RouteFailure::Hub)),
        "an unknown node is a hub-side failure, got: {err}"
    );
    // The message has to carry the distinction too — `failure()` is for code,
    // and the text is what a human or an agent actually reads.
    let message = err.to_string();
    assert!(
        message.contains("nowhere") && message.contains("could not deliver"),
        "a hub-side failure must name the node and say it never arrived, got: {message}"
    );

    // Real node, real connector, method it does not have: the connector refused.
    let err = client
        .route(Target::node("alpha"), call("files-0", "no-such-method"))
        .await
        .expect_err("an unknown method must fail");
    assert!(
        matches!(err.failure(), Some(RouteFailure::Node)),
        "an unknown method is a node-side refusal, got: {err}"
    );
    let message = err.to_string();
    assert!(
        message.contains("files-0") && message.contains("no-such-method"),
        "a node-side refusal must name the connector and the method, got: {message}"
    );
    assert!(
        message.contains("received the call"),
        "and must say it arrived, so nobody retries it as a delivery problem: {message}"
    );

    // Real node, connector it does not offer: still node-side — it reached the
    // node, which is what the distinction is about.
    let err = client
        .route(Target::node("alpha"), call("no-such-connector", "list"))
        .await
        .expect_err("an unknown connector must fail");
    assert!(
        matches!(err.failure(), Some(RouteFailure::Node)),
        "an unknown connector is a node-side refusal, got: {err}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
