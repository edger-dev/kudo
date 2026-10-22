//! The typed worker start, as it crosses the hub: what a launcher asks a
//! node for, and what comes back. The node composes everything else
//! (kudo-node's `worker`); these are the only values that travel.
//!
//! implements: kudo:5720302421d8748ea19b23da1f119f7c0e7514753b340ae5e425fa7e48588b41

use facet::Facet;

/// The connector a node offers worker starts under.
pub const WORKER_CONNECTOR: &str = "kudo-worker";
/// Its one method.
pub const START_WORKER: &str = "start_worker";

/// Start a worker for a context (a full 64-hex id), in a repo the context
/// declares on that node (a name; empty for none), under the lease the
/// launcher picked.
#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct WorkerStart {
    pub context: String,
    #[facet(default)]
    pub repo: String,
    pub lease: String,
}

/// The job the worker runs as.
#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct WorkerStarted {
    pub id: String,
}

/// Encode a worker-connector value.
pub fn encode<T: Facet<'static>>(value: &T) -> Result<Vec<u8>, String> {
    facet_json::to_string(value)
        .map(String::into_bytes)
        .map_err(|e| e.to_string())
}

/// Decode a worker-connector value.
pub fn decode<T: Facet<'static>>(bytes: &[u8]) -> Result<T, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    facet_json::from_str(text).map_err(|e| e.to_string())
}
