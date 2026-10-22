//! The waker: once a minute, a context with an item that came due and no
//! worker running gets one, on its wake node, through the same launch policy
//! the console uses (`panes::start`). It wakes for due items only: an
//! ordinary item waits for a person.
//!
//! implements: 43643bef4da543e855bafcb2308401df3c7b2f1c5a4ef03ac69c06b7a4bd2d2a

use std::sync::Arc;
use std::time::Duration;

use kudo_context::{ContextId, ContextStore, Error, Now, TraceEvent};

use crate::HubClient;

/// How long a pane the console (or the waker) started counts as a worker on
/// its way, before its CLI traces anything (it may sit at a startup prompt).
pub const PANE_GRACE: i64 = 15 * 60;
/// How often the waker looks.
pub const EVERY: Duration = Duration::from_secs(60);

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
    let mut wakes = Vec::new();
    for card in store.contexts()? {
        let ctx = card.id;
        if store.due_ready_count(&ctx, now)? == 0 || runs_a_worker(store, &ctx, now, quiet_after)? {
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
        now - pane.at <= PANE_GRACE && !leases.iter().any(|(l, _, _)| *l == pane.lease)
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
) -> Vec<String> {
    let due = {
        let (store, own) = (store.clone(), own_node.to_owned());
        tokio::task::spawn_blocking(move || due_contexts(&store, now, &own, quiet_after)).await
    };
    let wakes = match due {
        Ok(Ok(wakes)) => wakes,
        Ok(Err(e)) => return vec![format!("could not read the store: {e}")],
        Err(e) => return vec![format!("could not read the store: {e}")],
    };
    let mut said = Vec::new();
    for wake in wakes {
        let line =
            match crate::panes::start(hub, store.clone(), &wake.context.0, &wake.node, now).await {
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
