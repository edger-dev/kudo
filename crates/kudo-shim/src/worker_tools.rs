//! The worker's MCP tools — `context_read`, `pull`, `submit`, `release` — as
//! plain functions over the context store. The server wraps them; like the
//! hooks, they reach the store directly, never through the hub.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

use kudo_context::{ClaimId, ContextStore, Error, Now, Submission};
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
    let store = open(setup)?;
    let book = LeaseBook::new(&setup.leases_dir());
    let submission = Submission {
        head: head.to_owned(),
        log: log.to_owned(),
        base: base.map(|b| {
            b.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect()
        }),
    };
    let id = match Worker::new(&store, &book, setup.config()).submit(
        &ClaimId(claim.to_owned()),
        &submission,
        now,
    ) {
        Ok(id) => id,
        Err(Error::HeadMoved { current, .. }) => return Err(moved(&store, setup, claim, &current)),
        Err(e) => return Err(e.to_string()),
    };
    Ok(format!(
        "Submitted {} for claim {claim}; the head is superseded. End your turn now: \
         the next item, if any, is handed to you when you stop.",
        id.0
    ))
}

/// The refusal of a submit from a moved head: the head as it is now, and how
/// to build on it.
// implements: 3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab@3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab
fn moved(store: &ContextStore, setup: &WorkerSetup, claim: &str, current: &[String]) -> String {
    let heads = match store.heads(&setup.context) {
        Ok(heads) => heads,
        Err(e) => return e.to_string(),
    };
    let mut out = format!(
        "Not submitted: another worker superseded the head since claim {claim} was \
         taken. Your claim is still yours. The head as it is now:\n"
    );
    for head in &heads {
        out.push_str(&format!(
            "\n--- version {}\n{}\n",
            head.version,
            head.body.trim_end()
        ));
    }
    out.push_str(&format!(
        "\nFold what it established into your own new head, then call `submit` \
         again with the same claim and base \"{}\".",
        current.join(",")
    ));
    out
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
