//! The console server's state: the context store (with this machine's mirror)
//! and the hub (for nodes, and for the loop's events, which bump a version the
//! page waits on).
//!
//! implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::model::{ConsoleData, NodeView};

/// Bumped by every event the hub carries under `kudo/`, and by the console's
/// own writes.
static VERSION: AtomicU64 = AtomicU64::new(1);
static MOVED: OnceLock<tokio::sync::Notify> = OnceLock::new();
static FOLLOWING: OnceLock<()> = OnceLock::new();

fn moved() -> &'static tokio::sync::Notify {
    MOVED.get_or_init(tokio::sync::Notify::new)
}

fn bump() {
    VERSION.fetch_add(1, Ordering::Relaxed);
    moved().notify_waiters();
}

fn store_dir() -> Result<PathBuf, String> {
    kudo_shim::cli::default_store_dir()
        .ok_or_else(|| "cannot find the context store: set KUDO_CONTEXT_STORE".to_owned())
}

pub(crate) async fn console_data(selected: Option<String>) -> Result<ConsoleData, String> {
    follow_events();
    let dir = store_dir()?;
    let store =
        kudo_shim::cli::open_store(&dir, kudo_shim::cli::mirror_from_env().as_deref(), true)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    let contexts = crate::views::contexts(&store, now).map_err(|e| e.to_string())?;
    let selected = match selected.or_else(|| contexts.first().map(|c| c.id.clone())) {
        Some(id) => Some(crate::views::detail(&store, &id).map_err(|e| e.to_string())?),
        None => None,
    };
    Ok(ConsoleData {
        nodes: nodes().await,
        contexts,
        selected,
    })
}

/// The nodes the hub reaches; none when no hub is configured or it is down.
async fn nodes() -> Vec<NodeView> {
    let Some(addr) = kudo_shim::hub_addr_from_env() else {
        return Vec::new();
    };
    let Ok(Ok(hub)) =
        tokio::time::timeout(Duration::from_secs(2), kudo_shim::HubClient::dial(&addr)).await
    else {
        return Vec::new();
    };
    match hub.topology().await {
        Ok(snapshot) => snapshot
            .nodes
            .into_iter()
            .map(|n| NodeView {
                id: n.node.0,
                links: n.links.into_iter().map(|l| l.link.0).collect(),
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn enqueue(context: &str, ask: &str) -> Result<String, String> {
    let dir = store_dir()?;
    let said = kudo_shim::cli::context_with(
        &dir,
        kudo_shim::cli::mirror_from_env().as_deref(),
        kudo_shim::hub_addr_from_env().as_deref(),
        &["enqueue".to_owned(), context.to_owned(), ask.to_owned()],
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0),
    )?;
    bump();
    Ok(said)
}

pub(crate) async fn wait_version(after: u64) -> u64 {
    follow_events();
    let now = VERSION.load(Ordering::Relaxed);
    if now != after {
        return now;
    }
    let _ = tokio::time::timeout(Duration::from_secs(25), moved().notified()).await;
    VERSION.load(Ordering::Relaxed)
}

/// Subscribe once to the hub's `kudo/` events; each one bumps the version.
/// Re-subscribes when the subscription ends (hub restart, dropped).
fn follow_events() {
    if FOLLOWING.set(()).is_err() {
        return;
    }
    let Some(addr) = kudo_shim::hub_addr_from_env() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            let _ = follow_once(&addr).await;
            bump(); // whatever was missed meanwhile: re-read
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

async fn follow_once(addr: &str) -> Option<()> {
    use hub_protocol::HubEvent;
    use hub_protocol::service::ConsumerApiClient;
    let socket = tokio::net::TcpStream::connect(addr).await.ok()?;
    let consumer: ConsumerApiClient =
        vox::initiator_on(vox::transport::tcp::StreamLink::tcp(socket))
            .establish_connection()
            .await
            .ok()?
            .open_lane()
            .await
            .ok()?;
    let (tx, mut rx) = vox::channel::<HubEvent>();
    let subscription = tokio::spawn(async move {
        let _ = consumer.subscribe_events("kudo/".to_owned(), tx).await;
    });
    while let Ok(Some(_event)) = rx.recv().await {
        bump();
    }
    subscription.abort();
    Some(())
}
