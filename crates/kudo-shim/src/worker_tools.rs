//! The worker's MCP tools — `context_read`, `pull`, `submit`, `release` — as
//! plain functions over the context store. The server wraps them; like the
//! hooks, they reach the store directly, never through the hub.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

use kudo_context::{ClaimId, ContextStore, Error, Now, Submission};
use kudo_worker::{LeaseBook, Pull, Worker, WorkerSetup};

fn open(setup: &WorkerSetup) -> Result<ContextStore, String> {
    crate::cli::open_store(
        &setup.store_dir,
        crate::cli::mirror_from_env().as_deref(),
        false,
    )
}

/// The worker for these tools, telling the hub of the loop's events when one
/// is configured.
fn worker<'a>(store: &'a ContextStore, book: &'a LeaseBook, setup: &WorkerSetup) -> Worker<'a> {
    let worker = Worker::new(store, book, setup.config());
    match crate::events::observer_from_env() {
        Some(observer) => worker.with_observer(observer),
        None => worker,
    }
}

/// The bound context's head.
pub fn context_read(setup: &WorkerSetup) -> Result<String, String> {
    let heads = open(setup)?
        .heads(&setup.context)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Context {}. {}",
        setup.context.0,
        kudo_worker::render_heads(&heads)
    ))
}

/// Claim the next item under the bound context for this lease — unless this
/// turn already submitted one, when the answer is to end the turn.
pub fn pull(setup: &WorkerSetup, now: Now) -> Result<String, String> {
    let store = open(setup)?;
    let book = LeaseBook::new(&setup.leases_dir());
    let worker = worker(&store, &book, setup);
    let pulled = worker.pull(now).map_err(|e| e.to_string())?;
    Ok(match pulled {
        Pull::Claimed(c) => format!(
            "Claimed, live for {}s. {}",
            setup.claim_ttl,
            worker.claim_text(&c).map_err(|e| e.to_string())?
        ),
        Pull::Nothing => format!("Nothing ready under context {}.", setup.context.0),
        Pull::EndTurn => "You already submitted an item this turn: end your turn now. If \
                          another item is ready it is handed to you as the next turn."
            .to_owned(),
    })
}

/// Submit the work for a claim: the typed submission, with the full new head.
/// The reply names any established line that landed as claimed.
// implements: 4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9@1b7ca6a98f08e1312c89a21ba22a046c2f9677965e58dd2fde0cae2756307a74
pub fn submit(
    setup: &WorkerSetup,
    claim: &str,
    submission: &Submission,
    now: Now,
) -> Result<String, String> {
    let store = open(setup)?;
    let book = LeaseBook::new(&setup.leases_dir());
    let done =
        match worker(&store, &book, setup).submit(&ClaimId(claim.to_owned()), submission, now) {
            Ok(done) => done,
            Err(Error::HeadMoved { current, .. }) => return Err(moved(&store, claim, &current)),
            Err(e) => return Err(e.to_string()),
        };
    let mut reply = format!(
        "Submitted {} for claim {claim}; the head is superseded.",
        done.id.0
    );
    if !done.downgraded.is_empty() {
        reply.push_str(
            "\n\nThese established lines landed as claimed — none of their evidence \
             resolved. Evidence is an http(s) URL, a kino in the ledger of a repo the \
             context declares, or an earlier submission's id; the queue's own ids \
             (claims, items, the context, heads) back nothing:",
        );
        for line in &done.downgraded {
            reply.push_str(&format!("\n- {line}"));
        }
    }
    if !done.unresolved.is_empty() {
        reply.push_str(&format!(
            "\n\nEvidence that did not resolve: {}.",
            done.unresolved.join(", ")
        ));
    }
    reply.push_str("\n\nEnd your turn now: the next item, if any, is handed to you when you stop.");
    Ok(reply)
}

/// The refusal of a submit from a moved head: the head as it is now, and how
/// to build on it.
// implements: 3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab@aaddfc2e7ee067ea5780ce6ee2dfe4587bfb47b0c067b2670a31308c019a7e21
fn moved(store: &ContextStore, claim: &str, current: &[String]) -> String {
    let heads = match current
        .iter()
        .map(|v| store.head_version(v))
        .collect::<Result<Vec<_>, _>>()
    {
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
    let store = open(setup)?;
    let book = LeaseBook::new(&setup.leases_dir());
    worker(&store, &book, setup)
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
