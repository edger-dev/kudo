//! The console server's reads: plain functions over the context store, so
//! everything a pane shows is computed here and tested without a browser.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

use kudo_context::{ContextStore, Error, Now, TraceEvent};

use crate::model::{ContextCard, ContextDetail, TraceRow, WorkerView};

/// Every context, newest first, with its counts and the leases still running
/// under it (a lease whose last traced event is not its end).
pub fn contexts(store: &ContextStore, now: Now) -> Result<Vec<ContextCard>, Error> {
    store
        .contexts()?
        .into_iter()
        .map(|c| {
            let mut last: Vec<(String, TraceEvent)> = Vec::new();
            for entry in store.trace(&c.id)? {
                match last.iter_mut().find(|(lease, _)| *lease == entry.lease) {
                    Some(slot) => slot.1 = entry.event,
                    None => last.push((entry.lease, entry.event)),
                }
            }
            let workers = last
                .into_iter()
                .filter(|(_, event)| !matches!(event, TraceEvent::LeaseEnd(_)))
                .map(|(lease, event)| WorkerView {
                    lease,
                    last: event.to_line(),
                    quiet: false,
                })
                .collect();
            Ok(ContextCard {
                ready: store.ready_count(&c.id, now)?,
                submitted: store.log(&c.id)?.len(),
                id: c.id.0,
                title: c.title,
                workers,
            })
        })
        .collect()
}

/// Contexts and the selected one, never failing as a whole (stub).
pub fn console(
    store: &ContextStore,
    selected: Option<String>,
    now: Now,
) -> crate::model::ConsoleData {
    let _ = (store, selected, now);
    crate::model::ConsoleData::default()
}

/// One context — by id or unique prefix — its head (the newest tip, flagged
/// when forked) and its trace, newest first.
pub fn detail(store: &ContextStore, id: &str) -> Result<ContextDetail, Error> {
    let ctx = store.resolve_context(id)?;
    let heads = store.heads(&ctx)?;
    let title = store
        .contexts()?
        .into_iter()
        .find(|c| c.id == ctx)
        .map(|c| c.title)
        .unwrap_or_default();
    let mut trace: Vec<TraceRow> = store
        .trace(&ctx)?
        .into_iter()
        .map(|t| TraceRow {
            at: t.at,
            lease: t.lease,
            seq: t.seq,
            line: t.event.to_line(),
        })
        .collect();
    trace.reverse();
    Ok(ContextDetail {
        id: ctx.0,
        title,
        head: heads.first().map(|h| h.body.clone()).unwrap_or_default(),
        forked: heads.len() > 1,
        trace,
    })
}
