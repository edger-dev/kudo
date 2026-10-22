//! The console server's reads: plain functions over the context store, so
//! everything a pane shows is computed here and tested without a browser.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29

use kudo_context::{ContextStore, Error, Now, TraceEvent};

use crate::model::{ConsoleData, ContextCard, ContextDetail, PaneRef, TraceRow, WorkerView};

/// A lease silent for longer than a claim lives is likely dead (killed,
/// crashed, its machine asleep): shown as quiet, never as working.
const QUIET_AFTER: i64 = kudo_worker::DEFAULT_CLAIM_TTL;
/// A pane that never traced: quiet after this (stuck at a prompt, or dead)…
// implements: 0c4e36f264b2b4fbc6fcfee93bc02ad5b8419a1d21a8638bf080623ab942a37d@0c4e36f264b2b4fbc6fcfee93bc02ad5b8419a1d21a8638bf080623ab942a37d
const STARTING_QUIET_AFTER: i64 = 600;
/// …and no longer listed after this.
const STARTING_GONE_AFTER: i64 = 86_400;

/// Every context, newest first, with its counts and the leases still running
/// under it (a lease whose last traced event is not its end).
pub fn contexts(store: &ContextStore, now: Now) -> Result<Vec<ContextCard>, Error> {
    store
        .contexts()?
        .into_iter()
        .map(|c| {
            let mut last: Vec<(String, TraceEvent, String)> = Vec::new();
            // A lease that ended stays ended, whatever it traces after (a
            // job a human-driven session ran, say).
            let mut ended: Vec<String> = Vec::new();
            for entry in store.trace(&c.id)? {
                if matches!(entry.event, TraceEvent::LeaseEnd(_)) {
                    ended.push(entry.lease.clone());
                }
                match last.iter_mut().find(|(lease, _, _)| *lease == entry.lease) {
                    Some(slot) => {
                        slot.1 = entry.event;
                        slot.2 = entry.at;
                    }
                    None => last.push((entry.lease, entry.event, entry.at)),
                }
            }
            let panes = store.panes(&c.id)?;
            let mut starting: Vec<(String, bool)> = Vec::new();
            let pane_of = |lease: &str| {
                panes.iter().rfind(|p| p.lease == lease).map(|p| PaneRef {
                    node: p.node.clone(),
                    job: p.job.clone(),
                })
            };
            // A pane started but not traced yet (its CLI at a startup
            // prompt) is a worker too: starting, and reachable to take over.
            // Silent for minutes it is stuck or dead: quiet, then gone.
            for pane in &panes {
                let age = now - pane.at;
                if !last.iter().any(|(lease, _, _)| *lease == pane.lease)
                    && age < STARTING_GONE_AFTER
                {
                    starting.push((pane.lease.clone(), age >= STARTING_QUIET_AFTER));
                }
            }
            let workers = last
                .into_iter()
                .filter(|(lease, event, _)| {
                    !matches!(event, TraceEvent::LeaseEnd(_)) && !ended.contains(lease)
                })
                .map(|(lease, event, at)| WorkerView {
                    last: event.to_line(),
                    quiet: at
                        .parse::<jiff::Timestamp>()
                        .map(|t| now - t.as_second() > QUIET_AFTER)
                        .unwrap_or(false),
                    pane: pane_of(&lease),
                    lease,
                })
                .chain(starting.iter().map(|(lease, quiet)| WorkerView {
                    lease: lease.clone(),
                    last: "starting".to_owned(),
                    quiet: *quiet,
                    pane: pane_of(lease),
                }))
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

/// Contexts and the selected one (the newest when none is), never failing as
/// a whole: what could not be read is said in `error`, and the rest still
/// shows — so the *where* pane, the way to a different selection, stays.
pub fn console(store: &ContextStore, selected: Option<String>, now: Now) -> ConsoleData {
    let mut data = ConsoleData::default();
    match contexts(store, now) {
        Ok(contexts) => data.contexts = contexts,
        Err(e) => data.error = Some(e.to_string()),
    }
    if let Some(id) = selected.or_else(|| data.contexts.first().map(|c| c.id.clone())) {
        match detail(store, &id) {
            Ok(detail) => data.selected = Some(detail),
            Err(e) => data.error = Some(format!("{id}: {e}")),
        }
    }
    data
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

pub fn approvals(
    _pending: Vec<(String, moco_job::wire::PendingJob)>,
    _asked: &std::collections::HashMap<(String, String), String>,
) -> Vec<crate::model::ApprovalView> {
    Vec::new()
}

pub fn job_contexts(
    _store: &ContextStore,
) -> Result<std::collections::HashMap<(String, String), String>, Error> {
    Ok(Default::default())
}

pub fn verdict_from(_name: &str) -> Option<moco_job::wire::WireVerdict> {
    None
}
