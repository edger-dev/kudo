#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hub::{HubListener, ServedHub};
use hub_protocol::{Connector as ConnectorDescriptor, ConnectorKind, LinkLabel, NodeId};
use kudo_intake::{AdmissionRules, IntakeReply, Note, Receiver};
use kudo_node::intake::{INTAKE_CONNECTOR, IntakeConnector};
use kudo_shim::HubClient;
use kudo_shim::intake;
use node::{Connectors, HubEndpoint, NodeConfig};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "kudo-intake-e2e-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    let init = Command::new("git")
        .args(["init", "-b", "main"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(init.status.success());
    git(&path, &["config", "user.email", "kudo@example.org"]);
    git(&path, &["config", "user.name", "Kudo Test"]);
    kinora::init::init(&path, "https://example.com/intake.git").unwrap();
    let config_path = path.join(".kinora/config.styx");
    let text = std::fs::read_to_string(&config_path).unwrap();
    let mut config = kinora::config::Config::from_styx(&text).unwrap();
    config
        .roots
        .insert("inbox".into(), kinora::config::RootPolicy::Never);
    std::fs::write(&config_path, config.to_styx().unwrap()).unwrap();
    git(&path, &["add", ".kinora"]);
    git(&path, &["commit", "-m", "init"]);
    path
}

fn note(id: &str) -> Note {
    Note {
        id: id.into(),
        target: "kudo".into(),
        title: "A remote thought".into(),
        body: "Keep the receiver thin.".into(),
        attestation: true,
        author: "YJ Park".into(),
        machine: "work".into(),
        session: "session-123".into(),
        created_at: "2026-11-23T12:00:00Z".into(),
    }
}

async fn mesh(repo: &Path) -> (HubClient, vox::ConnectionHandle) {
    let listener = HubListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().to_string();
    let _hub = tokio::spawn(listener.serve(ServedHub::new().with_auth_token("secret")));
    let receiver = Receiver::new(BTreeMap::from([("kudo".into(), repo.to_path_buf())]));
    let rules = AdmissionRules {
        max_body_bytes: 262_144,
        sensitive_markers: vec![],
    };
    let mut connectors = Connectors::new();
    connectors.register(Box::new(IntakeConnector::new(receiver, rules)));
    let config = NodeConfig::new(
        NodeId("home".into()),
        LinkLabel::stable(),
        Some(HubEndpoint(format!("tcp://{addr}"))),
        vec![ConnectorDescriptor {
            id: INTAKE_CONNECTOR.into(),
            kind: ConnectorKind::Other("intake".into()),
        }],
        "test".into(),
    )
    .unwrap()
    .with_auth_token("secret");
    let socket = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let connection = node::serve_and_register_with(
        vox::transport::tcp::StreamLink::tcp(socket),
        &config,
        Arc::new(connectors),
    )
    .await
    .unwrap();
    let client = HubClient::dial_with_token(&addr, "secret").await.unwrap();
    (client, connection)
}

#[tokio::test(flavor = "multi_thread")]
async fn authenticated_delivery_deduplicates_and_lists() {
    let repo = repo();
    let (hub, _node) = mesh(&repo).await;
    let kino_id = match intake::send_note(&hub, "home", &note("note-1")).await.unwrap() {
        IntakeReply::Delivered { kino_id } => kino_id,
        other => panic!("expected delivery, got {other:?}"),
    };
    assert_eq!(
        intake::send_note(&hub, "home", &note("note-1")).await.unwrap(),
        IntakeReply::Duplicate {
            kino_id: kino_id.clone()
        }
    );
    match intake::list(&hub, "home", "kudo", "2026-11-23T00:00:00Z")
        .await
        .unwrap()
    {
        IntakeReply::Listed { notes } => {
            assert_eq!(notes.len(), 1);
            assert_eq!(notes[0].note_id, "note-1");
            assert_eq!(notes[0].kino_id, kino_id);
        }
        other => panic!("expected a listing, got {other:?}"),
    }

    let mut refused = note("note-2");
    refused.attestation = false;
    assert!(matches!(
        intake::send_note(&hub, "home", &refused).await,
        Ok(IntakeReply::Refused { .. })
    ));
}
