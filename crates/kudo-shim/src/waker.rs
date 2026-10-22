//! The waker: once a minute, a context with an item that came due and no
//! worker running gets one, on its wake node, through the same launch policy
//! the console uses (`panes::start`). It wakes for due items only: an
//! ordinary item waits for a person.
//!
//! implements: 43643bef4da543e855bafcb2308401df3c7b2f1c5a4ef03ac69c06b7a4bd2d2a@f76f4215e264261e284792a84ffb31b5fbc7f3498b04b00dccf45aa79cac60a2

use std::sync::Arc;
use std::time::Duration;

use kudo_context::{ContextId, ContextStore, Error, Now, TraceEvent};

use crate::HubClient;

/// How long a pane the console (or the waker) started counts as a worker on
/// its way, before its CLI traces anything (it may sit at a startup prompt).
pub const PANE_GRACE: i64 = 15 * 60;
/// How often the waker looks.
pub const EVERY: Duration = Duration::from_secs(60);

/// The first wait before a context is woken again, and the longest.
pub const BACKOFF_FIRST: i64 = 15 * 60;
pub const BACKOFF_MAX: i64 = 4 * 3600;

/// How soon each context may be woken again. A woken worker that does not
/// answer its due item (stuck, ended, at a prompt) must not cost a session a
/// minute: each wake doubles the wait, up to [`BACKOFF_MAX`], until nothing
/// is due there.
#[derive(Debug, Default)]
pub struct Backoff {
    next: std::collections::HashMap<ContextId, (Now, i64)>,
    /// Finished panes already stopped (or found gone): looked at once.
    stopped: std::collections::HashSet<String>,
}

impl Backoff {
    /// Whether `ctx` may be woken at `now`.
    pub fn may_wake(&self, ctx: &ContextId, now: Now) -> bool {
        self.next.get(ctx).is_none_or(|(at, _)| now >= *at)
    }

    /// `ctx` was woken (or a wake was tried) at `now`.
    pub fn woke(&mut self, ctx: &ContextId, now: Now) {
        let wait = self
            .next
            .get(ctx)
            .map_or(BACKOFF_FIRST, |(_, wait)| (wait * 2).min(BACKOFF_MAX));
        self.next.insert(ctx.clone(), (now + wait, wait));
    }

    /// Nothing is due under `ctx` any more: the next wake starts afresh.
    pub fn settled(&mut self, ctx: &ContextId) {
        self.next.remove(ctx);
    }
}

/// The panes under `ctx` that never traced and are past [`PANE_GRACE`]: a
/// worker stuck at a startup prompt, or dead. Stopped before another starts.
pub fn stale_panes(
    store: &ContextStore,
    ctx: &ContextId,
    now: Now,
) -> Result<Vec<kudo_context::PaneRecord>, Error> {
    let traced: Vec<String> = store.trace(ctx)?.into_iter().map(|e| e.lease).collect();
    Ok(store
        .panes(ctx)?
        .into_iter()
        .filter(|p| now - p.at > PANE_GRACE && !traced.contains(&p.lease))
        .collect())
}

/// A worker to start: for which context, on which node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wake {
    pub context: ContextId,
    pub node: String,
}

/// The contexts to wake now: an item with a due time is ready, and no worker
/// runs — no lease traced, not ended, and heard from within `quiet_after`;
/// no pane started within [`PANE_GRACE`]. Each wakes on its wake node, else
/// on `own_node`.
pub fn due_contexts(
    store: &ContextStore,
    now: Now,
    own_node: &str,
    quiet_after: i64,
) -> Result<Vec<Wake>, Error> {
    due_contexts_with(store, now, own_node, quiet_after, &[])
}

/// [`due_contexts`], knowing which panes' jobs are already gone (killed by
/// hand): those no longer count as a worker on its way.
// implements: 43643bef4da543e855bafcb2308401df3c7b2f1c5a4ef03ac69c06b7a4bd2d2a
pub fn due_contexts_with(
    store: &ContextStore,
    now: Now,
    own_node: &str,
    quiet_after: i64,
    gone: &[String],
) -> Result<Vec<Wake>, Error> {
    let mut wakes = Vec::new();
    for card in store.contexts()? {
        let ctx = card.id;
        if store.due_ready_count(&ctx, now)? == 0
            || runs_a_worker(store, &ctx, now, quiet_after, gone)?
        {
            continue;
        }
        let node = store
            .wake_node(&ctx)?
            .unwrap_or_else(|| own_node.to_owned());
        wakes.push(Wake { context: ctx, node });
    }
    Ok(wakes)
}

/// Whether a worker runs under `ctx`: a live lease, or a pane just started.
fn runs_a_worker(
    store: &ContextStore,
    ctx: &ContextId,
    now: Now,
    quiet_after: i64,
    gone: &[String],
) -> Result<bool, Error> {
    // Each lease's last word, and whether it ended.
    let mut leases: Vec<(String, i64, bool)> = Vec::new();
    for entry in store.trace(ctx)? {
        let at = entry
            .at
            .parse::<jiff::Timestamp>()
            .map(|t| t.as_second())
            .unwrap_or(0);
        let ended = matches!(entry.event, TraceEvent::LeaseEnd(_));
        match leases
            .iter_mut()
            .find(|(lease, _, _)| *lease == entry.lease)
        {
            Some(slot) => {
                slot.1 = slot.1.max(at);
                slot.2 |= ended;
            }
            None => leases.push((entry.lease, at, ended)),
        }
    }
    if leases
        .iter()
        .any(|(_, last, ended)| !ended && now - last <= quiet_after)
    {
        return Ok(true);
    }
    Ok(store.panes(ctx)?.iter().any(|pane| {
        now - pane.at <= PANE_GRACE
            && !gone.contains(&pane.job)
            && !leases.iter().any(|(l, _, _)| *l == pane.lease)
    }))
}

/// Look once: start a worker for each context [`due_contexts`] names; what
/// happened, a line each.
pub async fn wake_once(
    hub: &HubClient,
    store: Arc<ContextStore>,
    now: Now,
    own_node: &str,
    quiet_after: i64,
    backoff: &mut Backoff,
) -> Vec<String> {
    let mut said = Vec::new();
    // What the store says about panes: the waker's finished ones, and the
    // recent ones still counting as a worker on its way.
    let panes = {
        let store = store.clone();
        tokio::task::spawn_blocking(move || -> Result<_, Error> {
            let mut finished = Vec::new();
            let mut recent = Vec::new();
            for card in store.contexts()? {
                // A day back is enough: older panes were looked at, or are
                // long gone.
                finished.extend(
                    finished_panes(&store, &card.id)?
                        .into_iter()
                        .filter(|p| now - p.at <= 86_400),
                );
                recent.extend(
                    store
                        .panes(&card.id)?
                        .into_iter()
                        .filter(|p| now - p.at <= PANE_GRACE),
                );
            }
            Ok((finished, recent))
        })
        .await
    };
    let (finished, recent) = match panes {
        Ok(Ok(panes)) => panes,
        Ok(Err(e)) => return vec![format!("could not read the store: {e}")],
        Err(e) => return vec![format!("could not read the store: {e}")],
    };
    // A finished worker's pane is stopped once; one already gone is let be.
    for pane in finished {
        if backoff.stopped.contains(&pane.job) {
            continue;
        }
        if job_running(hub, &pane).await {
            stop(hub, &pane).await;
            said.push(format!(
                "stopped pane {} on {} (its lease ended)",
                pane.job, pane.node
            ));
        }
        backoff.stopped.insert(pane.job);
    }
    // A recent pane whose job is gone (killed by hand) holds nothing back.
    let mut gone = Vec::new();
    for pane in recent {
        if !job_running(hub, &pane).await {
            gone.push(pane.job);
        }
    }
    let due = {
        let (store, own) = (store.clone(), own_node.to_owned());
        tokio::task::spawn_blocking(move || {
            due_contexts_with(&store, now, &own, quiet_after, &gone)
        })
        .await
    };
    let wakes = match due {
        Ok(Ok(wakes)) => wakes,
        Ok(Err(e)) => return vec![format!("could not read the store: {e}")],
        Err(e) => return vec![format!("could not read the store: {e}")],
    };
    // Contexts with nothing due any more start afresh next time.
    let waking: Vec<ContextId> = wakes.iter().map(|w| w.context.clone()).collect();
    backoff.next.retain(|ctx, _| waking.contains(ctx));
    for wake in wakes {
        if !backoff.may_wake(&wake.context, now) {
            continue;
        }
        backoff.woke(&wake.context, now);
        // A pane that never traced is stuck or dead: stop it first, so panes
        // do not pile up on the node.
        let stale = {
            let (store, ctx) = (store.clone(), wake.context.clone());
            tokio::task::spawn_blocking(move || stale_panes(&store, &ctx, now))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default()
        };
        for pane in stale {
            let _ = crate::jobs::kill(
                hub,
                crate::route::Target::node(pane.node.clone()),
                crate::jobs::DEFAULT_JOB_CONNECTOR,
                moco_job::wire::KillRequest {
                    id: pane.job.clone(),
                    caller: moco_job::wire::WireCaller::Console,
                },
            )
            .await;
            said.push(format!(
                "stopped pane {} on {} (never traced)",
                pane.job, pane.node
            ));
        }
        let line = match crate::panes::start_by(
            hub,
            store.clone(),
            &wake.context.0,
            &wake.node,
            now,
            "waker",
        )
        .await
        {
            Ok(pane) => format!(
                "woke a worker for {} on {} (job {}, lease {})",
                wake.context.0, wake.node, pane.job, pane.lease
            ),
            Err(e) => format!("could not wake {} on {}: {e}", wake.context.0, wake.node),
        };
        said.push(line);
    }
    said
}

/// The panes the waker started whose lease has ended: stopped, so a finished
/// worker's agent session and its MCP servers do not outlive the work. A pane
/// a person started is never among them.
// implements: 43643bef4da543e855bafcb2308401df3c7b2f1c5a4ef03ac69c06b7a4bd2d2a
pub fn finished_panes(
    store: &ContextStore,
    ctx: &ContextId,
) -> Result<Vec<kudo_context::PaneRecord>, Error> {
    let ended: Vec<String> = store
        .trace(ctx)?
        .into_iter()
        .filter(|e| matches!(e.event, TraceEvent::LeaseEnd(_)))
        .map(|e| e.lease)
        .collect();
    Ok(store
        .panes(ctx)?
        .into_iter()
        .filter(|p| p.by == "waker" && ended.contains(&p.lease))
        .collect())
}

/// The node woken workers default to: the waker's own when it may start
/// there, else the first node it may start on.
pub fn default_node(own: &str, allowed: &[String]) -> String {
    if allowed.iter().any(|n| n == own) {
        return own.to_owned();
    }
    allowed.first().cloned().unwrap_or_else(|| own.to_owned())
}

/// Whether a pane's job still runs on its node; a job the node no longer
/// knows, or cannot say, counts as not running.
async fn job_running(hub: &HubClient, pane: &kudo_context::PaneRecord) -> bool {
    crate::jobs::tail(
        hub,
        crate::route::Target::node(pane.node.clone()),
        crate::jobs::DEFAULT_JOB_CONNECTOR,
        moco_job::wire::TailRequest {
            id: pane.job.clone(),
            offset: u64::MAX,
        },
    )
    .await
    .is_ok_and(|t| !t.status.is_terminal())
}

/// Stop a pane's job, as the console.
async fn stop(hub: &HubClient, pane: &kudo_context::PaneRecord) {
    let _ = crate::jobs::kill(
        hub,
        crate::route::Target::node(pane.node.clone()),
        crate::jobs::DEFAULT_JOB_CONNECTOR,
        moco_job::wire::KillRequest {
            id: pane.job.clone(),
            caller: moco_job::wire::WireCaller::Console,
        },
    )
    .await;
}
