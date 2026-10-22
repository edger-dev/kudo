//! Starting a worker in a pane on a node — the launch policy every face that
//! starts one shares: what to run, under which lease, in which repo, and how
//! to tell a node that did not apply it.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

use kudo_context::{ContextStore, PaneRecord};
use kudo_worker::pane::WorkerStart;

use crate::HubClient;
use crate::jobs::{DEFAULT_JOB_CONNECTOR, kill, start_worker};
use crate::route::Target;

/// The nodes the console may start workers on, comma-separated.
// implements: 2f073c122047d13470407233fd0095dd86408b1ebc66ed6f136c3f3b7027b2c5@2f073c122047d13470407233fd0095dd86408b1ebc66ed6f136c3f3b7027b2c5
pub const WORKER_NODES_ENV: &str = "KUDO_CONSOLE_WORKER_NODES";

/// The nodes named in [`WORKER_NODES_ENV`]; `None` when it is unset (any
/// node).
pub fn worker_nodes_from(lookup: impl Fn(&str) -> Option<String>) -> Option<Vec<String>> {
    lookup(WORKER_NODES_ENV).map(|list| {
        list.split(',')
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .collect()
    })
}

/// Whether a worker may start on `node`: any node when none are named.
pub fn may_start(node: &str, named: Option<&[String]>) -> bool {
    named.is_none_or(|named| named.iter().any(|n| n == node))
}

/// What a worker pane is asked for: the context's full id, its repo on that
/// node (if it declares one), and the lease the launcher picked. The node
/// composes the command, its environment and its terminal from these
/// (kudo_node::worker); no command line, environment or secret travels.
// implements: 5720302421d8748ea19b23da1f119f7c0e7514753b340ae5e425fa7e48588b41@f34266201d70f3802a59ab928778c8fbc78c14e8aa300c9c3c53301828ba8c1c
pub fn worker_ask(context: &str, repo: Option<&str>, lease: &str) -> WorkerStart {
    WorkerStart {
        context: context.to_owned(),
        repo: repo.unwrap_or_default().to_owned(),
        lease: lease.to_owned(),
    }
}

/// What a failed worker start says: a node that does not offer the worker
/// connector at all is too old and needs redeploying; any other refusal — a
/// value the node refused, a spawn that failed — is reported as it came.
pub fn start_failure(node: &str, error: &crate::ShimError) -> String {
    match error {
        crate::ShimError::Route {
            side: crate::route::RouteFailure::Node,
            detail,
            ..
        } if detail == "UnknownConnector" => format!(
            "node {node} does not offer worker starts; it is too old for a governed \
             worker pane — redeploy it"
        ),
        other => other.to_string(),
    }
}

/// Start a worker for `context` (an id or unique prefix) in a pane on
/// `node`, and record the pane. A node without the worker operation is
/// refused as too old; a pane that could not be recorded gets its job killed:
/// no worker is left running unseen.
pub async fn start(
    hub: &HubClient,
    store: std::sync::Arc<ContextStore>,
    context: &str,
    node: &str,
    now: i64,
) -> Result<PaneRecord, String> {
    let named = worker_nodes_from(|k| std::env::var(k).ok());
    if !may_start(node, named.as_deref()) {
        let named = named.unwrap_or_default();
        return Err(if named.is_empty() {
            format!("workers start on no node ({WORKER_NODES_ENV} is empty)")
        } else {
            format!(
                "workers start only on {} (set in {WORKER_NODES_ENV})",
                named.join(", ")
            )
        });
    }
    let (ctx, repo) = {
        let (store, context, node) = (store.clone(), context.to_owned(), node.to_owned());
        tokio::task::spawn_blocking(move || {
            let ctx = store.resolve_context(&context).map_err(|e| e.to_string())?;
            let repo = store
                .repos(&ctx)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|r| r.node == node)
                .map(|r| r.name);
            Ok::<_, String>((ctx, repo))
        })
        .await
        .map_err(|e| e.to_string())??
    };
    let lease = kudo_worker::new_lease_id().map_err(|e| e.to_string())?;
    let target = Target::node(node.to_owned());
    let reply = start_worker(
        hub,
        target.clone(),
        worker_ask(&ctx.0, repo.as_deref(), &lease),
    )
    .await
    .map_err(|e| start_failure(node, &e))?;
    let stop = |why: String| {
        let (hub, target, id) = (hub, target.clone(), reply.id.clone());
        async move {
            let _ = kill(
                hub,
                target,
                DEFAULT_JOB_CONNECTOR,
                moco_job::wire::KillRequest {
                    id,
                    caller: moco_job::wire::WireCaller::Console,
                },
            )
            .await;
            Err(why)
        }
    };
    let pane = PaneRecord {
        lease,
        node: node.to_owned(),
        job: reply.id.clone(),
        at: now,
        by: "console".to_owned(),
    };
    let recorded = {
        let (store, ctx, pane) = (store.clone(), ctx.clone(), pane.clone());
        tokio::task::spawn_blocking(move || {
            store.record_pane(&ctx, &pane.lease, &pane.node, &pane.job, now)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()))
    };
    if let Err(e) = recorded {
        return stop(format!(
            "the pane could not be recorded ({e}); its job was stopped"
        ))
        .await;
    }
    Ok(pane)
}
