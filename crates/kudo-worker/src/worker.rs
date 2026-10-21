//! The hook decisions for one worker: a lease bound to one context.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

use kudo_context::{
    Claim, ClaimId, ContextId, ContextStore, Error, Now, Submission, Submitted, TraceEvent,
};

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
    /// Seconds the lease may wait in a row with nothing ready; `0` ends it
    /// as soon as nothing is.
    pub idle_limit: i64,
}

/// What the Stop hook tells the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// Keep the session going; `reason` becomes the next turn's prompt.
    Block { reason: String },
    /// Let the session end; the lease has ended for this reason.
    Allow(LeaseEnd),
    /// Nothing is ready, and the lease may wait: ask again shortly.
    Wait,
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

/// Told of every loop event the worker traces: the context, the lease, the
/// event.
pub type Observer = std::sync::Arc<dyn Fn(&ContextId, &str, &TraceEvent) + Send + Sync>;

/// The hook decisions over a context store and a lease book.
pub struct Worker<'a> {
    store: &'a ContextStore,
    book: &'a LeaseBook,
    cfg: WorkerConfig,
    observer: Option<Observer>,
}

impl<'a> Worker<'a> {
    pub fn new(store: &'a ContextStore, book: &'a LeaseBook, cfg: WorkerConfig) -> Self {
        Self {
            store,
            book,
            cfg,
            observer: None,
        }
    }

    /// Tell `observer` of every loop event this worker traces, after the
    /// trace records it.
    pub fn with_observer(mut self, observer: Observer) -> Self {
        self.observer = Some(observer);
        self
    }

    /// SessionStart: the text injected into the session — the context's head
    /// and what a worker does.
    pub fn session_start(&self, now: Now) -> Result<String, Error> {
        let heads = self.store.heads(&self.cfg.context)?;
        let mut record = self.record()?;
        if record.ended.is_none() && !record.started {
            record.started = true;
            self.book.put(&self.cfg.lease, &record)?;
            self.trace(&TraceEvent::LeaseStart, now);
        }
        Ok(format!(
            "You are a kudo worker under context {ctx}. {body}\n\nWork exactly one item per turn. Call \
             `pull` to claim an item (unless one was handed to you), do it, call \
             `submit` with the claim id, the full new head (where / next / \
             established / open), a short log, and what you established — each \
             line with its evidence (kino ids or http(s) URLs; a line without \
             evidence that resolves lands as claimed) — what you ruled out and \
             what you could not see; then end your turn. Do not pull again in the \
             same turn: the next item is handed to you when you stop.",
            ctx = self.cfg.context.0,
            body = render_heads(&heads).trim_end(),
        ))
    }

    /// A claim as the worker is handed it: the ask, and the head the claim
    /// was taken from — what the new head must build on.
    pub fn claim_text(&self, claim: &Claim) -> Result<String, Error> {
        let heads = if claim.base.is_empty() {
            self.store.heads(&self.cfg.context)?
        } else {
            claim
                .base
                .iter()
                .map(|v| self.store.head_version(v))
                .collect::<Result<Vec<_>, Error>>()?
        };
        Ok(format!(
            "Claim {id} (attempt {attempt}) under context {ctx}:\n\n{ask}\n\n{heads}\n\n\
             Work it, then call `submit` with claim id {id}, the full new head built \
             on the one above, a short log and what you established with its \
             evidence. Call `release` instead if you cannot finish it.",
            id = claim.id.0,
            attempt = claim.attempt,
            ctx = claim.context.0,
            ask = claim.item.ask.trim_end(),
            heads = render_heads(&heads).trim_end(),
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
                self.trace(&TraceEvent::Release(open.id.clone()), now);
                return self.end(record, LeaseEnd::Stuck, now);
            }
            record.refusals += 1;
            self.book.put(&self.cfg.lease, &record)?;
            self.trace(&TraceEvent::Refused(open.id.clone()), now);
            return Ok(Stop::Block {
                reason: format!(
                    "Claim {} (\"{}\") is still open: submit or release it before stopping.",
                    open.id.0, open.item.ask
                ),
            });
        }
        if record.turns >= self.cfg.max_turns {
            return self.end(record, LeaseEnd::Budget, now);
        }
        let Some(claim) =
            self.store
                .pull(&self.cfg.context, &self.cfg.lease, now, self.cfg.claim_ttl)?
        else {
            // implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed
            let since = record.idle_since.unwrap_or(now);
            if now - since >= self.cfg.idle_limit {
                return self.end(record, LeaseEnd::QueueEmpty, now);
            }
            if record.idle_since.is_none() {
                record.idle_since = Some(now);
                self.book.put(&self.cfg.lease, &record)?;
                self.trace(&TraceEvent::Wait, now);
            }
            return Ok(Stop::Wait);
        };
        record.idle_since = None;
        record.turns += 1;
        record.refusals = 0;
        record.submitted_this_turn = false;
        self.book.put(&self.cfg.lease, &record)?;
        self.trace(&TraceEvent::Handoff(claim.id.clone()), now);
        Ok(Stop::Block {
            reason: format!("Next item. {}", self.claim_text(&claim)?),
        })
    }

    /// The Stop hook's answer, waiting up to `window` seconds for an item
    /// (`pause` between looks): never [`Stop::Wait`]. A window with nothing
    /// ends the turn with a short idle reply, so the next Stop waits again.
    // implements: 9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed@9680c1e9f147ea4dba7d85a17e7c2405f6cd5b4bb13e87dd81849d986e06dfed
    pub fn stop_waiting(
        &self,
        clock: impl Fn() -> Now,
        pause: impl Fn(),
        window: i64,
    ) -> Result<Stop, Error> {
        let deadline = clock() + window;
        loop {
            match self.stop(clock())? {
                Stop::Wait if clock() >= deadline => return Ok(self.idle_turn(clock())),
                Stop::Wait => pause(),
                other => return Ok(other),
            }
        }
    }

    /// A turn with nothing to do: traced as a wait (a live waiter never
    /// looks quiet), and a reply asked for in one line.
    fn idle_turn(&self, now: Now) -> Stop {
        self.trace(&TraceEvent::Wait, now);
        Stop::Block {
            reason: format!(
                "Nothing is ready under context {} yet. Reply with one short line and end \
                 your turn; the next item is handed to you when it is queued.",
                self.cfg.context.0
            ),
        }
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
                Some(claim) => {
                    self.trace(&TraceEvent::Pull(claim.id.clone()), now);
                    Pull::Claimed(claim)
                }
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
        let done = self
            .store
            .submit_as(&self.cfg.lease, claim, submission, now)?;
        let mut record = self.record()?;
        record.submitted_this_turn = true;
        self.book.put(&self.cfg.lease, &record)?;
        self.trace(
            &TraceEvent::Submit {
                claim: claim.clone(),
                submission: done.id.clone(),
            },
            now,
        );
        Ok(done)
    }

    /// The `release` tool: give the claim back unfinished.
    pub fn release(&self, claim: &ClaimId, now: Now) -> Result<(), Error> {
        self.store.release_as(&self.cfg.lease, claim, now)?;
        self.trace(&TraceEvent::Release(claim.clone()), now);
        Ok(())
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
            self.release(&open.id, now)?;
        }
        self.end(record, why, now).map(|_| ())
    }

    fn end(&self, mut record: LeaseRecord, why: LeaseEnd, now: Now) -> Result<Stop, Error> {
        record.ended = Some(why);
        self.book.put(&self.cfg.lease, &record)?;
        self.trace(&TraceEvent::LeaseEnd(why.as_str().to_owned()), now);
        Ok(Stop::Allow(why))
    }

    /// Record one loop event of this lease. Best effort: it runs after the
    /// change it records has happened, so a failed write must not undo the
    /// hook's answer — it is reported on stderr and the loop goes on.
    // implements: a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098@a6751902e8e01e5d7d5983196da9170f33f40d0d74246b5746977ea1c5e45098
    fn trace(&self, event: &TraceEvent, now: Now) {
        if let Err(e) = self
            .store
            .record_trace(&self.cfg.context, &self.cfg.lease, event, now)
        {
            eprintln!("kudo: could not trace {event:?}: {e}");
            return; // the channel says only what the trace holds
        }
        // implements: 03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d@03045b42c8f5eb9f6b68ffe4b372e8560b43d4da21c4a77c6ddab998713b7c6d
        if let Some(observe) = &self.observer {
            observe(&self.cfg.context, &self.cfg.lease, event);
        }
    }

    fn record(&self) -> Result<LeaseRecord, Error> {
        Ok(self.book.get(&self.cfg.lease)?)
    }
}

/// The head as a worker is shown it: every tip, newest first, each with its
/// version.
pub fn render_heads(heads: &[kudo_context::Head]) -> String {
    match heads {
        [one] => format!(
            "The context's head (version {}):\n\n{}\n",
            one.version,
            one.body.trim_end()
        ),
        many => {
            let mut out = format!(
                "The context's head is forked: two submits raced, leaving {} tips, \
                 newest first. Your new head must fold them together.\n",
                many.len()
            );
            for head in many {
                out.push_str(&format!(
                    "\n--- version {}\n{}\n",
                    head.version,
                    head.body.trim_end()
                ));
            }
            out
        }
    }
}
