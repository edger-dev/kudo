#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use hub::{HubListener, Peers, ServedHub};
use hub_protocol::SessionInfo;
use kudo_shim::HubClient;
use kudo_shim::server::{SessionSendArgs, ShimServer};

fn info(id: &str, node: &str) -> SessionInfo {
    SessionInfo {
        id: id.into(),
        node: node.into(),
        harness: "test".into(),
        workspace: format!("/work/{id}"),
        context: None,
    }
}

async fn eventually<F, Fut>(mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never held");
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_sessions_list_send_and_reply_across_hubs() {
    let public_listener = HubListener::bind("127.0.0.1:0").await.unwrap();
    let public_addr = public_listener.local_addr().to_string();
    let public = ServedHub::new()
        .with_auth_token("secret")
        .with_peers(Peers::new("hub").with_auth_token("secret"));
    let _public_server = tokio::spawn(public_listener.serve(public));
    let edger_listener = HubListener::bind("127.0.0.1:0").await.unwrap();
    let edger_addr = edger_listener.local_addr().to_string();
    let edger = ServedHub::new()
        .with_auth_token("secret")
        .with_peers(Peers::new("edger").with_auth_token("secret"));
    edger
        .peers()
        .add("hub", hub::tcp_dialer(&edger, "hub", public_addr.clone()))
        .unwrap();
    let _edger_server = tokio::spawn(edger_listener.serve(edger));

    let a = Arc::new(
        HubClient::dial_session_with_token(&edger_addr, "secret", info("a", "yolo"))
            .await
            .unwrap(),
    );
    let b = Arc::new(
        HubClient::dial_session_with_token(&public_addr, "secret", info("b", "public-node"))
            .await
            .unwrap(),
    );
    let server_a = ShimServer::new(a);
    let server_b = ShimServer::new(b);
    for server in [&server_a, &server_b] {
        for tool in ["session_list", "session_send", "session_messages"] {
            assert!(server.listed_tool_names().contains(&tool.to_string()));
        }
    }

    eventually(|| {
        let (server_a, server_b) = (server_a.clone(), server_b.clone());
        async move {
            let a = format!("{:?}", server_a.call_session_list().await);
            let b = format!("{:?}", server_b.call_session_list().await);
            a.contains("hub::b") && b.contains("edger::a")
        }
    })
    .await;

    let sent = server_a
        .call_session_send(SessionSendArgs {
            target: "hub::b".into(),
            body: "hello from a".into(),
            id: Some("m1".into()),
        })
        .await;
    assert_eq!(sent.is_error, Some(false), "{sent:?}");
    let received = format!("{:?}", server_b.call_session_messages().await);
    assert!(received.contains("hello from a"), "{received}");
    assert!(received.contains("edger::a"), "{received}");

    let replied = server_b
        .call_session_send(SessionSendArgs {
            target: "edger::a".into(),
            body: "reply from b".into(),
            id: Some("m2".into()),
        })
        .await;
    assert_eq!(replied.is_error, Some(false), "{replied:?}");
    let received = format!("{:?}", server_a.call_session_messages().await);
    assert!(received.contains("reply from b"), "{received}");
    assert!(received.contains("hub::b"), "{received}");

    let missing = server_a
        .call_session_send(SessionSendArgs {
            target: "hub::missing".into(),
            body: "nobody".into(),
            id: None,
        })
        .await;
    assert_eq!(missing.is_error, Some(true), "{missing:?}");
}
