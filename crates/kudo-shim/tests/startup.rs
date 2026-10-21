//! What the shim needs to start: a worker session needs the context store,
//! not the hub — it starts without one and its job tools say why; a session
//! that is only job tools needs the hub and refuses to start without it.
//!
//! implements: 482bd420c2a2251c6d5c2fa311e8a14b2bb0898cf84cedfbef88fe6ba8a119ae@482bd420c2a2251c6d5c2fa311e8a14b2bb0898cf84cedfbef88fe6ba8a119ae

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_shim::startup::{Startup, after_dial_failure, plan_startup};
use kudo_shim::{ShimError, ShimServer};

#[test]
fn a_hub_address_is_dialled() {
    assert_eq!(
        plan_startup(Some("10.0.0.1:15400".into()), true),
        Startup::Dial("10.0.0.1:15400".into())
    );
    assert_eq!(
        plan_startup(Some("10.0.0.1:15400".into()), false),
        Startup::Dial("10.0.0.1:15400".into())
    );
}

#[test]
fn a_worker_session_starts_without_a_hub_address() {
    match plan_startup(None, true) {
        Startup::WithoutHub(why) => assert!(why.contains("KUDO_HUB_ADDR"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_job_only_session_without_a_hub_address_refuses_to_start() {
    assert!(matches!(plan_startup(None, false), Startup::Refuse(_)));
}

#[test]
fn a_hub_that_does_not_answer_only_stops_a_job_only_session() {
    let err = ShimError::HubUnreachable {
        addr: "10.0.0.1:15400".into(),
        source: std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused"),
    };
    match after_dial_failure(&err, true) {
        Startup::WithoutHub(why) => assert!(why.contains("connection refused"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        after_dial_failure(&err, false),
        Startup::Refuse(_)
    ));
}

/// Without a hub, every job tool answers that there is none, and why —
/// as a tool error, never as an empty result.
#[tokio::test]
async fn without_a_hub_a_job_tool_says_so() {
    let server = ShimServer::without_hub("KUDO_HUB_ADDR is not set".into());
    let reply = server.call_nodes().await;
    assert_eq!(reply.is_error, Some(true));
    let text = serde_json::to_string(&reply.content).unwrap();
    assert!(
        text.contains("no hub") && text.contains("KUDO_HUB_ADDR is not set"),
        "{text}"
    );
}
