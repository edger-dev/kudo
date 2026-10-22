//! The typed worker start: the one operation a governed node offers besides
//! the gated start.
//!
//! A caller asks for a worker on `{context, repo, lease}`; the node checks each
//! value's shape and composes the command, its environment and its terminal
//! itself, then starts it **vouched** — past the rule gate, because no caller
//! wrote a command line. Everything else a caller wants to run goes through
//! the gate.
//!
//! This is the one decision this composition root makes, and it makes it on
//! purpose: the values a worker start needs are kudo's, not the engine's.
//!
//! implements: kudo:5720302421d8748ea19b23da1f119f7c0e7514753b340ae5e425fa7e48588b41@f34266201d70f3802a59ab928778c8fbc78c14e8aa300c9c3c53301828ba8c1c

use std::sync::Arc;

use hub_protocol::{Connector as ConnectorDescriptor, ConnectorKind, NodeError};
use moco_job::lens::HumanView;
use moco_job::{JobRegistry, JobRequest, Scope};
use node::connector::Connector;

pub use kudo_worker::pane::{
    START_WORKER, WORKER_CONNECTOR, WorkerStart, WorkerStarted, decode, encode,
};

/// What the node fills in itself: the program, the start directory, its own
/// name as the hub knows it, and its hub (an address, no scheme).
#[derive(Debug, Clone)]
pub struct PaneSetup {
    pub program: String,
    /// Where the pane starts when its context declares no repo here: inside
    /// the node's allowed root, since a vouched start is still confined.
    pub cwd: std::path::PathBuf,
    pub node: String,
    pub hub: Option<String>,
}

fn is_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A lease id as `new_lease_id` mints it: lowercase 8-4-4-4-12 hex.
fn is_lease(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && is_hex(p))
}

/// A repo name: what `context repo add` accepts as a name, and nothing a
/// command line or a path could read as more.
fn is_repo(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !s.starts_with(['-', '.'])
}

/// The job a worker start becomes, or why its values were refused.
pub fn compose(ask: &WorkerStart, setup: &PaneSetup) -> Result<JobRequest, String> {
    if ask.context.len() != 64 || !is_hex(&ask.context) {
        return Err(format!(
            "context {:?} is not a full context id (64 lowercase hex)",
            ask.context
        ));
    }
    if !ask.repo.is_empty() && !is_repo(&ask.repo) {
        return Err(format!(
            "repo {:?} is not a repo name (letters, digits, - _ ., at most 64)",
            ask.repo
        ));
    }
    if !is_lease(&ask.lease) {
        return Err(format!("lease {:?} is not a lease id", ask.lease));
    }
    let mut argv = vec![
        setup.program.clone(),
        "worker".to_owned(),
        ask.context.clone(),
    ];
    if !ask.repo.is_empty() {
        argv.extend(["--repo".to_owned(), ask.repo.clone()]);
    }
    let mut env = vec![
        (
            kudo_worker::env::GIVEN_LEASE_ENV.to_owned(),
            ask.lease.clone(),
        ),
        ("KUDO_NODE".to_owned(), setup.node.clone()),
    ];
    if let Some(hub) = &setup.hub {
        env.push(("KUDO_HUB_ADDR".to_owned(), hub.clone()));
    }
    // The node's own: no session's workspace owns a pane the console started,
    // so none can stop it.
    Ok(JobRequest::new(argv, setup.cwd.clone())
        .with_env(env)
        .with_human_view(HumanView::Terminal)
        .in_scope(Scope::System))
}

/// Offers worker starts over the hub.
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

    fn call(&self, method: &str, payload: &[u8]) -> Result<Vec<u8>, NodeError> {
        if method != START_WORKER {
            return Err(NodeError::Rejected(format!(
                "unknown method `{method}` on {WORKER_CONNECTOR}; it answers {START_WORKER}"
            )));
        }
        let ask: WorkerStart =
            decode(payload).map_err(|e| NodeError::Rejected(format!("{START_WORKER}: {e}")))?;
        let job = compose(&ask, &self.setup).map_err(NodeError::Rejected)?;
        let id = self
            .registry
            .start_vouched(job)
            .map_err(|e| NodeError::Rejected(e.to_string()))?;
        encode(&WorkerStarted { id: id.0 }).map_err(NodeError::Rejected)
    }
}

/// The directory a pane starts in: `/tmp` when the allowed root holds it,
/// else the root itself.
pub fn pane_cwd(allowed_root: &std::path::Path) -> std::path::PathBuf {
    let tmp = std::path::Path::new("/tmp");
    if tmp.starts_with(allowed_root) {
        tmp.to_path_buf()
    } else {
        allowed_root.to_path_buf()
    }
}
