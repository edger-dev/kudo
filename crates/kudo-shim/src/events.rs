//! The loop's events on the hub's event channel: published best effort.
//!
//! implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d

use kudo_worker::Observer;

/// The topic of one loop event (stub).
pub fn topic(context: &str, lease: &str, event: &str) -> String {
    let _ = (context, lease, event);
    String::new()
}

/// Publish one event, best effort (stub).
pub fn publish(addr: &str, topic: String, payload: Vec<u8>) -> bool {
    let _ = (addr, topic, payload);
    false
}

/// An observer publishing each traced event to the hub at `addr` (stub).
pub fn observer(addr: String) -> Observer {
    let _ = addr;
    std::sync::Arc::new(|_, _, _| {})
}

/// The observer for this machine's hub, when one is configured.
pub fn observer_from_env() -> Option<Observer> {
    crate::hub_addr_from_env().map(observer)
}
