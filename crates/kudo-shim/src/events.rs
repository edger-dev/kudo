//! The loop's events on the hub's event channel: published best effort.
//!
//! A hook or tool publishes after the write it reports has landed, gives the
//! hub half a second, and goes on without it: no hub, a hub that is down, a
//! publish refused — none of them fails the loop.
//!
//! implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d

use std::time::Duration;

use hub_protocol::HubEvent;
use hub_protocol::service::ConsumerApiClient;
use kudo_worker::Observer;

/// How long a publish may take before the loop goes on without it.
const BUDGET: Duration = Duration::from_millis(500);

/// The topic of one loop event: `kudo/context/<context>/<lease>/<event>`.
pub fn topic(context: &str, lease: &str, event: &str) -> String {
    format!("kudo/context/{context}/{lease}/{event}")
}

/// Publish one event to the hub at `addr`, best effort: whether the hub took
/// it within the budget. Safe from plain code and from inside a runtime.
pub fn publish(addr: &str, topic: String, payload: Vec<u8>) -> bool {
    let attempt = async {
        let socket = tokio::net::TcpStream::connect(addr).await.ok()?;
        let client: ConsumerApiClient =
            vox::initiator_on(vox::transport::tcp::StreamLink::tcp(socket))
                .establish_connection()
                .await
                .ok()?
                .open_lane()
                .await
                .ok()?;
        client.publish(HubEvent { topic, payload }).await.ok()
    };
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()
                    .and_then(|rt| {
                        rt.block_on(async { tokio::time::timeout(BUDGET, attempt).await })
                            .ok()
                    })
                    .flatten()
                    .is_some()
            })
            .join()
            .unwrap_or(false)
    })
}

/// An observer publishing each traced event to the hub at `addr`.
pub fn observer(addr: String) -> Observer {
    std::sync::Arc::new(move |context, lease, event| {
        let _ = publish(
            &addr,
            topic(&context.0, lease, event.name()),
            event.to_line().into_bytes(),
        );
    })
}

/// The observer for this machine's hub, when one is configured.
pub fn observer_from_env() -> Option<Observer> {
    crate::hub_addr_from_env().map(observer)
}
