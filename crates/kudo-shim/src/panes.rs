//! Starting a worker in a pane on a node (stub).

use moco_job::wire::{StartReply, StartRequest};

/// The start request for a worker pane (stub).
pub fn worker_request(
    context: &str,
    repo: Option<&str>,
    lease: &str,
    node: &str,
    hub: Option<&str>,
) -> StartRequest {
    let _ = (context, repo, lease, node, hub);
    StartRequest {
        argv: vec![],
        cwd: String::new(),
        deadline_ms: 0,
        caller: moco_job::wire::WireCaller::Console,
        env: vec![],
        human_view: moco_job::lens::HumanView::Logs,
    }
}

/// Whether the node applied what was asked (stub).
pub fn applied(request: &StartRequest, reply: &StartReply) -> bool {
    let _ = (request, reply);
    true
}
