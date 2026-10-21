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

/// The server's own description to the model says when this session has no
/// hub, rather than promising one.
#[test]
fn without_a_hub_the_instructions_say_so() {
    use rmcp::ServerHandler;
    let info = ShimServer::without_hub("KUDO_HUB_ADDR is not set".into()).get_info();
    let text = info.instructions.unwrap_or_default();
    assert!(text.contains("no hub"), "{text}");
    assert!(text.contains("pull") && text.contains("submit"), "{text}");
}

#[test]
fn an_empty_hub_address_is_no_hub_address() {
    assert!(matches!(
        plan_startup(Some(String::new()), true),
        Startup::WithoutHub(_)
    ));
    assert!(matches!(
        plan_startup(Some("  ".into()), false),
        Startup::Refuse(_)
    ));
}

/// A worker session dials the hub when a job tool first needs it, and again
/// after a failure: a hub that was down when the session started is used
/// once it is up.
#[tokio::test]
async fn a_hub_that_comes_up_later_is_used() {
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = free.local_addr().unwrap().to_string();
    drop(free);

    let server = ShimServer::lazy(addr.clone(), None);
    let down = server.call_nodes().await;
    assert_eq!(down.is_error, Some(true), "nothing listening yet");

    let listener = hub::HubListener::bind(&addr).await.unwrap();
    tokio::spawn(async move { listener.serve(hub::ServedHub::new()).await });
    let up = server.call_nodes().await;
    assert_ne!(up.is_error, Some(true), "{:?}", up.content);
}

/// A hub that accepts the connection but never answers costs a few seconds,
/// not the session.
#[tokio::test]
async fn a_hub_that_hangs_gives_up_in_seconds() {
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = silent.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = silent.accept().await {
            held.push(socket); // accept, never answer
        }
    });
    let server = ShimServer::lazy(addr, None);
    let started = std::time::Instant::now();
    let reply = server.call_nodes().await;
    assert_eq!(reply.is_error, Some(true));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

/// With no hub address at all, the job and node tools are not listed: they
/// could never work in this session.
#[test]
fn with_no_hub_address_only_the_worker_tools_are_listed() {
    let names = ShimServer::without_hub("KUDO_HUB_ADDR is not set".into()).listed_tool_names();
    let mut names: Vec<&str> = names.iter().map(String::as_str).collect();
    names.sort();
    assert_eq!(
        names,
        ["context_read", "inbox", "pull", "release", "submit"]
    );
}

/// Only the mirror's non-secret settings are handed to the worker by name;
/// keys reach it by inheritance, never on a command line.
#[test]
fn only_non_secret_mirror_settings_are_forwarded() {
    let vars = [
        ("KUDO_CONTEXT_MIRROR", "s3://b/p"),
        ("AWS_ENDPOINT", "http://127.0.0.1:9100"),
        ("AWS_REGION", "auto"),
        ("AWS_ACCESS_KEY_ID", "id"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
        ("AWS_PROFILE", "work"),
        ("HOME", "/home/x"),
    ]
    .map(|(k, v)| (k.to_owned(), v.to_owned()));
    let mut kept: Vec<String> = kudo_shim::cli::mirror_settings_from(vars)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    kept.sort();
    assert_eq!(kept, ["AWS_ENDPOINT", "AWS_REGION", "KUDO_CONTEXT_MIRROR"]);
}
