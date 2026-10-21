//! kudo-context — the context engine.
//!
//! A **context** is the unit of memory a worker is bound to: a superseded head
//! (where / next / established / open), its log of submissions, the repos it
//! spans, and its queue of items
//! (`kudo:6f2cfc2264e8f880d1300d1d79b96c597b995b36b756a03d6871a945b986f2dd`).
//! Its store must be reachable with no node online
//! (`kudo:4909f3e9f6d391d8a20dc8587a1ffc33160eb238d86d1cc55d5b9fb9ce754e76`).
//!
//! This crate is an engine, not a service: every composition root that needs
//! contexts links it and talks to the store directly.
//!
//! Phase 1 gives it the queue a pull worker needs: enqueue, pull (a claim with a
//! deadline), submit (supersedes the head, settles the claim) and release. Every
//! write is a kino in a local kinora store; nothing is edited in place.
//!
//! task: 4aef99814b668f7cbb8900eee43e35b67fdb6dbcd18ba196858f6226874cc0e2@715de6400c53b648fca5c4b6005b5b0be2327af9d505061c29fd17377f6f565a

pub mod error;
pub mod store;

pub use error::Error;
pub use store::ContextStore;

/// Seconds since the Unix epoch. Every time-dependent call takes `now`
/// explicitly, so deadlines are testable without a clock.
pub type Now = i64;

/// A context's stable identity (its kinora id).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContextId(pub String);

/// A queue item's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemId(pub String);

/// A claim's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClaimId(pub String);

/// A submission's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubmissionId(pub String);

/// The context's head: the current version of its resume block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// The version hash of this head.
    pub version: String,
    /// The resume block as markdown.
    pub body: String,
}

/// One item of work enqueued under a context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: ItemId,
    /// Enqueue order within its context, from 1.
    pub seq: u64,
    /// The ask, as typed.
    pub ask: String,
}

/// A worker's claim on an item: exclusive until settled or past its deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub id: ClaimId,
    pub context: ContextId,
    pub item: Item,
    /// 1 for the first claim on the item, N+1 after attempt N settled or expired.
    pub attempt: u32,
    /// The lease (session) that holds the claim.
    pub lease: String,
    pub deadline: Now,
}

/// What a worker hands back for the item it claimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submission {
    /// The full new head body, as the worker leaves it.
    pub head: String,
    /// The log entry: what was established, with its evidence.
    pub log: String,
}

/// A rough edge filed from inside the work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEntry {
    pub line: String,
    pub body: Option<String>,
    /// The lease (session) that filed it.
    pub lease: String,
}

/// A context as a list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSummary {
    pub id: ContextId,
    pub title: String,
    /// When it was created, as an RFC 3339 timestamp.
    pub created: String,
}
