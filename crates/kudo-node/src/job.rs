//! The adapter: moco's job dispatch, offered as a hub connector.
//!
//! Deliberately almost nothing. It owns a registry, answers the daemon's
//! `Connector` trait, and hands every call straight to the engine's byte
//! dispatch. If this file ever grows a decision — a default, a retry, a
//! translation of one error into another — that decision has escaped the engine
//! and should be pushed back into it.
//!
//! implements: moco:3aa206d0a5ecd433ea159c78d3176934e98adef12bcca380b42ccf2b1ced591b

use std::sync::Arc;

use hub_protocol::{Connector as ConnectorDescriptor, ConnectorKind, NodeError};
use moco_job::JobRegistry;
use moco_job::wire;
use node::connector::Connector;

/// Offers a [`JobRegistry`] over the hub under a connector id.
pub struct JobConnector {
    id: String,
    registry: Arc<JobRegistry>,
}

impl JobConnector {
    /// Offer `registry` under the connector id `id`.
    pub fn new(id: impl Into<String>, registry: Arc<JobRegistry>) -> Self {
        Self {
            id: id.into(),
            registry,
        }
    }

    /// The registry this connector serves, for a caller that also drives it
    /// locally (the daemon that owns it).
    pub fn registry(&self) -> &Arc<JobRegistry> {
        &self.registry
    }
}

impl Connector for JobConnector {
    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: self.id.clone(),
            kind: ConnectorKind::ProcessManager,
        }
    }

    fn call(&self, method: &str, payload: &[u8]) -> Result<Vec<u8>, NodeError> {
        // Every failure here is a `Rejected`, including an unknown method.
        // `UnknownConnector` means *this link offers no connector by that id* —
        // a different claim entirely, and asserting it for a connector that
        // plainly answered would send the caller looking for the wrong problem.
        //
        // The engine's message is carried verbatim: it already names the
        // method, the binary, or the field at fault, and rewording it here
        // could only lose detail.
        wire::dispatch(&self.registry, method, payload)
            .map_err(|e| NodeError::Rejected(e.to_string()))
    }
}
