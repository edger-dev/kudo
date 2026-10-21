//! The hook decisions for one worker: a lease bound to one context.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

use kudo_context::{Claim, ClaimId, ContextId, ContextStore, Error, Now, Submission, Submitted};

use crate::lease::LeaseRecord;
use crate::{LeaseBook, LeaseEnd};

/// Stops refused for one open claim before the claim is given back.
const MAX_REFUSALS: u32 = 3;

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

/// What the `pull` tool gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pull {
    /// A claim on the next item.
    Claimed(Claim),
    /// Nothing is ready under the context.
    Nothing,
    /// This turn already submitted an item: end the turn, and the Stop hook
    /// hands over the next one. One item per turn keeps a clean point for a
    /// human to take over and for the open-claim guard to act.
    EndTurn,
}

/// The hook decisions over a context store and a lease book.
pub struct Worker<'a> {
    store: &'a ContextStore,
    book: &'a LeaseBook,
    cfg: WorkerConfig,
}

impl<'a> Worker<'a> {
    pub fn new(store: &'a ContextStore, book: &'a LeaseBook, cfg: WorkerConfig) -> Self {
        Self { store, book, cfg }
    }

    /// SessionStart: the text injected into the session — the context's head
    /// and what a worker does.
    pub fn session_start(&self, _now: Now) -> Result<String, Error> {
        let head = self.store.head(&self.cfg.context)?;
        Ok(format!(
            "You are a kudo worker under context {ctx}. Its head, as the last \
             worker left it:\n\n{body}\n\nWork exactly one item per turn. Call \
             `pull` to claim an item (unless one was handed to you), do it, call \
             `submit` with the claim id, the full new head (where / next / \
             established / open) and a log entry citing your evidence — then end \
             your turn. Do not pull again in the same turn: the next item is \
             handed to you when you stop.",
            ctx = self.cfg.context.0,
            body = head.body.trim_end(),
        ))
    }

    /// Stop: hand over the next item of the bound context, or let the session
    /// end.
    pub fn stop(&self, now: Now) -> Result<Stop, Error> {
        let mut record = self.record()?;
        if let Some(why) = record.ended {
            return Ok(Stop::Allow(why));
        }
        if let Some(open) = self
            .store
            .held_by(&self.cfg.context, &self.cfg.lease, now)?
        {
            // A session that cannot (or will not) submit would otherwise be
            // held open stop after stop until its turn limit: give the item
            // back after a few refusals instead.
            if record.refusals >= MAX_REFUSALS {
                self.store.release(&open.id, now)?;
                return self.end(record, LeaseEnd::Stuck);
            }
            record.refusals += 1;
            self.book.put(&self.cfg.lease, &record)?;
            return Ok(Stop::Block {
                reason: format!(
                    "Claim {} (\"{}\") is still open: submit or release it before stopping.",
                    open.id.0, open.item.ask
                ),
            });
        }
        if record.turns >= self.cfg.max_turns {
            return self.end(record, LeaseEnd::Budget);
        }
        let Some(claim) =
            self.store
                .pull(&self.cfg.context, &self.cfg.lease, now, self.cfg.claim_ttl)?
        else {
            return self.end(record, LeaseEnd::QueueEmpty);
        };
        record.turns += 1;
        record.refusals = 0;
        record.submitted_this_turn = false;
        self.book.put(&self.cfg.lease, &record)?;
        Ok(Stop::Block {
            reason: handoff(&claim),
        })
    }

    /// The `pull` tool: claim the next item, unless this turn already
    /// submitted one.
    pub fn pull(&self, now: Now) -> Result<Pull, Error> {
        if self.record()?.submitted_this_turn {
            return Ok(Pull::EndTurn);
        }
        Ok(
            match self
                .store
                .pull(&self.cfg.context, &self.cfg.lease, now, self.cfg.claim_ttl)?
            {
                Some(claim) => Pull::Claimed(claim),
                None => Pull::Nothing,
            },
        )
    }

    /// The `submit` tool: submit the claim's work, and mark the turn done.
    pub fn submit(
        &self,
        claim: &ClaimId,
        submission: &Submission,
        now: Now,
    ) -> Result<Submitted, Error> {
        let done = self.store.submit(claim, submission, now)?;
        let mut record = self.record()?;
        record.submitted_this_turn = true;
        self.book.put(&self.cfg.lease, &record)?;
        Ok(done)
    }

    /// PreCompact: the lease ends, and an open claim is released.
    pub fn pre_compact(&self, now: Now) -> Result<(), Error> {
        self.end_releasing(LeaseEnd::Compaction, now)
    }

    /// UserPromptSubmit: a human took over; the lease ends, and an open claim
    /// is released.
    pub fn user_prompt(&self, prompt: &str, now: Now) -> Result<(), Error> {
        if prompt.trim() == crate::KICKOFF {
            return Ok(()); // the launcher's own first prompt
        }
        self.end_releasing(LeaseEnd::Takeover, now)
    }

    fn end_releasing(&self, why: LeaseEnd, now: Now) -> Result<(), Error> {
        let record = self.record()?;
        if record.ended.is_some() {
            return Ok(());
        }
        if let Some(open) = self
            .store
            .held_by(&self.cfg.context, &self.cfg.lease, now)?
        {
            self.store.release(&open.id, now)?;
        }
        self.end(record, why).map(|_| ())
    }

    fn end(&self, mut record: LeaseRecord, why: LeaseEnd) -> Result<Stop, Error> {
        record.ended = Some(why);
        self.book.put(&self.cfg.lease, &record)?;
        Ok(Stop::Allow(why))
    }

    fn record(&self) -> Result<LeaseRecord, Error> {
        Ok(self.book.get(&self.cfg.lease)?)
    }
}

/// The next turn's prompt: the claimed item and how to hand it back.
fn handoff(claim: &Claim) -> String {
    format!(
        "Next item under context {ctx} — claim {id} (attempt {attempt}):\n\n{ask}\n\n\
         Work it, then call `submit` with claim id {id}, the full new head and a \
         log entry. Call `release` instead if you cannot finish it.",
        ctx = claim.context.0,
        id = claim.id.0,
        attempt = claim.attempt,
        ask = claim.item.ask.trim_end(),
    )
}
