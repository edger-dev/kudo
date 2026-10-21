//! Starting a worker in a pane on a node: what the launcher asks the node for,
//! and how it reads the answer — the same for any face that starts one.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_shim::panes;

#[test]
fn a_worker_pane_request_binds_the_worker_and_asks_for_a_terminal() {
    let req = panes::worker_request(
        "ctx-full-id",
        Some("kudo"),
        "lease-1",
        "yolo",
        Some("10.0.0.1:15400"),
    );
    assert_eq!(
        req.argv,
        ["kudo-shim", "worker", "ctx-full-id", "--repo", "kudo"]
    );
    assert_eq!(req.human_view, moco_job::lens::HumanView::Terminal);
    let env: Vec<(&str, &str)> = req
        .env
        .iter()
        .map(|v| (v.name.as_str(), v.value.as_str()))
        .collect();
    assert!(env.contains(&("KUDO_GIVEN_LEASE", "lease-1")), "{env:?}");
    assert!(
        env.contains(&("KUDO_NODE", "yolo")),
        "the worker names its node as the hub does: {env:?}"
    );
    assert!(
        env.contains(&("KUDO_HUB_ADDR", "10.0.0.1:15400")),
        "{env:?}"
    );
    assert!(
        env.iter().all(|(k, _)| !k.starts_with("AWS_")),
        "no secrets travel"
    );
}

#[test]
fn a_reply_that_ignored_the_fields_is_an_old_node() {
    let req = panes::worker_request("c", None, "l", "n", None);
    let honoured = moco_job::wire::StartReply {
        id: "j".into(),
        env_set: u32::try_from(req.env.len()).unwrap(),
        human_view: moco_job::lens::HumanView::Terminal,
    };
    assert!(panes::applied(&req, &honoured));
    let ignored = moco_job::wire::StartReply {
        id: "j".into(),
        env_set: 0,
        human_view: moco_job::lens::HumanView::Logs,
    };
    assert!(!panes::applied(&req, &ignored));
}

/// A launcher's lease comes in its own variable: a shell inside a worker
/// session inherits that session's KUDO_LEASE, and must not reuse it.
#[test]
fn an_inherited_lease_is_never_reused() {
    let given = "6f1d0c9e-2b7a-4c3e-9f00-1234567890ab";
    let inherited = "11111111-2222-3333-4444-555555555555";
    let lease = kudo_shim::cli::lease_from(|k| match k {
        "KUDO_LEASE" => Some(inherited.to_owned()),
        _ => None,
    })
    .unwrap();
    assert_ne!(lease, inherited);
    let lease = kudo_shim::cli::lease_from(|k| match k {
        "KUDO_LEASE" => Some(inherited.to_owned()),
        "KUDO_GIVEN_LEASE" => Some(given.to_owned()),
        _ => None,
    })
    .unwrap();
    assert_eq!(lease, given);
}

/// The console starts workers only on the nodes it is told of; told of none,
/// on any.
// implements: 2f073c122047d13470407233fd0095dd86408b1ebc66ed6f136c3f3b7027b2c5@2f073c122047d13470407233fd0095dd86408b1ebc66ed6f136c3f3b7027b2c5
#[test]
fn workers_start_only_on_the_nodes_named() {
    let named =
        panes::worker_nodes_from(|k| (k == panes::WORKER_NODES_ENV).then(|| " yolo , ".to_owned()));
    assert_eq!(named, Some(vec!["yolo".to_owned()]));
    assert!(panes::may_start("yolo", named.as_deref()));
    assert!(!panes::may_start("pc", named.as_deref()));
    assert!(panes::may_start("pc", None));
    assert_eq!(panes::worker_nodes_from(|_| None), None);
    assert_eq!(panes::WORKER_NODES_ENV, "KUDO_CONSOLE_WORKER_NODES");
}
