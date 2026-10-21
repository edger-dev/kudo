//! The console server's state: one context store (with this machine's
//! mirror), and the hub — for nodes, and for the loop's events, which bump a
//! version the page waits on.
//!
//! implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use dioxus::prelude::ServerFnError;
use kudo_context::ContextStore;

use crate::model::{ConsoleData, NodeView};

/// Bumped by every event the hub carries under `kudo/`, and by the console's
/// own writes.
static VERSION: AtomicU64 = AtomicU64::new(1);
static MOVED: OnceLock<tokio::sync::Notify> = OnceLock::new();
static FOLLOWING: OnceLock<()> = OnceLock::new();
/// One store for the server's life, so its mirror spacing and offline backoff
/// hold across requests.
static STORE: Mutex<Option<Arc<ContextStore>>> = Mutex::new(None);

fn moved() -> &'static tokio::sync::Notify {
    MOVED.get_or_init(tokio::sync::Notify::new)
}

fn bump() {
    VERSION.fetch_add(1, Ordering::Relaxed);
    moved().notify_waiters();
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Refuse what another web page could make a browser send.
pub(crate) async fn guarded() -> Result<(), ServerFnError> {
    let headers =
        dioxus::fullstack::FullstackContext::extract::<dioxus::fullstack::HeaderMap, _>().await?;
    crate::guard::allowed(&headers).map_err(ServerFnError::new)
}

fn store() -> Result<Arc<ContextStore>, String> {
    let mut held = STORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(store) = held.as_ref() {
        return Ok(store.clone());
    }
    let dir = kudo_shim::cli::default_store_dir()
        .ok_or_else(|| "cannot find the context store: set KUDO_CONTEXT_STORE".to_owned())?;
    let store = Arc::new(kudo_shim::cli::open_store(
        &dir,
        kudo_shim::cli::mirror_from_env().as_deref(),
        true,
    )?);
    *held = Some(store.clone());
    Ok(store)
}

pub(crate) async fn console_data(selected: Option<String>) -> ConsoleData {
    follow_events();
    // The store's reads (and its mirror's syncs) block: off the runtime.
    let read = tokio::task::spawn_blocking(move || {
        store().map(|store| crate::views::console(&store, selected, now()))
    })
    .await;
    let mut data = match read {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => ConsoleData {
            error: Some(e),
            ..ConsoleData::default()
        },
        Err(e) => ConsoleData {
            error: Some(e.to_string()),
            ..ConsoleData::default()
        },
    };
    data.nodes = nodes().await;
    data
}

/// The nodes the hub reaches; none when no hub is configured, it is down, or
/// it does not answer within two seconds.
async fn nodes() -> Vec<NodeView> {
    let Some(addr) = kudo_shim::hub_addr_from_env() else {
        return Vec::new();
    };
    let ask = async {
        let hub = kudo_shim::HubClient::dial(&addr).await.ok()?;
        hub.topology().await.ok()
    };
    match tokio::time::timeout(Duration::from_secs(2), ask).await {
        Ok(Some(snapshot)) => snapshot
            .nodes
            .into_iter()
            .map(|n| NodeView {
                id: n.node.0,
                links: n.links.into_iter().map(|l| l.link.0).collect(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) async fn enqueue(context: String, ask: String) -> Result<String, String> {
    let said = tokio::task::spawn_blocking(move || {
        let dir = kudo_shim::cli::default_store_dir()
            .ok_or_else(|| "cannot find the context store: set KUDO_CONTEXT_STORE".to_owned())?;
        kudo_shim::cli::context_with(
            &dir,
            kudo_shim::cli::mirror_from_env().as_deref(),
            kudo_shim::hub_addr_from_env().as_deref(),
            &["enqueue".to_owned(), context, ask],
            now(),
        )
    })
    .await
    .map_err(|e| e.to_string())??;
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
/// When an established subscription ends (hub restart, dropped for falling
/// behind) the version is bumped once — whatever was missed is re-read — and
/// it subscribes again. A hub that cannot subscribe at all (absent, too old
/// for events) is retried with backoff and bumps nothing: no hidden timer.
fn follow_events() {
    if FOLLOWING.set(()).is_err() {
        return;
    }
    let Some(addr) = kudo_shim::hub_addr_from_env() else {
        return;
    };
    tokio::spawn(async move {
        let mut wait = Duration::from_secs(2);
        loop {
            if follow_once(&addr).await {
                bump();
                wait = Duration::from_secs(2);
            } else {
                wait = (wait * 2).min(Duration::from_secs(60));
            }
            tokio::time::sleep(wait).await;
        }
    });
}

/// Follow one subscription until it ends; whether it was ever live.
async fn follow_once(addr: &str) -> bool {
    use hub_protocol::HubEvent;
    use hub_protocol::service::ConsumerApiClient;
    let Ok(socket) = tokio::net::TcpStream::connect(addr).await else {
        return false;
    };
    let Ok(connection) = vox::initiator_on(vox::transport::tcp::StreamLink::tcp(socket))
        .establish_connection()
        .await
    else {
        return false;
    };
    let Ok(consumer): Result<ConsumerApiClient, _> = connection.open_lane().await else {
        return false;
    };
    let (tx, mut rx) = vox::channel::<HubEvent>();
    let subscription = tokio::spawn(async move {
        let _ = consumer.subscribe_events("kudo/".to_owned(), tx).await;
    });
    // The hub's first event says the subscription is live.
    let live = matches!(
        tokio::time::timeout(Duration::from_secs(5), rx.recv()).await,
        Ok(Ok(Some(_)))
    );
    if live {
        while let Ok(Some(_event)) = rx.recv().await {
            bump();
        }
    }
    subscription.abort();
    live
}
