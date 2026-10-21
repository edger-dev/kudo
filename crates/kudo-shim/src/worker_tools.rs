//! The worker's MCP tools — `context_read`, `pull`, `submit`, `release` — as
//! plain functions over the context store. The server wraps them; like the
//! hooks, they reach the store directly, never through the hub.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

use kudo_context::{ClaimId, ContextStore, Now, Submission};
use kudo_worker::{LeaseBook, Pull, Worker, WorkerSetup};

fn open(setup: &WorkerSetup) -> Result<ContextStore, String> {
    ContextStore::open(&setup.store_dir).map_err(|e| e.to_string())
}

/// The bound context's head.
pub fn context_read(setup: &WorkerSetup) -> Result<String, String> {
    let head = open(setup)?
        .head(&setup.context)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Head of context {} (version {}):\n\n{}",
        setup.context.0, head.version, head.body
    ))
}

/// Claim the next item under the bound context for this lease — unless this
/// turn already submitted one, when the answer is to end the turn.
pub fn pull(setup: &WorkerSetup, now: Now) -> Result<String, String> {
    let store = open(setup)?;
    let book = LeaseBook::new(&setup.leases_dir());
    let pulled = Worker::new(&store, &book, setup.config())
        .pull(now)
        .map_err(|e| e.to_string())?;
    Ok(match pulled {
        Pull::Claimed(c) => format!(
            "Claimed {} (attempt {}), live for {}s:\n\n{}\n\nSubmit with this claim id when done.",
            c.id.0,
            c.attempt,
            setup.claim_ttl,
            c.item.ask.trim_end()
        ),
        Pull::Nothing => format!("Nothing ready under context {}.", setup.context.0),
        Pull::EndTurn => "You already submitted an item this turn: end your turn now. If \
                          another item is ready it is handed to you as the next turn."
            .to_owned(),
    })
}

/// Submit the work for a claim: the full new head and a log entry.
pub fn submit(
    setup: &WorkerSetup,
    claim: &str,
    head: &str,
    log: &str,
    base: Option<&str>,
    now: Now,
) -> Result<String, String> {
    let _ = base;
    let store = open(setup)?;
    let book = LeaseBook::new(&setup.leases_dir());
    let submission = Submission {
        head: head.to_owned(),
        log: log.to_owned(),
        base: None,
    };
    let id = Worker::new(&store, &book, setup.config())
        .submit(&ClaimId(claim.to_owned()), &submission, now)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Submitted {} for claim {claim}; the head is superseded. End your turn now: \
         the next item, if any, is handed to you when you stop.",
        id.0
    ))
}

/// Give a claim back unfinished.
pub fn release(setup: &WorkerSetup, claim: &str, now: Now) -> Result<String, String> {
    open(setup)?
        .release(&ClaimId(claim.to_owned()), now)
        .map_err(|e| e.to_string())?;
    Ok(format!("Released claim {claim}; its item is ready again."))
}

/// File a rough edge in the context's inbox: one line, optionally a body.
pub fn inbox(
    setup: &WorkerSetup,
    line: &str,
    body: Option<&str>,
    now: Now,
) -> Result<String, String> {
    let id = open(setup)?
        .file_inbox(&setup.context, &setup.lease, line, body, now)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "filed {id} in the inbox of context {}",
        setup.context.0
    ))
}
