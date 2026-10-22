//! The typed worker start.
use std::sync::Arc;

use facet::Facet;
use hub_protocol::{Connector as ConnectorDescriptor, ConnectorKind, NodeError};
use moco_job::{JobRegistry, JobRequest};
use node::connector::Connector;

pub const WORKER_CONNECTOR: &str = "kudo-worker";
pub const START_WORKER: &str = "start_worker";

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct WorkerStart {
    pub context: String,
    #[facet(default)]
    pub repo: String,
    pub lease: String,
}

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct WorkerStarted {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct PaneSetup {
    pub program: String,
    pub node: String,
    pub hub: Option<String>,
}

pub fn encode<T: Facet<'static>>(value: &T) -> Result<Vec<u8>, String> {
    let _ = value;
    Err("not yet".into())
}

pub fn decode<T: Facet<'static>>(bytes: &[u8]) -> Result<T, String> {
    let _ = bytes;
    Err("not yet".into())
}

pub fn compose(_ask: &WorkerStart, _setup: &PaneSetup) -> Result<JobRequest, String> {
    Err("not yet".into())
}

pub struct WorkerConnector {
    registry: Arc<JobRegistry>,
    setup: PaneSetup,
}

impl WorkerConnector {
    pub fn new(registry: Arc<JobRegistry>, setup: PaneSetup) -> Self {
        Self { registry, setup }
    }
}

impl Connector for WorkerConnector {
    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: WORKER_CONNECTOR.to_owned(),
            kind: ConnectorKind::ProcessManager,
        }
    }

    fn call(&self, _method: &str, _payload: &[u8]) -> Result<Vec<u8>, NodeError> {
        let _ = (&self.registry, &self.setup);
        Err(NodeError::Rejected("not yet".into()))
    }
}
