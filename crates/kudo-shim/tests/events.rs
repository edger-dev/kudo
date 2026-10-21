//! The loop's events on the hub: published best effort, read by prefix.
//!
//! implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::time::Duration;

use hub::{HubListener, ServedHub};
use hub_protocol::HubEvent;
use hub_protocol::service::ConsumerApiClient;
use kudo_context::ContextStore;
use kudo_shim::{events, hook};
use kudo_worker::WorkerSetup;

const T0: i64 = 1_792_500_000;

async fn hub() -> String {
    let listener = HubListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().to_string();
    tokio::spawn(async move { listener.serve(ServedHub::new()).await });
    addr
}

fn owned(got: vox::SelfRef<HubEvent>) -> HubEvent {
    let mut out = None;
    let _ = got.map(|e| out = Some(e));
    out.unwrap()
}

/// Subscribe to `prefix` and wait until the hub says the subscription is live.
async fn subscribed(addr: &str, prefix: &str) -> vox::Rx<HubEvent> {
    let socket = tokio::net::TcpStream::connect(addr).await.unwrap();
    let consumer: ConsumerApiClient =
        vox::initiator_on(vox::transport::tcp::StreamLink::tcp(socket))
            .establish_connection()
            .await
            .unwrap()
            .open_lane()
            .await
            .unwrap();
    let (tx, mut rx) = vox::channel::<HubEvent>();
    let prefix = prefix.to_string();
    tokio::spawn(async move {
        let _ = consumer.subscribe_events(prefix, tx).await;
    });
    let hello = owned(rx.recv().await.unwrap().unwrap());
    assert_eq!(hello.topic, hub_protocol::SUBSCRIBED_TOPIC);
    rx
}

async fn next(rx: &mut vox::Rx<HubEvent>) -> HubEvent {
    owned(
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("an event in time")
            .unwrap()
            .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_published_event_reaches_a_subscriber() {
    let addr = hub().await;
    let mut rx = subscribed(&addr, "kudo/context/c1/").await;
    let topic = events::topic("c1", "lease-1", "submit");
    assert_eq!(topic, "kudo/context/c1/lease-1/submit");
    assert!(events::publish(
        &addr,
        topic.clone(),
        b"submit x y".to_vec()
    ));
    let got = next(&mut rx).await;
    assert_eq!(got.topic, topic);
    assert_eq!(got.payload, b"submit x y");
}

#[test]
fn a_hub_that_is_not_there_costs_under_a_second() {
    let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = free.local_addr().unwrap().to_string();
    drop(free);
    let started = std::time::Instant::now();
    assert!(!events::publish(&addr, "kudo/x".into(), Vec::new()));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

/// A hook publishes the event it traces.
#[tokio::test(flavor = "multi_thread")]
async fn the_session_start_hook_publishes_lease_start() {
    let addr = hub().await;
    let dir = std::env::temp_dir().join(format!("kudo-shim-events-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let ctx = store.create_context("evented", "# e\n", T0).unwrap();
    let setup = WorkerSetup {
        store_dir: dir.clone(),
        context: ctx.clone(),
        lease: "lease-9".into(),
        claim_ttl: 600,
        max_turns: 10,
        idle_limit: 0,
    };
    let mut rx = subscribed(&addr, &format!("kudo/context/{}/", ctx.0)).await;
    let observer = events::observer(addr.clone());
    let setup2 = setup.clone();
    tokio::task::spawn_blocking(move || {
        hook::run_with(
            "session-start",
            r#"{"hook_event_name":"SessionStart","source":"startup"}"#,
            &setup2,
            T0,
            Some(observer),
        )
    })
    .await
    .unwrap();
    let got = next(&mut rx).await;
    assert_eq!(
        got.topic,
        format!("kudo/context/{}/lease-9/lease-start", ctx.0)
    );
    assert_eq!(got.payload, b"lease-start");
}

/// Enqueueing from the CLI publishes too, though no lease made it.
#[tokio::test(flavor = "multi_thread")]
async fn an_enqueue_is_published() {
    let addr = hub().await;
    let dir = std::env::temp_dir().join(format!("kudo-shim-events-enq-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let ctx = kudo_shim::cli::context_with(&dir, None, None, &args(&["new", "q"]), T0).unwrap();
    let ctx = ctx.trim().to_owned();
    let mut rx = subscribed(&addr, &format!("kudo/context/{ctx}/")).await;
    let (d, a, c) = (dir.clone(), addr.clone(), ctx.clone());
    tokio::task::spawn_blocking(move || {
        kudo_shim::cli::context_with(&d, None, Some(&a), &args(&["enqueue", &c, "the ask"]), T0)
    })
    .await
    .unwrap()
    .unwrap();
    let got = next(&mut rx).await;
    assert_eq!(got.topic, format!("kudo/context/{ctx}/enqueue/enqueued"));
}

/// A hub that accepts the connection and never answers costs a hook about the
/// budget per event, not the session.
#[test]
fn a_hub_that_hangs_costs_a_hook_about_the_budget() {
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = silent.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for socket in silent.incoming().flatten() {
            held.push(socket); // accept, never answer
        }
    });
    let dir = std::env::temp_dir().join(format!("kudo-shim-events-hang-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let ctx = store.create_context("hung", "# h\n", T0).unwrap();
    let setup = WorkerSetup {
        store_dir: dir,
        context: ctx,
        lease: "lease-h".into(),
        claim_ttl: 600,
        max_turns: 10,
        idle_limit: 0,
    };
    let started = std::time::Instant::now();
    let reply = hook::run_with(
        "session-start",
        r#"{"hook_event_name":"SessionStart"}"#,
        &setup,
        T0,
        Some(events::observer(addr)),
    );
    assert_eq!(reply.code, 0);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}
