//! The context store: a local kinora store holding contexts, items, claims,
//! settlements and submissions, each a kino.

use std::path::{Path, PathBuf};

use crate::{Claim, ClaimId, ContextId, Error, Head, ItemId, Now, Submission, SubmissionId};

/// A context store rooted at a directory.
pub struct ContextStore {
    #[allow(dead_code, reason = "phase 1 stub; the implementation reads it")]
    dir: PathBuf,
}

impl ContextStore {
    /// Create a new store in `dir` (which must not hold one yet).
    pub fn init(dir: &Path) -> Result<Self, Error> {
        Ok(Self { dir: dir.to_path_buf() })
    }

    /// Open an existing store in `dir`.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        Ok(Self { dir: dir.to_path_buf() })
    }

    /// Birth a context whose first head is `head`.
    pub fn create_context(&self, _title: &str, _head: &str, _now: Now) -> Result<ContextId, Error> {
        Err(Error::Kinora("not built yet".into()))
    }

    /// The context's current head.
    pub fn head(&self, ctx: &ContextId) -> Result<Head, Error> {
        Err(Error::UnknownContext(ctx.0.clone()))
    }

    /// Every version of the head, oldest first.
    pub fn head_history(&self, ctx: &ContextId) -> Result<Vec<Head>, Error> {
        Err(Error::UnknownContext(ctx.0.clone()))
    }

    /// Enqueue `ask` under the context.
    pub fn enqueue(&self, ctx: &ContextId, _ask: &str, _now: Now) -> Result<ItemId, Error> {
        Err(Error::UnknownContext(ctx.0.clone()))
    }

    /// Whether an item is ready to pull: not submitted, and not held by a live
    /// claim.
    pub fn has_ready(&self, ctx: &ContextId, _now: Now) -> Result<bool, Error> {
        Err(Error::UnknownContext(ctx.0.clone()))
    }

    /// Claim the oldest ready item for `lease`, with a deadline `ttl` seconds
    /// from `now`. `None` when nothing is ready.
    pub fn pull(
        &self,
        ctx: &ContextId,
        _lease: &str,
        _now: Now,
        _ttl: i64,
    ) -> Result<Option<Claim>, Error> {
        Err(Error::UnknownContext(ctx.0.clone()))
    }

    /// Submit the work for a live claim: the head is superseded by
    /// `submission.head`, the log entry is stored, and the claim is settled.
    pub fn submit(
        &self,
        claim: &ClaimId,
        _submission: &Submission,
        _now: Now,
    ) -> Result<SubmissionId, Error> {
        Err(Error::UnknownClaim(claim.0.clone()))
    }

    /// Give a live claim back: its item is ready again, as the next attempt.
    pub fn release(&self, claim: &ClaimId, _now: Now) -> Result<(), Error> {
        Err(Error::UnknownClaim(claim.0.clone()))
    }

    /// The context's log: its submissions' entries, oldest first.
    pub fn log(&self, ctx: &ContextId) -> Result<Vec<String>, Error> {
        Err(Error::UnknownContext(ctx.0.clone()))
    }
}
