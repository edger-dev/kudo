#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Every node runs governed, with the policy its configuration gives it.
//!
//! implements: kudo:daf63591a53811da5e761c847a528776758b90ee07fc55f8c16ce14789be1f44

use std::collections::HashMap;
use std::time::Duration;

use kudo_node::governance::{
    ALLOWED_ROOT_ENV, APPROVAL_TIMEOUT_ENV, DEFAULT_APPROVAL_TIMEOUT, RULES_ENV, policy_from,
};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |k| map.get(k).cloned()
}

fn seed_file(text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "kudo-node-rules-{}-{}.styx",
        std::process::id(),
        text.len()
    ));
    std::fs::write(&path, text).unwrap();
    path
}

/// With nothing configured the node is still governed: no rules, so every
/// command asks; the whole filesystem is the root; 30 minutes to answer.
#[test]
fn an_unconfigured_node_is_governed_and_asks_for_everything() {
    let policy = policy_from(env(&[])).unwrap();
    assert!(policy.rules().allow_rules().is_empty());
    assert!(policy.rules().deny_rules().is_empty());
    assert_eq!(policy.allowed_root(), std::path::Path::new("/"));
    assert_eq!(policy.approval_timeout(), DEFAULT_APPROVAL_TIMEOUT);
    assert_eq!(DEFAULT_APPROVAL_TIMEOUT, Duration::from_secs(30 * 60));
}

/// The seed rules come from the file the configuration names.
#[test]
fn the_seed_rules_come_from_the_named_file() {
    let path = seed_file("allow ((df -h /))\ndeny ((rm -rf /))\n");
    let policy = policy_from(env(&[(RULES_ENV, path.to_str().unwrap())])).unwrap();
    assert_eq!(
        policy.rules().allow_rules(),
        &[vec!["df".to_owned(), "-h".to_owned(), "/".to_owned()]]
    );
    assert_eq!(policy.rules().deny_rules().len(), 1);
}

/// A named file that is missing or unreadable fails the start, naming it:
/// a node must not come up with its rules silently gone.
#[test]
fn a_missing_or_broken_rules_file_is_an_error_naming_it() {
    let missing = std::env::temp_dir().join("kudo-node-no-such-rules.styx");
    let Err(err) = policy_from(env(&[(RULES_ENV, missing.to_str().unwrap())])) else {
        panic!("a missing rules file must be an error");
    };
    assert!(err.contains("kudo-node-no-such-rules.styx"), "{err}");

    let broken = seed_file("allow ((((");
    let Err(err) = policy_from(env(&[(RULES_ENV, broken.to_str().unwrap())])) else {
        panic!("a broken rules file must be an error");
    };
    assert!(err.contains(broken.to_str().unwrap()), "{err}");
}

/// The root and the timeout can be set; a timeout that is not a number of
/// seconds is an error, not a default.
#[test]
fn root_and_timeout_are_configurable() {
    let policy = policy_from(env(&[
        (ALLOWED_ROOT_ENV, "/home"),
        (APPROVAL_TIMEOUT_ENV, "90"),
    ]))
    .unwrap();
    assert_eq!(policy.allowed_root(), std::path::Path::new("/home"));
    assert_eq!(policy.approval_timeout(), Duration::from_secs(90));

    assert!(policy_from(env(&[(APPROVAL_TIMEOUT_ENV, "soon")])).is_err());
    assert!(policy_from(env(&[(APPROVAL_TIMEOUT_ENV, "0")])).is_err());
}

/// The node's registry knows the node's name, so a manifest entry's `hosts`
/// match it: a recipe (or boot job) declared for this node is this node's.
// implements: 7735b6fcb04483f5bb958b825bd6e7057146db208d5f5e5b9da25d4ad9dcff01
#[test]
fn the_nodes_registry_knows_the_nodes_name() {
    let state = std::env::temp_dir().join(format!("kudo-node-named-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let state = state.canonicalize().unwrap();
    std::fs::write(
        state.join(moco_job::MANIFEST_FILE),
        format!(
            r#"proc (
  {{name mine, argv (echo mine), cwd "{0}", hosts (laptop)}}
  {{name theirs, argv (echo theirs), cwd "{0}", hosts (desk)}}
)"#,
            state.display()
        ),
    )
    .unwrap();
    let policy = policy_from(env(&[])).unwrap();
    let registry = kudo_node::open_registry(policy, &state, "laptop").unwrap();
    let names: Vec<String> = registry
        .recipes()
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["mine"]);
}
