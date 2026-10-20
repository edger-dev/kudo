//! The environment the launcher hands a worker session, read back by the
//! shim's hooks and tools.

#![allow(clippy::unwrap_used, reason = "tests")]

use std::collections::HashMap;
use std::path::PathBuf;

use kudo_context::ContextId;
use kudo_worker::WorkerSetup;

#[test]
fn a_setup_survives_the_environment_round_trip() {
    let setup = WorkerSetup {
        store_dir: PathBuf::from("/var/kudo/contexts"),
        context: ContextId("abc".into()),
        lease: "6f1e…".into(),
        claim_ttl: 900,
        max_turns: 7,
    };
    let env: HashMap<String, String> = setup.to_env().into_iter().collect();
    let back = WorkerSetup::from_lookup(|k| env.get(k).cloned()).unwrap();
    assert_eq!(back, setup);
}

#[test]
fn a_session_without_a_context_is_not_a_worker() {
    assert_eq!(WorkerSetup::from_lookup(|_| None), None);
}

#[test]
fn missing_limits_take_the_defaults() {
    let env: HashMap<&str, &str> = [
        ("KUDO_CONTEXT_STORE", "/s"),
        ("KUDO_CONTEXT", "c"),
        ("KUDO_LEASE", "l"),
    ]
    .into_iter()
    .collect();
    let setup = WorkerSetup::from_lookup(|k| env.get(k).map(|v| v.to_string())).unwrap();
    assert_eq!(setup.claim_ttl, kudo_worker::DEFAULT_CLAIM_TTL);
    assert_eq!(setup.max_turns, kudo_worker::DEFAULT_MAX_TURNS);
}
