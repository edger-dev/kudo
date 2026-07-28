#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The whole path, for real: a consumer routes through the hub, the node
//! dispatches to the job connector, and moco's engine runs an actual process.
//!
//! This is the test the composition root exists to make possible. Every piece is
//! the real one — a real hub, a real node, a real registry spawning a real
//! child — because the interesting failures live in the seams between them, and
//! a mock of any layer would hide exactly those.
//!
//! implements: kudo:7ea24d8f08c2e3e9c4502030a86fd9d5f9e36c1067d7e2d034d2796177e719ec

use std::sync::Arc;
use std::time::Duration;

use hub::{HubListener, ServedHub};
use hub_protocol::identity::{LinkLabel, NodeId};
use hub_protocol::service::ConsumerApiClient;
use hub_protocol::{Connector, ConnectorKind, LinkSelector, RoutedCall};
use moco_job::JobRegistry;
use moco_job::wire::{self, StartReply, StartRequest, WaitReply, WaitRequest};
use node::Connectors;
use node::reconnect::{Backoff, DialOutcome};

use kudo_node::JobConnector;

async fn consumer_for(addr: &str) -> ConsumerApiClient {
    let socket = tokio::net::TcpStream::connect(addr).await.expect("connect");
    vox::initiator_on(vox::transport::tcp::StreamLink::tcp(socket))
        .establish_connection()
        .await
        .expect("establish")
        .open_lane()
        .await
        .expect("open lane")
}

/// A job started through the hub really runs, and its outcome comes back.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_runs_through_the_whole_mesh() {
    let hub_state: ServedHub = ServedHub::new();
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    let serving = tokio::spawn({
        let hub_state = hub_state.clone();
        async move { listener.serve(hub_state).await }
    });

    let registry = Arc::new(JobRegistry::ungoverned().expect("registry"));
    let mut connectors = Connectors::new();
    connectors.register(Box::new(JobConnector::new("jobs-0", registry.clone())));
    let connectors = Arc::new(connectors);

    let cfg = node::NodeConfig::new(
        NodeId("alpha".to_string()),
        LinkLabel::stable(),
        Some(node::HubEndpoint(format!("tcp://{addr}"))),
        vec![Connector {
            id: "jobs-0".to_string(),
            kind: ConnectorKind::ProcessManager,
        }],
        "test".to_string(),
    )
    .expect("node config");

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

    let consumer = consumer_for(&addr).await;
    for _ in 0..100 {
        if consumer.topology().await.expect("topology").nodes.len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // ── start a real process, through the hub ────────────────────────────────
    let payload = wire::encode(&StartRequest {
        argv: vec!["echo".into(), "through-the-mesh".into()],
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        deadline_ms: 0,
        caller: moco_job::wire::WireCaller::Console,
    })
    .expect("encode start");

    let reply = consumer
        .route(
            LinkSelector::node(NodeId("alpha".to_string())),
            RoutedCall {
                connector: "jobs-0".to_string(),
                method: "start".to_string(),
                payload,
            },
        )
        .await
        .expect("start should route to the job connector");
    let started: StartReply = wire::decode(&reply.payload).expect("decode start reply");
    assert!(!started.id.is_empty());

    // ── and wait for it, through the hub ─────────────────────────────────────
    let payload = wire::encode(&WaitRequest {
        id: started.id.clone(),
    })
    .expect("encode wait");
    let reply = consumer
        .route(
            LinkSelector::node(NodeId("alpha".to_string())),
            RoutedCall {
                connector: "jobs-0".to_string(),
                method: "wait".to_string(),
                payload,
            },
        )
        .await
        .expect("wait should route");
    let outcome: WaitReply = wire::decode(&reply.payload).expect("decode wait reply");
    assert_eq!(outcome.status, moco_job::JobStatus::Done { code: 0 });

    // ── the engine really ran it: the output is there ────────────────────────
    let tail = registry
        .tail(&moco_job::JobId(started.id), 0)
        .expect("tail locally");
    assert_eq!(
        String::from_utf8_lossy(&tail.bytes).trim(),
        "through-the-mesh",
        "a real process must have run and written its output"
    );

    node_task.abort();
    serving.abort();
}

/// An unknown method is refused **by the engine**, and the refusal survives the
/// trip back as a node-side rejection naming the method.
#[tokio::test(flavor = "multi_thread")]
async fn an_engine_refusal_survives_the_round_trip() {
    let hub_state: ServedHub = ServedHub::new();
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    let serving = tokio::spawn({
        let hub_state = hub_state.clone();
        async move { listener.serve(hub_state).await }
    });

    let registry = Arc::new(JobRegistry::ungoverned().expect("registry"));
    let mut connectors = Connectors::new();
    connectors.register(Box::new(JobConnector::new("jobs-0", registry)));
    let connectors = Arc::new(connectors);

    let cfg = node::NodeConfig::new(
        NodeId("alpha".to_string()),
        LinkLabel::stable(),
        Some(node::HubEndpoint(format!("tcp://{addr}"))),
        vec![Connector {
            id: "jobs-0".to_string(),
            kind: ConnectorKind::ProcessManager,
        }],
        "test".to_string(),
    )
    .expect("node config");

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

    let consumer = consumer_for(&addr).await;
    for _ in 0..100 {
        if consumer.topology().await.expect("topology").nodes.len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let err = consumer
        .route(
            LinkSelector::node(NodeId("alpha".to_string())),
            RoutedCall {
                connector: "jobs-0".to_string(),
                method: "not-a-method".to_string(),
                payload: b"{}".to_vec(),
            },
        )
        .await
        .expect_err("an unknown method must be refused");

    match err {
        vox::VoxError::User(route_error) => match *route_error {
            // Rejected, *not* UnknownConnector: the connector plainly answered.
            // Claiming the connector is missing would send a caller looking for
            // the wrong problem entirely.
            hub_protocol::RouteError::Node(hub_protocol::NodeError::Rejected(detail)) => {
                assert!(
                    detail.contains("not-a-method"),
                    "the engine's own message must survive the trip, got: {detail}"
                );
            }
            other => panic!("expected a node-side rejection, got {other:?}"),
        },
        other => panic!("expected a domain error, got {other:?}"),
    }

    node_task.abort();
    serving.abort();
}

/// **Boot autostart does not wait on the hub.** These are the machine's own
/// services; making them depend on a transport being reachable would tie a
/// local concern to a remote one, so a node with no hub in sight still brings
/// up what it declares.
///
/// implements: boot-autostart-reads-the-node-manifest
#[test]
#[allow(clippy::expect_used, reason = "a failure here is a broken harness")]
fn a_node_brings_up_its_declared_jobs_without_any_hub() {
    let dir = std::env::temp_dir().join(format!("kudo-node-boot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create");
    let dir = dir.canonicalize().expect("canonicalize");

    std::fs::write(
        dir.join(moco_job::MANIFEST_FILE),
        format!(
            r#"proc ({{name node-service, argv (sleep 30), cwd "{}", autostart @Boot}})"#,
            dir.display()
        ),
    )
    .expect("node manifest");

    let registry = JobRegistry::ungoverned()
        .expect("registry")
        .with_dir(&dir)
        .expect("with_dir");

    let started = registry.boot().expect("boot");
    assert_eq!(started.len(), 1, "the node's own declaration came up");
    assert_eq!(
        registry.scope_of(&started[0]),
        Some(moco_job::Scope::System),
        "a node job belongs to the node"
    );

    let _ = registry.kill(&started[0], &moco_job::Caller::Console);
    let _ = std::fs::remove_dir_all(&dir);
}
