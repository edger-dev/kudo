#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A worker pane starts through a typed node operation: the node checks the
//! values and composes the command itself; no caller writes a command line.
//!
//! implements: kudo:5720302421d8748ea19b23da1f119f7c0e7514753b340ae5e425fa7e48588b41@f34266201d70f3802a59ab928778c8fbc78c14e8aa300c9c3c53301828ba8c1c

use std::sync::Arc;
use std::time::Duration;

use moco_job::lens::HumanView;
use moco_job::{JobId, JobRegistry, NodePolicy, RuleSet, Scope, SeedConfig, Verdict};
use node::connector::Connector;

use kudo_node::worker::{
    PaneSetup, START_WORKER, WORKER_CONNECTOR, WorkerConnector, WorkerStart, WorkerStarted, compose,
};

const CTX: &str = "2d46ab242a8fe23db32f26bbf51025de73f571949966f7b06725c5157596c0a0";
const LEASE: &str = "2b92aaa1-70c9-4b24-b472-030f2ff09d68";

fn setup(program: &str) -> PaneSetup {
    PaneSetup {
        program: program.into(),
        cwd: "/tmp".into(),
        node: "yolo".into(),
        hub: Some("10.100.0.1:15400".into()),
    }
}

fn ask(context: &str, repo: &str, lease: &str) -> WorkerStart {
    WorkerStart {
        context: context.into(),
        repo: repo.into(),
        lease: lease.into(),
    }
}

fn governed() -> Arc<JobRegistry> {
    Arc::new(
        JobRegistry::with_policy(
            NodePolicy::new(RuleSet::from_seed(SeedConfig::default()), "/")
                .with_approval_timeout(Duration::from_secs(5)),
        )
        .unwrap(),
    )
}

/// The node composes `kudo-shim worker <context>` in a terminal, bound by the
/// lease, named as the hub knows the node, pointed at its hub.
#[test]
fn the_node_composes_the_worker_command() {
    let job = compose(&ask(CTX, "", LEASE), &setup("kudo-shim")).unwrap();
    assert_eq!(job.argv, vec!["kudo-shim", "worker", CTX]);
    assert_eq!(job.cwd, std::path::PathBuf::from("/tmp"));
    assert_eq!(job.human_view, HumanView::Terminal);
    assert_eq!(job.scope, Some(Scope::System));
    let env: std::collections::HashMap<_, _> = job.env.iter().cloned().collect();
    assert_eq!(env.get("KUDO_GIVEN_LEASE").map(String::as_str), Some(LEASE));
    assert_eq!(env.get("KUDO_NODE").map(String::as_str), Some("yolo"));
    assert_eq!(
        env.get("KUDO_HUB_ADDR").map(String::as_str),
        Some("10.100.0.1:15400")
    );
    assert_eq!(env.len(), 3, "nothing else crosses: {env:?}");
}

#[test]
fn a_repo_becomes_the_repo_flag() {
    let job = compose(&ask(CTX, "kudo", LEASE), &setup("kudo-shim")).unwrap();
    assert_eq!(job.argv, vec!["kudo-shim", "worker", CTX, "--repo", "kudo"]);
}

/// Each value is checked for its shape; anything else is refused, naming it.
#[test]
fn values_of_the_wrong_shape_are_refused() {
    let short = &CTX[..12];
    let upper = CTX.to_uppercase();
    let not_hex = format!("{}zz", &CTX[..62]);
    for context in [short, upper.as_str(), not_hex.as_str(), ""] {
        let err = compose(&ask(context, "", LEASE), &setup("kudo-shim")).unwrap_err();
        assert!(err.contains("context"), "{context:?}: {err}");
    }
    let long = "a".repeat(65);
    for repo in ["-x", "a b", "../up", "a/b", ".hidden", long.as_str()] {
        let err = compose(&ask(CTX, repo, LEASE), &setup("kudo-shim")).unwrap_err();
        assert!(err.contains("repo"), "{repo:?}: {err}");
    }
    for lease in [
        "",
        "not-a-lease",
        "2B92AAA1-70C9-4B24-B472-030F2FF09D68",
        &LEASE[..35],
    ] {
        let err = compose(&ask(CTX, "", lease), &setup("kudo-shim")).unwrap_err();
        assert!(err.contains("lease"), "{lease:?}: {err}");
    }
}

/// Over the connector, on a governed registry, the pane starts at once — no
/// approval — and the audit says the node vouched for it.
#[test]
fn a_worker_starts_on_a_governed_node_vouched() {
    let registry = governed();
    let connector = WorkerConnector::new(registry.clone(), setup("echo"));
    assert_eq!(connector.descriptor().id, WORKER_CONNECTOR);

    let payload = kudo_node::worker::encode(&ask(CTX, "kudo", LEASE)).unwrap();
    let reply = connector.call(START_WORKER, &payload).unwrap();
    let started: WorkerStarted = kudo_node::worker::decode(&reply).unwrap();

    let id = JobId(started.id);
    let outcome = registry.wait(&id).unwrap();
    assert_eq!(outcome.verdict, Verdict::Vouched);
    let screen = registry.screen(&id).unwrap().text;
    assert!(
        screen.contains("worker") && screen.contains(CTX),
        "{screen:?}"
    );
}

/// A refused value comes back as the connector's refusal, and nothing starts.
#[test]
fn a_refused_start_starts_nothing() {
    let registry = governed();
    let connector = WorkerConnector::new(registry.clone(), setup("echo"));
    let payload = kudo_node::worker::encode(&ask(CTX, "../up", LEASE)).unwrap();
    assert!(connector.call(START_WORKER, &payload).is_err());
    assert!(registry.list().is_empty());
}

#[test]
fn an_unknown_method_is_refused() {
    let connector = WorkerConnector::new(governed(), setup("echo"));
    assert!(connector.call("start", b"{}").is_err());
}

/// A pane starts inside the allowed root: `/tmp` when the root holds it.
#[test]
fn a_pane_starts_inside_the_allowed_root() {
    use kudo_node::worker::pane_cwd;
    use std::path::Path;
    assert_eq!(pane_cwd(Path::new("/")), Path::new("/tmp"));
    assert_eq!(pane_cwd(Path::new("/home/yj")), Path::new("/home/yj"));
}
