#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Phase 3 — the job tools, driven against a real substrate.
//!
//! A real hub, a real node, a real registry spawning real children. What is
//! proven here is the whole agent-facing path: encode the engine's request in
//! the shim, route it through the hub, run it, and read the output back.
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

use std::sync::Arc;
use std::time::Duration;

use hub::{HubListener, ServedHub};
use hub_protocol::identity::{LinkLabel, NodeId};
use hub_protocol::{Connector, ConnectorKind};
use moco_job::JobRegistry;
use moco_job::wire::{KillRequest, StartRequest, TailRequest};
use node::Connectors;
use node::reconnect::{Backoff, DialOutcome};

use kudo_node::JobConnector;
use kudo_shim::hub::HubClient;
use kudo_shim::jobs::{self, DEFAULT_JOB_CONNECTOR};
use kudo_shim::route::Target;

/// A hub with one node offering the job substrate.
async fn mesh() -> (String, Arc<JobRegistry>) {
    let hub_state: ServedHub = ServedHub::new();
    let listener = HubListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().to_string();
    tokio::spawn({
        let hub_state = hub_state.clone();
        async move { listener.serve(hub_state).await }
    });

    let registry = Arc::new(JobRegistry::ungoverned().expect("registry"));
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

/// Start a job through the shim, then read back exactly what it wrote.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_starts_and_its_output_reads_back() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;

    let started = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["echo".into(), "from-the-shim".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("start");

    // Poll rather than block: nothing on this surface waits on a job, so a job
    // that never ends can never wedge the lane.
    let mut seen = String::new();
    for _ in 0..100 {
        let tail = jobs::tail(
            &client,
            alpha(),
            DEFAULT_JOB_CONNECTOR,
            TailRequest {
                id: started.id.clone(),
                offset: 0,
            },
        )
        .await
        .expect("tail");
        seen = String::from_utf8_lossy(&tail.bytes).into_owned();
        if seen.contains("from-the-shim") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        seen.contains("from-the-shim"),
        "the job's own output must come back, got: {seen}"
    );
}

/// `next_offset` really resumes: a second read returns only what is new.
#[tokio::test(flavor = "multi_thread")]
async fn tailing_from_next_offset_returns_only_new_output() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;

    let started = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["echo".into(), "first".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("start");

    let mut first = jobs::tail(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        TailRequest {
            id: started.id.clone(),
            offset: 0,
        },
    )
    .await
    .expect("tail");
    for _ in 0..100 {
        if !first.bytes.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        first = jobs::tail(
            &client,
            alpha(),
            DEFAULT_JOB_CONNECTOR,
            TailRequest {
                id: started.id.clone(),
                offset: 0,
            },
        )
        .await
        .expect("tail");
    }
    assert!(!first.bytes.is_empty(), "expected some output first");

    let second = jobs::tail(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        TailRequest {
            id: started.id.clone(),
            offset: first.next_offset,
        },
    )
    .await
    .expect("tail again");

    assert!(
        second.bytes.is_empty(),
        "resuming from next_offset must not re-deliver what was already read, got {:?}",
        String::from_utf8_lossy(&second.bytes)
    );
}

/// A started job shows up in the listing — including one this session did not
/// start, since jobs outlive the session that created them.
#[tokio::test(flavor = "multi_thread")]
async fn a_started_job_appears_in_the_listing() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;

    let started = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["echo".into(), "listed".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("start");

    let listing = jobs::list(&client, alpha(), DEFAULT_JOB_CONNECTOR)
        .await
        .expect("list");
    assert!(
        listing.jobs.iter().any(|j| j.id == started.id),
        "the job just started must be listed"
    );

    let rendered = jobs::render_jobs(&listing);
    assert!(rendered.contains(&started.id), "got:\n{rendered}");
}

/// A long-running job can be stopped, and its record survives the stop.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_can_be_killed_and_still_be_listed() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;

    let started = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["sleep".into(), "30".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("start");

    jobs::kill(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        KillRequest {
            id: started.id.clone(),
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("kill");

    let listing = jobs::list(&client, alpha(), DEFAULT_JOB_CONNECTOR)
        .await
        .expect("list");
    assert!(
        listing.jobs.iter().any(|j| j.id == started.id),
        "a killed job must still be listed — its fate is part of the history"
    );
}

/// The engine's refusal reaches the caller intact, still naming the binary.
#[tokio::test(flavor = "multi_thread")]
async fn an_unstartable_program_reports_the_engine_s_own_message() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;

    let err = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["definitely-not-a-real-program-xyz".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect_err("an unstartable program must fail");

    let message = err.to_string();
    assert!(
        message.contains("definitely-not-a-real-program-xyz"),
        "the engine's own message must survive the trip, got: {message}"
    );
}

/// **A foreign workspace's job cannot be written**, and the engine's refusal
/// reaches the caller unwidened.
///
/// This was deferred through all of v1 because the engine had no workspace
/// concept to refuse on. It does now, so the shim's half is finally testable:
/// the shim reports where it is, the engine decides what that means, and the
/// answer comes back naming both workspaces.
#[tokio::test(flavor = "multi_thread")]
async fn a_foreign_workspace_job_cannot_be_killed_through_the_shim() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;

    // Two distinct workspaces, neither of them the other.
    let theirs = std::env::temp_dir().join(format!("kudo-theirs-{}", std::process::id()));
    let mine = std::env::temp_dir().join(format!("kudo-mine-{}", std::process::id()));
    for dir in [&theirs, &mine] {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir.join(".git")).expect("make a repo");
    }

    // Started as if by a session in `theirs`.
    let started = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["sleep".into(), "30".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: theirs.to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("start");

    // And a session in `mine` tries to stop it.
    let err = jobs::kill(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        KillRequest {
            id: started.id.clone(),
            caller: moco_job::wire::WireCaller::Session {
                cwd: mine.to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect_err("a foreign workspace must be refused");

    let message = err.to_string();
    // Unwidened: the engine's own wording, naming both sides, not a generic
    // "permission denied" invented by the face.
    assert!(
        message.contains("owned by workspace"),
        "the engine's own refusal must survive the trip, got: {message}"
    );
    assert!(
        message.contains(&theirs.canonicalize().unwrap().display().to_string()),
        "must name the owning workspace, got: {message}"
    );

    // The job really was not stopped: the owning session can still stop it.
    jobs::kill(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        KillRequest {
            id: started.id,
            caller: moco_job::wire::WireCaller::Session {
                cwd: theirs.to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("the owning workspace may stop its own job");

    let _ = std::fs::remove_dir_all(&theirs);
    let _ = std::fs::remove_dir_all(&mine);
}

/// Resource readings reach an agent over the hub, and the render answers the
/// question that was asked rather than dumping the series.
#[tokio::test(flavor = "multi_thread")]
async fn a_jobs_resource_use_is_readable_over_the_mesh() {
    let (addr, registry) = mesh().await;
    let client = connected(&addr).await;

    let started = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec!["sleep".into(), "30".into()],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("start");

    // Stand in for the daemon's tick, which this harness does not run.
    registry.sample_all();

    let stats = jobs::stats(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        moco_job::wire::StatsRequest {
            id: started.id.clone(),
        },
    )
    .await
    .expect("stats");

    let latest = stats.samples.last().expect("a sample after sampling");
    assert!(latest.rss_bytes > 0, "a live process occupies memory");

    let rendered = jobs::render_stats(&stats);
    assert!(rendered.contains("memory:"), "got:\n{rendered}");
    assert!(rendered.contains("peak"), "got:\n{rendered}");

    let _ = jobs::kill(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        KillRequest {
            id: started.id,
            caller: moco_job::wire::WireCaller::Console,
        },
    )
    .await;
}

/// A breach reads as **a fact about the reading**, and says outright that
/// nothing acts on it.
///
/// The wording is the contract here: an agent told only "over its cpu ceiling"
/// may reasonably infer the supervisor is about to stop the job, and kill it
/// first to be tidy — which would be the enforcement this design refuses,
/// arrived at by inference.
#[test]
fn a_reported_breach_says_plainly_that_nothing_will_act_on_it() {
    let rendered = jobs::render_stats(&moco_job::wire::StatsReply {
        samples: vec![moco_job::Sample {
            at_unix_ms: 1,
            cpu_pct: 800,
            rss_bytes: 64 * 1024 * 1024,
        }],
        limits: moco_job::Limits {
            cpu_pct: 100,
            mem_mb: 0,
        },
        breach: moco_job::Breach {
            cpu: true,
            memory: false,
        },
    });

    assert!(rendered.contains("cpu ceiling"), "got:\n{rendered}");
    assert!(rendered.contains("advisory"), "got:\n{rendered}");
    assert!(
        rendered.contains("nothing will"),
        "the reply must foreclose the inference that a stop is coming, got:\n{rendered}"
    );
}

/// A job that has not been sampled reports that, rather than an idle-looking
/// zero.
#[test]
fn no_samples_is_said_out_loud_and_not_shown_as_idle() {
    let rendered = jobs::render_stats(&moco_job::wire::StatsReply {
        samples: vec![],
        limits: moco_job::Limits::default(),
        breach: moco_job::Breach {
            cpu: false,
            memory: false,
        },
    });
    assert!(rendered.contains("no samples yet"), "got:\n{rendered}");
    assert!(!rendered.contains("0%"), "got:\n{rendered}");
}

/// The screen lens over the mesh: a redrawing job reads back as its current
/// frame, not every frame it ever drew.
#[tokio::test(flavor = "multi_thread")]
async fn a_redrawing_job_reads_back_as_one_screen_over_the_mesh() {
    let (addr, _registry) = mesh().await;
    let client = connected(&addr).await;

    let started = jobs::start(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        StartRequest {
            argv: vec![
                "sh".into(),
                "-c".into(),
                "printf 'step 1/3\rstep 2/3\rstep 3/3'; sleep 5".into(),
            ],
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            deadline_ms: 0,
            caller: moco_job::wire::WireCaller::Session {
                cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            },
        },
    )
    .await
    .expect("start");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let view = jobs::screen(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        moco_job::wire::ScreenRequest {
            id: started.id.clone(),
        },
    )
    .await
    .expect("screen");

    // An ad-hoc start has no declared human view, so this is a logs job and its
    // screen is reconstructed — which is exactly what must be reported.
    assert_eq!(view.source, moco_job::ScreenSource::Replayed);
    let text = String::from_utf8_lossy(&view.bytes);
    assert!(text.contains("step 3/3"), "got:\n{text}");
    assert!(!text.contains("step 1/3"), "got:\n{text}");

    let rendered = jobs::render_screen(&view);
    assert!(
        rendered.contains("reconstructed"),
        "a reconstructed screen must say so, got:\n{rendered}"
    );

    let _ = jobs::kill(
        &client,
        alpha(),
        DEFAULT_JOB_CONNECTOR,
        KillRequest {
            id: started.id,
            caller: moco_job::wire::WireCaller::Console,
        },
    )
    .await;
}
