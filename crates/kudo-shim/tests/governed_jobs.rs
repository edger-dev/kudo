#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Governed jobs through the shim: a start that waits for the author's
//! verdict says so, a verdict from the console runs it, waiting on a job
//! comes back with how it ended and under which verdict — and a worker traces
//! the jobs it starts in its context.
//!
//! implements: kudo:daf63591a53811da5e761c847a528776758b90ee07fc55f8c16ce14789be1f44
//! implements: kudo:e23a8b06aa9331b3f1ffd8df36be1392daf44e93ca20037b50cec6c3132f53e1@d9656cc142296b3c9017b9d73ca59d326c9a29abd2945a5afd4a73eb6514f593

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use hub::{HubListener, ServedHub};
use hub_protocol::identity::{LinkLabel, NodeId};
use hub_protocol::{Connector, ConnectorKind};
use kudo_context::{ContextStore, TraceEvent};
use moco_job::wire::{DecideRequest, StartRequest, WireCaller, WireVerdict};
use moco_job::{JobRegistry, JobStatus, NodePolicy, RuleSet, SeedConfig, Verdict};
use node::Connectors;
use node::reconnect::{Backoff, DialOutcome};

use kudo_node::JobConnector;
use kudo_shim::hub::HubClient;
use kudo_shim::jobs::{self, DEFAULT_JOB_CONNECTOR};
use kudo_shim::route::Target;
use kudo_shim::worker_tools;
use kudo_worker::WorkerSetup;

const T0: i64 = 1_792_500_000;

/// A hub with one governed node offering the job substrate.
async fn mesh() -> (String, Arc<JobRegistry>) {
    let hub_state: ServedHub = ServedHub::new();
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    tokio::spawn({
        let hub_state = hub_state.clone();
        async move { listener.serve(hub_state).await }
    });

    let registry = Arc::new(
        JobRegistry::with_policy(
            NodePolicy::new(
                RuleSet::from_seed(SeedConfig {
                    allow: vec![vec!["echo".into(), "seeded".into()]],
                    deny: vec![vec!["echo".into(), "barred".into()]],
                }),
                "/",
            )
            .with_approval_timeout(Duration::from_secs(20)),
        )
        .expect("registry"),
    );
    let mut connectors = Connectors::new();
    connectors.register(Box::new(JobConnector::new(
        DEFAULT_JOB_CONNECTOR,
        registry.clone(),
    )));
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

    tokio::spawn({
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

    (addr, registry)
}

async fn connected(addr: &str) -> HubClient {
    let client = HubClient::dial(addr).await.expect("dial");
    for _ in 0..100 {
        if client.topology().await.map(|t| t.nodes.len()).unwrap_or(0) == 1 {
            return client;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the node never registered");
}

fn alpha() -> Target {
    Target::node("alpha")
}

fn adhoc(argv: &[&str]) -> StartRequest {
    StartRequest {
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        cwd: "/tmp".into(),
        deadline_ms: 0,
        caller: jobs::session_caller(),
        env: vec![],
        human_view: moco_job::lens::HumanView::Logs,
    }
}

/// A seeded argv runs; an unlisted one comes back waiting for a verdict.
#[tokio::test(flavor = "multi_thread")]
async fn a_start_says_whether_it_waits_for_a_verdict() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;
    let ran = jobs::start_gated(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        adhoc(&["echo", "seeded"]),
    )
    .await
    .unwrap();
    assert!(!ran.waiting);
    let asked = jobs::start_gated(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        adhoc(&["echo", "asked"]),
    )
    .await
    .unwrap();
    assert!(asked.waiting);
    let pending = jobs::pending(&client, alpha(), DEFAULT_JOB_CONNECTOR)
        .await
        .unwrap();
    assert_eq!(pending.jobs.len(), 1);
    assert_eq!(pending.jobs[0].id, asked.id);
}

/// Waiting on a pending job within a short budget says it is still waiting;
/// after the console's verdict, the wait comes back with the end and the
/// verdict.
#[tokio::test(flavor = "multi_thread")]
async fn waiting_returns_the_end_and_its_verdict() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;
    let asked = jobs::start_gated(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        adhoc(&["echo", "asked"]),
    )
    .await
    .unwrap();
    let early = jobs::wait_for(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        &asked.id,
        Duration::from_millis(300),
        Duration::from_millis(100),
    )
    .await
    .unwrap();
    assert!(early.is_none(), "still waiting for a verdict");

    let reply = jobs::decide(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        DecideRequest {
            id: asked.id.clone(),
            verdict: WireVerdict::AllowOnce,
            edited_argv: vec![],
            caller: WireCaller::Console,
        },
    )
    .await
    .unwrap();
    assert_eq!(reply.verdict, Verdict::ApprovedOnce);

    let end = jobs::wait_for(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        &asked.id,
        Duration::from_secs(5),
        Duration::from_millis(100),
    )
    .await
    .unwrap()
    .expect("it ended");
    assert_eq!(end.status, JobStatus::Done { code: 0 });
    assert_eq!(end.verdict, Verdict::ApprovedOnce);
    assert_eq!(jobs::status_word(&end.status), "done:0");
    assert_eq!(jobs::verdict_word(end.verdict), "approved-once");
}

/// A session's verdict is refused through the shim too.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_cannot_approve_through_the_shim() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;
    let asked = jobs::start_gated(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        adhoc(&["echo", "asked"]),
    )
    .await
    .unwrap();
    let refused = jobs::decide(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        DecideRequest {
            id: asked.id,
            verdict: WireVerdict::AllowOnce,
            edited_argv: vec![],
            caller: jobs::session_caller(),
        },
    )
    .await;
    assert!(refused.is_err());
}

fn worker() -> (WorkerSetup, ContextStore) {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "kudo-shim-governed-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let context = store.create_context("jobs", "# jobs\n", T0).unwrap();
    (
        WorkerSetup {
            store_dir: dir,
            context,
            lease: "lease-g".into(),
            claim_ttl: 600,
            max_turns: 10,
            idle_limit: 0,
        },
        store,
    )
}

/// A worker traces the jobs it starts, and their end once; an end for a job
/// it did not start is not its to trace.
#[test]
fn a_worker_traces_its_jobs_start_and_end_once() {
    let (setup, store) = worker();
    worker_tools::trace_job_start(&setup, "pc", "j-1", true, &["df".into(), "-h".into()], T0)
        .unwrap();
    let end = worker_tools::trace_job_end(
        &setup,
        "pc",
        "j-1",
        "done:0",
        "approved-once",
        "/dev/nvme1n1p1 1.8T 215G 1.5T 13% /home\n",
        T0 + 1,
    )
    .unwrap()
    .expect("the end is recorded, and named");
    // The end record holds what the job printed: a claim made from it is
    // citable.
    assert!(
        store.kino_text(&end).unwrap().contains("13% /home"),
        "{}",
        store.kino_text(&end).unwrap()
    );
    assert!(
        worker_tools::trace_job_end(&setup, "pc", "j-1", "done:0", "approved-once", "", T0 + 2)
            .unwrap()
            .is_none()
    );
    assert!(
        worker_tools::trace_job_end(&setup, "pc", "other", "done:0", "seed-allow", "", T0 + 3)
            .unwrap()
            .is_none()
    );

    let events: Vec<TraceEvent> = store
        .trace(&setup.context)
        .unwrap()
        .into_iter()
        .map(|e| e.event)
        .collect();
    assert_eq!(
        events,
        vec![
            TraceEvent::JobStart {
                node: "pc".into(),
                job: "j-1".into(),
                waiting: true,
                argv: "df -h".into(),
            },
            TraceEvent::JobEnd {
                node: "pc".into(),
                job: "j-1".into(),
                status: "done:0".into(),
                verdict: "approved-once".into(),
            },
        ]
    );
}

/// A start a deny rule refuses comes back ended, not started.
#[tokio::test(flavor = "multi_thread")]
async fn a_denied_start_comes_back_ended() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;
    let barred = jobs::start_gated(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        adhoc(&["echo", "barred"]),
    )
    .await
    .unwrap();
    assert!(!barred.waiting);
    assert!(barred.status.is_terminal(), "{:?}", barred.status);
    assert_eq!(jobs::status_word(&barred.status), "denied:rule");
}

/// The end is matched by the job id: a node spelled another way (a peer
/// hub's `edger::yolo` for `yolo`) still closes the start.
#[test]
fn an_end_matches_its_start_by_the_job_id() {
    let (setup, _store) = worker();
    worker_tools::trace_job_start(&setup, "yolo", "j-9", false, &["uptime".into()], T0).unwrap();
    assert!(worker_tools::awaits_end(&setup, "j-9").unwrap());
    assert!(
        worker_tools::trace_job_end(
            &setup,
            "edger::yolo",
            "j-9",
            "done:0",
            "seed-allow",
            "",
            T0 + 1
        )
        .unwrap()
        .is_some()
    );
    assert!(!worker_tools::awaits_end(&setup, "j-9").unwrap());
}

/// An argv that reads as nothing still traces and reads back.
#[test]
fn an_empty_argv_still_traces() {
    let (setup, store) = worker();
    worker_tools::trace_job_start(&setup, "pc", "j-e", true, &[String::new()], T0).unwrap();
    let events = store.trace(&setup.context).unwrap();
    assert_eq!(events.len(), 1, "the start must not be dropped");
    assert!(worker_tools::awaits_end(&setup, "j-e").unwrap());
}
