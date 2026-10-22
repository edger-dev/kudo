//! Starting a worker in a pane on a node: what the launcher asks the node for,
//! and how it reads the answer — the same for any face that starts one.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_shim::panes;

/// What crosses to the node is the typed ask alone: no command line, no
/// environment, no secret — the node composes those.
// implements: 5720302421d8748ea19b23da1f119f7c0e7514753b340ae5e425fa7e48588b41
#[test]
fn a_worker_pane_is_asked_for_by_its_values_alone() {
    let ask = panes::worker_ask("ctx-full-id", Some("kudo"), "lease-1");
    assert_eq!(ask.context, "ctx-full-id");
    assert_eq!(ask.repo, "kudo");
    assert_eq!(ask.lease, "lease-1");
    assert_eq!(panes::worker_ask("c", None, "l").repo, "");
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
