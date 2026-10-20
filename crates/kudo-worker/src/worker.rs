//! The hook decisions for one worker: a lease bound to one context.

use kudo_context::{ContextId, ContextStore, Error, Now};

use crate::{LeaseBook, LeaseEnd};

/// What a worker is bound to and how far its lease may run.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// The one context this lease works.
    pub context: ContextId,
    /// The lease id (the session id the launcher gave the CLI).
    pub lease: String,
    /// Seconds a claim stays live without a submit.
    pub claim_ttl: i64,
    /// Items the Stop hook may hand over before the lease ends.
    pub max_turns: u32,
}

/// What the Stop hook tells the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// Keep the session going; `reason` becomes the next turn's prompt.
    Block { reason: String },
    /// Let the session end; the lease has ended for this reason.
    Allow(LeaseEnd),
}

/// The hook decisions over a context store and a lease book.
pub struct Worker<'a> {
    #[allow(dead_code, reason = "phase 1 stub")]
    store: &'a ContextStore,
    #[allow(dead_code, reason = "phase 1 stub")]
    book: &'a LeaseBook,
    #[allow(dead_code, reason = "phase 1 stub")]
    cfg: WorkerConfig,
}

impl<'a> Worker<'a> {
    pub fn new(store: &'a ContextStore, book: &'a LeaseBook, cfg: WorkerConfig) -> Self {
        Self { store, book, cfg }
    }

    /// SessionStart: the text injected into the session — the context's head
    /// and what a worker does.
    pub fn session_start(&self, _now: Now) -> Result<String, Error> {
        Ok(String::new())
    }

    /// Stop: hand over the next item of the bound context, or let the session
    /// end.
    pub fn stop(&self, _now: Now) -> Result<Stop, Error> {
        Ok(Stop::Allow(LeaseEnd::QueueEmpty))
    }

    /// PreCompact: the lease ends, and an open claim is released.
    pub fn pre_compact(&self, _now: Now) -> Result<(), Error> {
        Ok(())
    }

    /// UserPromptSubmit: a human took over; the lease ends, and an open claim
    /// is released.
    pub fn user_prompt(&self, _now: Now) -> Result<(), Error> {
        Ok(())
    }
}
