//! Starting a worker in a pane on a node — the launch policy every face that
//! starts one shares: what to run, under which lease, in which repo, and how
//! to tell a node that did not apply it.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

use kudo_context::{ContextStore, PaneRecord};
use moco_job::wire::{EnvVar, StartReply, StartRequest};

use crate::HubClient;
use crate::jobs::{DEFAULT_JOB_CONNECTOR, kill, start as start_job};
use crate::route::Target;

/// The start request for a worker pane: `kudo-shim worker <context>` (in its
/// repo when the context declares one on that node), in a terminal, bound by
/// the lease the launcher picked, the node's name as the hub knows it, and
/// the hub. No secrets: the node's own environment holds those.
pub fn worker_request(
    context: &str,
    repo: Option<&str>,
    lease: &str,
    node: &str,
    hub: Option<&str>,
) -> StartRequest {
    let mut argv = vec![
        "kudo-shim".to_owned(),
        "worker".to_owned(),
        context.to_owned(),
    ];
    if let Some(repo) = repo {
        argv.extend(["--repo".to_owned(), repo.to_owned()]);
    }
    let var = |name: &str, value: &str| EnvVar {
        name: name.to_owned(),
        value: value.to_owned(),
    };
    let mut env = vec![
        var(kudo_worker::env::GIVEN_LEASE_ENV, lease),
        var("KUDO_NODE", node),
    ];
    if let Some(hub) = hub {
        env.push(var(crate::HUB_ADDR_ENV, hub));
    }
    StartRequest {
        argv,
        cwd: "/tmp".to_owned(),
        deadline_ms: 0,
        caller: moco_job::wire::WireCaller::Console,
        env,
        human_view: moco_job::lens::HumanView::Terminal,
    }
}

/// Whether the node applied what was asked: an older node ignores fields it
/// does not know, and would run the worker unbound, in no terminal.
pub fn applied(request: &StartRequest, reply: &StartReply) -> bool {
    reply.human_view == request.human_view && reply.env_set as usize == request.env.len()
}

/// Start a worker for `context` (an id or unique prefix) in a pane on
/// `node`, and record the pane. A node that did not apply the request, or a
/// pane that could not be recorded, gets its job killed: no unbound worker
/// is left running unseen.
pub async fn start(
    hub: &HubClient,
    store: std::sync::Arc<ContextStore>,
    context: &str,
    node: &str,
    now: i64,
) -> Result<PaneRecord, String> {
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
    let request = worker_request(
        &ctx.0,
        repo.as_deref(),
        &lease,
        node,
        crate::hub_addr_from_env().as_deref(),
    );
    let target = Target::node(node.to_owned());
    let reply = start_job(hub, target.clone(), DEFAULT_JOB_CONNECTOR, request.clone())
        .await
        .map_err(|e| e.to_string())?;
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
    if !applied(&request, &reply) {
        return stop(format!(
            "node {node} is too old to start a worker pane (it did not apply the \
             environment or the terminal); its job was stopped"
        ))
        .await;
    }
    let pane = PaneRecord {
        lease,
        node: node.to_owned(),
        job: reply.id.clone(),
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
