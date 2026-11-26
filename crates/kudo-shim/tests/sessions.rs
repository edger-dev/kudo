#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use hub::{HubListener, Peers, ServedHub};
use hub_protocol::SessionInfo;
use kudo_shim::HubClient;
use kudo_shim::server::{SessionSendArgs, ShimServer};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

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

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    next_id: u64,
}

impl McpProcess {
    async fn spawn(state: &std::path::Path, addr: &str, session: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kudo-shim"))
            .env("KUDO_HUB_ADDR", addr)
            .env("HUB_AUTH_TOKEN", "secret")
            .env("KUDO_CONTEXT_STORE", state)
            .env("KUDO_SESSION_ID", session)
            .env("KUDO_AGENT_HARNESS", "kimi")
            .env("NODE_ID", "yolo")
            .current_dir(state)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut process = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        process
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2026-07-28",
                    "capabilities": {},
                    "clientInfo": {"name": "session-test", "version": "0"}
                }),
            )
            .await;
        process
            .stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await
            .unwrap();
        process
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            tokio::time::timeout(Duration::from_secs(5), self.stdout.read_line(&mut line))
                .await
                .unwrap()
                .unwrap();
            let reply: Value = serde_json::from_str(&line).unwrap();
            if reply["id"] == id {
                return reply["result"].clone();
            }
        }
    }

    async fn tool(&mut self, name: &str, arguments: Value) -> String {
        let result = self
            .request("tools/call", json!({"name": name, "arguments": arguments}))
            .await;
        result["content"][0]["text"].as_str().unwrap().to_owned()
    }

    async fn close(mut self) {
        self.child.kill().await.unwrap();
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
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

#[tokio::test(flavor = "multi_thread")]
async fn bare_mcp_processes_share_one_session_and_mailbox() {
    let listener = HubListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().to_string();
    let hub = ServedHub::new()
        .with_auth_token("secret")
        .with_peers(Peers::new("hub").with_auth_token("secret"));
    let _server = tokio::spawn(listener.serve(hub));
    let state = std::env::temp_dir().join(format!(
        "kudo-session-process-{}",
        kudo_worker::new_lease_id().unwrap()
    ));
    std::fs::create_dir_all(&state).unwrap();
    let a = McpProcess::spawn(&state, &addr, "shared").await;
    let b = McpProcess::spawn(&state, &addr, "shared").await;
    let other = McpProcess::spawn(&state, &addr, "other").await;

    let mut a = a;
    let mut b = b;
    let mut listed = String::new();
    for _ in 0..100 {
        listed = a.tool("session_list", json!({})).await;
        if listed.matches("shared").count() == 1 && listed.matches("other").count() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(listed.matches("shared").count(), 1, "{listed}");
    assert_eq!(listed.matches("other").count(), 1, "{listed}");
    assert!(listed.contains("kimi"), "{listed}");
    other.close().await;

    let sent = b
        .tool(
            "session_send",
            json!({"target": "shared", "body": "through the newest", "id": "m1"}),
        )
        .await;
    assert!(sent.starts_with("delivered"), "{sent}");
    let received = a.tool("session_messages", json!({})).await;
    assert!(received.contains("through the newest"), "{received}");
    assert!(received.contains("from shared"), "{received}");

    b.close().await;
    let mut sent = String::new();
    for _ in 0..100 {
        sent = a
            .tool(
                "session_send",
                json!({"target": "shared", "body": "through the survivor", "id": "m2"}),
            )
            .await;
        if sent.starts_with("delivered") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(sent.starts_with("delivered"), "{sent}");
    let received = a.tool("session_messages", json!({})).await;
    assert!(received.contains("through the survivor"), "{received}");
    a.close().await;
    let _ = std::fs::remove_dir_all(state);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_worker_mcp_registers_without_a_job_tool_call() {
    let listener = HubListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().to_string();
    let hub = ServedHub::new()
        .with_auth_token("secret")
        .with_peers(Peers::new("hub").with_auth_token("secret"));
    let _server = tokio::spawn(listener.serve(hub));
    let state = std::env::temp_dir().join(format!(
        "kudo-worker-session-{}",
        kudo_worker::new_lease_id().unwrap()
    ));
    std::fs::create_dir_all(&state).unwrap();
    let mut worker = std::process::Command::new(env!("CARGO_BIN_EXE_kudo-shim"))
        .env("KUDO_HUB_ADDR", &addr)
        .env("HUB_AUTH_TOKEN", "secret")
        .env("KUDO_CONTEXT_STORE", &state)
        .env("KUDO_CONTEXT", "context")
        .env("KUDO_LEASE", "worker-session")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let observer =
        HubClient::dial_session_with_token(&addr, "secret", info("observer", "observer-node"))
            .await
            .unwrap();
    eventually(|| {
        let observer = &observer;
        async move {
            observer
                .sessions()
                .await
                .unwrap()
                .iter()
                .any(|session| session.id == "worker-session")
        }
    })
    .await;
    worker.kill().unwrap();
    worker.wait().unwrap();
    let _ = std::fs::remove_dir_all(state);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_failure_refreshes_registration_for_an_explicit_retry() {
    let listener = HubListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().to_string();
    let hub = ServedHub::new().with_peers(Peers::new("hub"));
    let server = tokio::spawn(listener.serve(hub.clone()));
    let a = HubClient::dial_session_with_token(&addr, "secret", info("a", "node-a"))
        .await
        .unwrap()
        .with_timeout(Duration::from_secs(1));
    let b = HubClient::dial_session_with_token(&addr, "secret", info("b", "node-b"))
        .await
        .unwrap()
        .with_timeout(Duration::from_secs(1));

    server.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let listener = HubListener::bind(&addr).await.unwrap();
    let _server = tokio::spawn(listener.serve(hub));
    assert!(
        a.send_session("b".into(), "retry-id".into(), "after restart".into())
            .await
            .is_err()
    );
    eventually(|| {
        let b = &b;
        async move { b.sessions().await.is_ok() }
    })
    .await;
    a.send_session("b".into(), "retry-id".into(), "after restart".into())
        .await
        .unwrap();
    let messages = b.take_session_messages().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "after restart");
}
