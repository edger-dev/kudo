//! The context store: a local kinora store holding contexts, items, claims,
//! settlements and submissions, each a kino. Nothing is edited in place: a
//! head moves by a new version, an item moves by a claim and a settlement.
//!
//! The shapes, all distinguished by `kudo::kind` metadata:
//!
//! - **context** — born as a card (a random marker, title, creation time —
//!   the marker keeps two same-title, same-second contexts apart); every head is a
//!   version of that identity, so the supersedes chain is the head history.
//! - **item** — write-once; the ask as typed, under a one-line marker that
//!   makes two identical asks two distinct kinos.
//! - **claim** — write-once, its content `(item, attempt)` only, so the same
//!   attempt can be claimed once; lease and deadline ride in metadata.
//! - **settle** — write-once, one per claim: `submitted` or `released`.
//! - **submission** — write-once; the typed submission rendered in fixed
//!   sections (established lines whose evidence did not resolve land as
//!   claimed), linked to its claim, item, head and cited kinos.
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439
//! implements: 6f2cfc2264e8f880d1300d1d79b96c597b995b36b756a03d6871a945b986f2dd@6f2cfc2264e8f880d1300d1d79b96c597b995b36b756a03d6871a945b986f2dd
//! implements: 3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab@aaddfc2e7ee067ea5780ce6ee2dfe4587bfb47b0c067b2670a31308c019a7e21
//! implements: 4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9@4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use kinora::event::Event;
use kinora::hash::Hash;
use kinora::kino::{StoreKinoParams, store_kino};
use kinora::paths::kinora_root;
use kinora::resolve::Resolver;
use kinora::store::ContentStore;

use crate::{
    Claim, ClaimId, ContextId, ContextSummary, Error, Head, InboxEntry, Item, ItemId, Now,
    Submission, SubmissionId, Submitted,
};

const KIND: &str = "kudo::kind";
const CONTEXT: &str = "kudo::context";
const ITEM: &str = "kudo::item";
const CLAIM: &str = "kudo::claim";
const SEQ: &str = "kudo::seq";
const ATTEMPT: &str = "kudo::attempt";
const LEASE: &str = "kudo::lease";
const DEADLINE: &str = "kudo::deadline";
const OUTCOME: &str = "kudo::outcome";
const HEAD: &str = "kudo::head";
const BASE: &str = "kudo::base";
const EVIDENCE: &str = "kudo::evidence";

const SUBMITTED: &str = "submitted";
const RELEASED: &str = "released";

/// A context store rooted at a directory.
pub struct ContextStore {
    /// The `.kinora` directory inside the store's directory.
    kinora: PathBuf,
}

impl ContextStore {
    /// Create a new store in `dir` (which must not hold one yet).
    pub fn init(dir: &Path) -> Result<Self, Error> {
        kinora::init::init(dir, "kudo-context").map_err(kin)?;
        Self::open(dir)
    }

    /// Open an existing store in `dir`.
    pub fn open(dir: &Path) -> Result<Self, Error> {
        let kinora = kinora_root(dir);
        if !kinora.is_dir() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no context store at {}", dir.display()),
            )));
        }
        Ok(Self { kinora })
    }

    /// Birth a context whose first head is `head`.
    pub fn create_context(&self, title: &str, head: &str, now: Now) -> Result<ContextId, Error> {
        // The marker makes the card unique: a title and a second do not.
        let card = format!(
            "<!-- kudo context {} -->\n# {title}\n\nkudo context, created {}\n",
            nonce()?,
            ts(now)
        );
        let born = self.put(
            "markdown",
            card.into_bytes(),
            meta([(KIND, "context")]),
            None,
            now,
        )?;
        let ctx = ContextId(born.id.clone());
        self.put(
            "markdown",
            head_content(&format!("{} new", ctx.0), head),
            meta([(KIND, "head")]),
            Some((born.id, vec![born.hash])),
            now,
        )?;
        Ok(ctx)
    }

    /// Every context in the store, newest first.
    pub fn contexts(&self) -> Result<Vec<ContextSummary>, Error> {
        let view = self.view()?;
        let mut cards = view.cards();
        cards.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| a.id.cmp(&b.id)));
        cards
            .into_iter()
            .map(|card| {
                let content = self.read(&card.hash)?;
                let title = strip_marker(&content)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim_start_matches("# ")
                    .to_owned();
                Ok(ContextSummary {
                    id: ContextId(card.id.clone()),
                    title,
                    created: card.ts.clone(),
                })
            })
            .collect()
    }

    /// The context `id_or_prefix` names: its full id, or a prefix of it that
    /// no other context shares (ids are all one length, so a full id is
    /// always unique).
    pub fn resolve_context(&self, id_or_prefix: &str) -> Result<ContextId, Error> {
        if id_or_prefix.is_empty() {
            return Err(Error::UnknownContext(String::new()));
        }
        let view = self.view()?;
        let matches: Vec<&Event> = view
            .cards()
            .into_iter()
            .filter(|card| card.id.starts_with(id_or_prefix))
            .collect();
        match matches.as_slice() {
            [one] => Ok(ContextId(one.id.clone())),
            [] => Err(Error::UnknownContext(id_or_prefix.to_owned())),
            many => Err(Error::AmbiguousContext(id_or_prefix.to_owned(), many.len())),
        }
    }

    /// The context's current head.
    pub fn head(&self, ctx: &ContextId) -> Result<Head, Error> {
        let view = self.view()?;
        let chain = view.head_chain(ctx)?;
        let last = chain
            .last()
            .ok_or_else(|| Error::UnknownContext(ctx.0.clone()))?;
        self.head_of(last)
    }

    /// The head's current versions, newest first: one, unless two submits
    /// raced and forked it.
    pub fn heads(&self, ctx: &ContextId) -> Result<Vec<Head>, Error> {
        let view = self.view()?;
        view.tips(ctx)?.iter().map(|e| self.head_of(e)).collect()
    }

    /// One version of a head, by its hash.
    pub fn head_version(&self, version: &str) -> Result<Head, Error> {
        Ok(Head {
            version: version.to_owned(),
            body: strip_marker(&self.read(version)?),
        })
    }

    /// Every version of the head, oldest first.
    pub fn head_history(&self, ctx: &ContextId) -> Result<Vec<Head>, Error> {
        let view = self.view()?;
        view.head_chain(ctx)?
            .iter()
            .map(|e| self.head_of(e))
            .collect()
    }

    /// Enqueue `ask` under the context.
    pub fn enqueue(&self, ctx: &ContextId, ask: &str, now: Now) -> Result<ItemId, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        let seq = (view.of_kind_in("item", ctx).len() + 1).to_string();
        let content = format!("<!-- kudo item {} #{seq} -->\n{ask}", ctx.0);
        let born = self.put(
            "markdown",
            content.into_bytes(),
            meta([(KIND, "item"), (CONTEXT, &ctx.0), (SEQ, &seq)]),
            None,
            now,
        )?;
        Ok(ItemId(born.id))
    }

    /// Whether an item is ready to pull: not submitted, and not held by a live
    /// claim.
    pub fn has_ready(&self, ctx: &ContextId, now: Now) -> Result<bool, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        Ok(view.next_ready(ctx, now).is_some())
    }

    /// How many items are ready to pull.
    pub fn ready_count(&self, ctx: &ContextId, now: Now) -> Result<usize, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        Ok(view.ready(ctx, now).len())
    }

    /// Claim the oldest ready item for `lease`, with a deadline `ttl` seconds
    /// from `now`. `None` when nothing is ready.
    pub fn pull(
        &self,
        ctx: &ContextId,
        lease: &str,
        now: Now,
        ttl: i64,
    ) -> Result<Option<Claim>, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        let Some((item_event, attempt)) = view.next_ready(ctx, now) else {
            return Ok(None);
        };
        let item = self.item_of(item_event)?;
        let deadline = now + ttl;

        // The content is (item, attempt) and nothing else, so an attempt can be
        // claimed once: a second writer computes the same id and finds it taken.
        let content = format!("kudo claim\nitem: {}\nattempt: {attempt}\n", item.id.0);
        if view
            .by_id
            .contains_key(Hash::of_content(content.as_bytes()).as_hex())
        {
            return Ok(None);
        }
        let base: Vec<String> = view.tips(ctx)?.iter().map(|e| e.hash.clone()).collect();
        let base_s = base.join(",");
        let attempt_s = attempt.to_string();
        let deadline_s = deadline.to_string();
        let born = self.put(
            "text",
            content.into_bytes(),
            meta([
                (KIND, "claim"),
                (CONTEXT, &ctx.0),
                (ITEM, &item.id.0),
                (ATTEMPT, &attempt_s),
                (LEASE, lease),
                (DEADLINE, &deadline_s),
                (BASE, &base_s),
            ]),
            None,
            now,
        )?;
        let id = ClaimId(born.id);
        if !self.confirm(&id, lease)? {
            return Ok(None); // lost a race for this attempt
        }
        Ok(Some(Claim {
            id,
            context: ctx.clone(),
            item,
            attempt,
            lease: lease.to_owned(),
            deadline,
            base,
        }))
    }

    /// Submit the work for a live claim: the head is superseded by
    /// `submission.head`, the log entry is stored, and the claim is settled.
    pub fn submit(
        &self,
        claim: &ClaimId,
        submission: &Submission,
        now: Now,
    ) -> Result<Submitted, Error> {
        let view = self.view()?;
        let claim_event = view.live_claim(claim, now)?;
        let ctx = ContextId(field(claim_event, CONTEXT).to_owned());
        let item = field(claim_event, ITEM).to_owned();

        // The new head must build on the head as it is now: every tip, if
        // two submits raced and forked it.
        let tips = view.tips(&ctx)?;
        let current: Vec<String> = tips.iter().map(|e| e.hash.clone()).collect();
        let mut base = submission
            .base
            .clone()
            .unwrap_or_else(|| base_of(claim_event));
        base.sort();
        let mut now_sorted = current.clone();
        now_sorted.sort();
        // A claim taken before claims recorded their base has none (a live
        // context always has a tip): there is nothing to check it against.
        let recorded = !base.is_empty();
        if recorded && base != now_sorted {
            return Err(Error::HeadMoved {
                claim: claim.0.clone(),
                current,
            });
        }
        // An unchanged head over a single tip needs no new version. Anything
        // else is written: the claim's marker makes the bytes new, so a head
        // that returns to an earlier version's text cannot loop the history,
        // and a fork always closes.
        let unchanged = match tips.as_slice() {
            [only] if self.head_of(only)?.body == submission.head => Some(only.hash.clone()),
            _ => None,
        };
        let head_version = match unchanged {
            Some(version) => version,
            None => {
                self.put(
                    "markdown",
                    head_content(&claim.0, &submission.head),
                    meta([(KIND, "head"), (CLAIM, &claim.0)]),
                    Some((ctx.0.clone(), current)),
                    now,
                )?
                .hash
            }
        };

        let checked = view.check_evidence(submission);
        let seq = (view.of_kind_in("submission", &ctx).len() + 1).to_string();
        let content = format!(
            "<!-- kudo submission {} -->\n{}",
            claim.0,
            render(submission, &checked, &head_version)
        );
        let cited = checked.kinos().join(",");
        let born = self.put(
            "markdown",
            content.into_bytes(),
            meta([
                (KIND, "submission"),
                (CONTEXT, &ctx.0),
                (ITEM, &item),
                (CLAIM, &claim.0),
                (SEQ, &seq),
                (HEAD, &head_version),
                (EVIDENCE, &cited),
            ]),
            None,
            now,
        )?;
        self.settle(&ctx, claim, SUBMITTED, now)?;
        Ok(Submitted {
            id: SubmissionId(born.id),
            downgraded: checked.downgraded,
            unresolved: checked.unresolved,
        })
    }

    /// Give a live claim back: its item is ready again, as the next attempt.
    pub fn release(&self, claim: &ClaimId, now: Now) -> Result<(), Error> {
        let view = self.view()?;
        let claim_event = view.live_claim(claim, now)?;
        let ctx = ContextId(field(claim_event, CONTEXT).to_owned());
        self.settle(&ctx, claim, RELEASED, now)
    }

    /// The live claim `lease` holds under the context, if any.
    pub fn held_by(&self, ctx: &ContextId, lease: &str, now: Now) -> Result<Option<Claim>, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        let held = view
            .by_id
            .values()
            .filter_map(|events| claim_birth(events))
            .filter(|e| field(e, CONTEXT) == ctx.0 && field(e, LEASE) == lease)
            .find(|e| view.live_claim(&ClaimId(e.id.clone()), now).is_ok());
        let Some(claim) = held else {
            return Ok(None);
        };
        let item = view
            .by_id
            .get(field(claim, ITEM))
            .and_then(|events| birth(events))
            .ok_or_else(|| Error::Kinora(format!("claim {} names a missing item", claim.id)))?;
        Ok(Some(Claim {
            id: ClaimId(claim.id.clone()),
            context: ctx.clone(),
            item: self.item_of(item)?,
            attempt: field(claim, ATTEMPT).parse().unwrap_or(1),
            lease: lease.to_owned(),
            deadline: deadline_of(claim),
            base: base_of(claim),
        }))
    }

    /// File a rough edge under the context: one line, an optional body, and
    /// the lease that filed it.
    pub fn file_inbox(
        &self,
        ctx: &ContextId,
        lease: &str,
        line: &str,
        body: Option<&str>,
        now: Now,
    ) -> Result<String, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        let seq = (view.of_kind_in("inbox", ctx).len() + 1).to_string();
        let mut content = format!("<!-- kudo inbox {} #{seq} -->\n# {}\n", ctx.0, line.trim());
        if let Some(body) = body.filter(|b| !b.trim().is_empty()) {
            content.push('\n');
            content.push_str(body.trim_end());
            content.push('\n');
        }
        let born = self.put(
            "markdown",
            content.into_bytes(),
            meta([
                (KIND, "inbox"),
                (CONTEXT, &ctx.0),
                (LEASE, lease),
                (SEQ, &seq),
            ]),
            None,
            now,
        )?;
        Ok(born.id)
    }

    /// The context's inbox, oldest first.
    pub fn inbox(&self, ctx: &ContextId) -> Result<Vec<InboxEntry>, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        let mut entries = view.of_kind_in("inbox", ctx);
        entries.sort_by_key(|e| seq_of(e));
        entries
            .into_iter()
            .map(|e| {
                let text = strip_marker(&self.read(&e.hash)?);
                let (first, rest) = text.split_once('\n').unwrap_or((&text, ""));
                let body = rest.trim();
                Ok(InboxEntry {
                    line: first.trim_start_matches("# ").to_owned(),
                    body: (!body.is_empty()).then(|| body.to_owned()),
                    lease: field(e, LEASE).to_owned(),
                })
            })
            .collect()
    }

    /// The context's log: its submissions' entries, oldest first.
    pub fn log(&self, ctx: &ContextId) -> Result<Vec<String>, Error> {
        let view = self.view()?;
        view.require_context(ctx)?;
        let mut subs = view.of_kind_in("submission", ctx);
        subs.sort_by_key(|e| seq_of(e));
        subs.into_iter()
            .map(|e| Ok(strip_marker(&self.read(&e.hash)?)))
            .collect()
    }

    /// Whether `lease` holds the claim. Two pulls racing for the same attempt
    /// both write it — the same id, two births; the birth with the smallest
    /// lease wins, and the loser must back off.
    pub(crate) fn confirm(&self, claim: &ClaimId, lease: &str) -> Result<bool, Error> {
        let view = self.view()?;
        let holder = view
            .by_id
            .get(&claim.0)
            .and_then(|events| claim_birth(events));
        Ok(holder.is_some_and(|e| field(e, LEASE) == lease))
    }

    fn settle(
        &self,
        ctx: &ContextId,
        claim: &ClaimId,
        outcome: &str,
        now: Now,
    ) -> Result<(), Error> {
        let content = format!("kudo settle\nclaim: {}\noutcome: {outcome}\n", claim.0);
        self.put(
            "text",
            content.into_bytes(),
            meta([
                (KIND, "settle"),
                (CONTEXT, &ctx.0),
                (CLAIM, &claim.0),
                (OUTCOME, outcome),
            ]),
            None,
            now,
        )?;
        Ok(())
    }

    /// Store one kino: a birth when `version_of` is `None`, else a version of
    /// that identity with the given parents.
    fn put(
        &self,
        kind: &str,
        content: Vec<u8>,
        metadata: BTreeMap<String, String>,
        version_of: Option<(String, Vec<String>)>,
        now: Now,
    ) -> Result<Event, Error> {
        let (id, parents) = match version_of {
            Some((id, parents)) => (Some(id), parents),
            None => (None, Vec::new()),
        };
        let stored = store_kino(
            &self.kinora,
            StoreKinoParams {
                kind: kind.to_owned(),
                content,
                author: "kudo".to_owned(),
                provenance: "kudo-context".to_owned(),
                ts: ts(now),
                metadata,
                id,
                parents,
            },
        )
        .map_err(kin)?;
        Ok(stored.event)
    }

    fn read(&self, hash: &str) -> Result<String, Error> {
        let hash: Hash = hash.parse().map_err(kin)?;
        let bytes = ContentStore::new(&self.kinora).read(&hash).map_err(kin)?;
        String::from_utf8(bytes).map_err(kin)
    }

    fn head_of(&self, event: &Event) -> Result<Head, Error> {
        self.head_version(&event.hash)
    }

    fn item_of(&self, event: &Event) -> Result<Item, Error> {
        Ok(Item {
            id: ItemId(event.id.clone()),
            seq: seq_of(event),
            ask: strip_marker(&self.read(&event.hash)?),
        })
    }

    fn view(&self) -> Result<View, Error> {
        View::load(&self.kinora)
    }
}

/// One read of the whole store, indexed for the queries above.
struct View {
    /// Every identity's events, by id.
    by_id: HashMap<String, Vec<Event>>,
}

impl View {
    fn load(kinora: &Path) -> Result<Self, Error> {
        let resolver = Resolver::load(kinora).map_err(kin)?;
        let by_id = resolver
            .identities()
            .iter()
            .map(|(id, identity)| (id.clone(), identity.events.clone()))
            .collect();
        Ok(Self { by_id })
    }

    /// The birth event of each write-once kino of `kind` under `ctx`.
    fn of_kind_in(&self, kind: &str, ctx: &ContextId) -> Vec<&Event> {
        self.by_id
            .values()
            .filter_map(|events| events.first())
            .filter(|e| field(e, KIND) == kind && field(e, CONTEXT) == ctx.0)
            .collect()
    }

    /// The card (birth event) of every context.
    fn cards(&self) -> Vec<&Event> {
        self.by_id
            .values()
            .filter_map(|events| birth(events))
            .filter(|e| field(e, KIND) == "context")
            .collect()
    }

    fn require_context(&self, ctx: &ContextId) -> Result<(), Error> {
        match self.by_id.get(&ctx.0).and_then(|events| birth(events)) {
            Some(card) if field(card, KIND) == "context" => Ok(()),
            _ => Err(Error::UnknownContext(ctx.0.clone())),
        }
    }

    /// The head versions of a context, oldest first (the card excluded).
    fn head_chain(&self, ctx: &ContextId) -> Result<Vec<&Event>, Error> {
        self.require_context(ctx)?;
        let events = self
            .by_id
            .get(&ctx.0)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let by_hash: HashMap<&str, &Event> = events.iter().map(|e| (e.hash.as_str(), e)).collect();
        // On a fork, follow the newest tip; a merge's first parent is the
        // newest tip it closed.
        let mut chain = Vec::new();
        let mut cursor = self.tips(ctx)?.first().copied();
        while let Some(e) = cursor {
            if e.parents.is_empty() {
                break; // the card
            }
            chain.push(e);
            cursor = e
                .parents
                .first()
                .and_then(|p| by_hash.get(p.as_str()).copied());
        }
        chain.reverse();
        Ok(chain)
    }

    /// Sort a submission's established lines by whether their evidence
    /// resolves: a kino id this store holds, or a URL.
    fn check_evidence(&self, submission: &Submission) -> Checked {
        let known: std::collections::HashSet<&str> = self
            .by_id
            .iter()
            .flat_map(|(id, events)| {
                std::iter::once(id.as_str()).chain(events.iter().map(|e| e.hash.as_str()))
            })
            .collect();
        let mut checked = Checked::default();
        for line in &submission.established {
            let (good, bad): (Vec<&String>, Vec<&String>) = line
                .evidence
                .iter()
                .partition(|e| is_url(e) || known.contains(e.trim()));
            checked
                .unresolved
                .extend(bad.into_iter().map(|e| e.trim().to_owned()));
            if good.is_empty() {
                checked.downgraded.push(line.claim.clone());
            } else {
                checked.established.push((
                    line.claim.clone(),
                    good.into_iter().map(|e| e.trim().to_owned()).collect(),
                ));
            }
        }
        checked
    }

    /// The head versions nothing supersedes, newest first: one, unless two
    /// submits raced.
    fn tips(&self, ctx: &ContextId) -> Result<Vec<&Event>, Error> {
        self.require_context(ctx)?;
        let events = self
            .by_id
            .get(&ctx.0)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let referenced: std::collections::HashSet<&str> = events
            .iter()
            .flat_map(|e| e.parents.iter().map(String::as_str))
            .collect();
        let mut tips: Vec<&Event> = events
            .iter()
            .filter(|e| !referenced.contains(e.hash.as_str()))
            .collect();
        tips.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| a.hash.cmp(&b.hash)));
        tips.dedup_by(|a, b| a.hash == b.hash);
        Ok(tips)
    }

    /// Claims on an item, as (claim birth event, its settle outcome if any).
    fn claims_on<'a>(&'a self, item: &str) -> Vec<(&'a Event, Option<&'a str>)> {
        let settles: HashMap<&str, &str> = self
            .by_id
            .values()
            .filter_map(|events| events.first())
            .filter(|e| field(e, KIND) == "settle")
            .map(|e| (field(e, CLAIM), field(e, OUTCOME)))
            .collect();
        self.by_id
            .values()
            .filter_map(|events| claim_birth(events))
            .filter(|e| field(e, ITEM) == item)
            .map(|e| (e, settles.get(e.id.as_str()).copied()))
            .collect()
    }

    /// The oldest ready item under `ctx` and the attempt its next claim takes.
    fn next_ready(&self, ctx: &ContextId, now: Now) -> Option<(&Event, u32)> {
        self.ready(ctx, now).into_iter().next()
    }

    /// Every ready item under `ctx`, oldest first, with the attempt its next
    /// claim takes.
    fn ready(&self, ctx: &ContextId, now: Now) -> Vec<(&Event, u32)> {
        let mut items = self.of_kind_in("item", ctx);
        items.sort_by_key(|e| seq_of(e));
        items
            .into_iter()
            .filter_map(|item| {
                let claims = self.claims_on(&item.id);
                let submitted = claims
                    .iter()
                    .any(|(_, outcome)| *outcome == Some(SUBMITTED));
                let held = claims
                    .iter()
                    .any(|(claim, outcome)| outcome.is_none() && deadline_of(claim) > now);
                let attempt = u32::try_from(claims.len())
                    .unwrap_or(u32::MAX)
                    .saturating_add(1);
                (!submitted && !held).then_some((item, attempt))
            })
            .collect()
    }

    /// The claim's birth event, if it is still live at `now`.
    fn live_claim(&self, claim: &ClaimId, now: Now) -> Result<&Event, Error> {
        let event = self
            .by_id
            .get(&claim.0)
            .and_then(|events| claim_birth(events))
            .ok_or_else(|| Error::UnknownClaim(claim.0.clone()))?;
        let settled = self
            .claims_on(field(event, ITEM))
            .iter()
            .any(|(c, outcome)| c.id == claim.0 && outcome.is_some());
        if settled {
            return Err(Error::ClaimSettled(claim.0.clone()));
        }
        if deadline_of(event) <= now {
            return Err(Error::ClaimExpired(claim.0.clone()));
        }
        Ok(event)
    }
}

/// A submission's established lines after the evidence check.
#[derive(Default)]
struct Checked {
    /// Lines kept as established, with the evidence that resolved.
    established: Vec<(String, Vec<String>)>,
    /// Lines moved to claimed: none of their evidence resolved.
    downgraded: Vec<String>,
    /// Evidence entries that did not resolve.
    unresolved: Vec<String>,
}

impl Checked {
    /// The kino ids cited as evidence that resolved (URLs left out).
    fn kinos(&self) -> Vec<&str> {
        self.established
            .iter()
            .flat_map(|(_, evidence)| evidence.iter().map(String::as_str))
            .filter(|e| !is_url(e))
            .collect()
    }
}

fn is_url(evidence: &str) -> bool {
    let e = evidence.trim();
    e.starts_with("https://") || e.starts_with("http://")
}

/// A submission's body, in fixed sections so it reads cold.
fn render(submission: &Submission, checked: &Checked, head_version: &str) -> String {
    fn list(out: &mut String, heading: &str, lines: &[String]) {
        out.push_str(&format!("\n## {heading}\n\n"));
        if lines.is_empty() {
            out.push_str("- none\n");
        }
        for line in lines {
            out.push_str(&format!("- {}\n", line.trim()));
        }
    }
    let mut out = format!("{}\n", submission.log.trim_end());
    let established: Vec<String> = checked
        .established
        .iter()
        .map(|(claim, evidence)| format!("{} (evidence: {})", claim.trim(), evidence.join(", ")))
        .collect();
    list(&mut out, "Established", &established);
    let claimed: Vec<String> = checked
        .downgraded
        .iter()
        .map(|c| format!("{} (no evidence that resolved)", c.trim()))
        .chain(submission.claimed.iter().cloned())
        .collect();
    list(&mut out, "Claimed", &claimed);
    let ruled_out: Vec<String> = submission
        .ruled_out
        .iter()
        .map(|r| format!("{} — {}", r.candidate.trim(), r.because.trim()))
        .collect();
    list(&mut out, "Ruled out", &ruled_out);
    list(&mut out, "Gaps", &submission.gaps);
    out.push_str(&format!("\nHead as left: version {head_version}\n"));
    out
}

fn birth(events: &[Event]) -> Option<&Event> {
    events.iter().find(|e| e.parents.is_empty())
}

/// The birth that holds a claim: racing pulls of one attempt each write a
/// birth with the same id, and the smallest lease wins — the same answer for
/// every reader, with no lock.
fn claim_birth(events: &[Event]) -> Option<&Event> {
    events
        .iter()
        .filter(|e| e.parents.is_empty() && field(e, KIND) == "claim")
        .min_by(|a, b| field(a, LEASE).cmp(field(b, LEASE)))
}

fn field<'a>(event: &'a Event, key: &str) -> &'a str {
    event
        .metadata
        .get(key)
        .map(String::as_str)
        .unwrap_or_default()
}

fn seq_of(event: &Event) -> u64 {
    field(event, SEQ).parse().unwrap_or(0)
}

/// The head versions a claim was taken from.
fn base_of(event: &Event) -> Vec<String> {
    field(event, BASE)
        .split(',')
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

fn deadline_of(event: &Event) -> Now {
    field(event, DEADLINE).parse().unwrap_or(0)
}

/// A head version's bytes: the body under a marker naming what wrote it, so
/// no two versions share a hash.
fn head_content(writer: &str, body: &str) -> Vec<u8> {
    format!("<!-- kudo head {writer} -->\n{body}").into_bytes()
}

/// Drop the one-line `<!-- kudo … -->` marker that makes a kino unique.
fn strip_marker(content: &str) -> String {
    match content.split_once('\n') {
        Some((first, rest)) if first.starts_with("<!-- kudo ") => rest.to_owned(),
        _ => content.to_owned(),
    }
}

/// Sixteen random bytes as hex, from the kernel.
fn nonce() -> Result<String, Error> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn meta<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn ts(now: Now) -> String {
    jiff::Timestamp::from_second(now)
        .map(|t| t.to_string())
        .unwrap_or_else(|_| now.to_string())
}

fn kin(e: impl std::fmt::Display) -> Error {
    Error::Kinora(e.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    const T0: Now = 1_792_500_000;

    /// A racing pull wrote the same attempt with another lease. Exactly one of
    /// the two leases holds the claim, and it is the same one for every reader.
    #[test]
    fn of_two_racing_claims_on_one_attempt_exactly_one_holds() {
        let dir = std::env::temp_dir().join(format!("kudo-context-race-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ContextStore::init(&dir).unwrap();
        let ctx = store.create_context("race", "h\n", T0).unwrap();
        let item = store.enqueue(&ctx, "contended", T0).unwrap();
        let claim = store.pull(&ctx, "lease-b", T0, 600).unwrap().unwrap();

        // The other writer's birth of the same claim, with a smaller lease.
        let content = format!("kudo claim\nitem: {}\nattempt: 1\n", item.0);
        let deadline = (T0 + 600).to_string();
        store
            .put(
                "text",
                content.into_bytes(),
                meta([
                    (KIND, "claim"),
                    (CONTEXT, &ctx.0),
                    (ITEM, &item.0),
                    (ATTEMPT, "1"),
                    (LEASE, "lease-a"),
                    (DEADLINE, &deadline),
                ]),
                None,
                T0,
            )
            .unwrap();

        assert!(store.confirm(&claim.id, "lease-a").unwrap());
        assert!(!store.confirm(&claim.id, "lease-b").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two submits raced past the moved-head check: the head forked. A claim
    /// taken now builds on both tips, and its submit closes the fork.
    // implements: 3a472b4021f7b7496dd37779f3d0c76cafe1c95e8e80b1b236786a81e4c7d0ab@aaddfc2e7ee067ea5780ce6ee2dfe4587bfb47b0c067b2670a31308c019a7e21
    #[test]
    fn a_forked_head_closes_on_the_next_submit() {
        let dir = std::env::temp_dir().join(format!("kudo-context-fork-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ContextStore::init(&dir).unwrap();
        let ctx = store.create_context("fork", "h0\n", T0).unwrap();
        let root = store.head(&ctx).unwrap().version;
        for (body, at) in [("left\n", T0 + 1), ("right\n", T0 + 2)] {
            store
                .put(
                    "markdown",
                    body.as_bytes().to_vec(),
                    meta([(KIND, "head")]),
                    Some((ctx.0.clone(), vec![root.clone()])),
                    at,
                )
                .unwrap();
        }
        let heads = store.heads(&ctx).unwrap();
        assert_eq!(heads.len(), 2, "{heads:?}");
        assert_eq!(heads[0].body, "right\n", "newest first");
        assert_eq!(store.head(&ctx).unwrap().body, "right\n");

        store.enqueue(&ctx, "merge", T0 + 3).unwrap();
        let claim = store.pull(&ctx, "lease-a", T0 + 3, 600).unwrap().unwrap();
        let mut base = claim.base.clone();
        base.sort();
        let mut tips: Vec<String> = heads.iter().map(|h| h.version.clone()).collect();
        tips.sort();
        assert_eq!(base, tips);

        let merged = Submission {
            head: "left\nright\n".into(),
            log: "merged".into(),
            ..Default::default()
        };
        store.submit(&claim.id, &merged, T0 + 4).unwrap();
        let heads = store.heads(&ctx).unwrap();
        assert_eq!(heads.len(), 1, "the fork is closed: {heads:?}");
        assert_eq!(heads[0].body, "left\nright\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A fork closes even when the worker keeps one tip's bytes as they are.
    #[test]
    fn a_fork_closes_on_a_submit_that_keeps_one_tip() {
        let dir =
            std::env::temp_dir().join(format!("kudo-context-fork-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ContextStore::init(&dir).unwrap();
        let ctx = store.create_context("fork", "h0\n", T0).unwrap();
        let root = store.head(&ctx).unwrap().version;
        for (body, at) in [("older\n", T0 + 1), ("newer\n", T0 + 2)] {
            store
                .put(
                    "markdown",
                    body.as_bytes().to_vec(),
                    meta([(KIND, "head")]),
                    Some((ctx.0.clone(), vec![root.clone()])),
                    at,
                )
                .unwrap();
        }
        store.enqueue(&ctx, "pick one", T0 + 3).unwrap();
        let claim = store.pull(&ctx, "lease-a", T0 + 3, 600).unwrap().unwrap();
        let keep = Submission {
            head: "older\n".into(),
            log: "the older tip was right".into(),
            ..Default::default()
        };
        store.submit(&claim.id, &keep, T0 + 4).unwrap();
        let heads = store.heads(&ctx).unwrap();
        assert_eq!(heads.len(), 1, "{heads:?}");
        assert_eq!(heads[0].body, "older\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A claim taken before claims recorded their base is checked against
    /// nothing: its first submit is not refused for a move nobody made.
    #[test]
    fn a_claim_without_a_recorded_base_is_not_refused() {
        let dir = std::env::temp_dir().join(format!("kudo-context-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ContextStore::init(&dir).unwrap();
        let ctx = store.create_context("legacy", "h\n", T0).unwrap();
        let item = store.enqueue(&ctx, "old claim", T0).unwrap();
        let content = format!("kudo claim\nitem: {}\nattempt: 1\n", item.0);
        let deadline = (T0 + 600).to_string();
        let born = store
            .put(
                "text",
                content.into_bytes(),
                meta([
                    (KIND, "claim"),
                    (CONTEXT, &ctx.0),
                    (ITEM, &item.0),
                    (ATTEMPT, "1"),
                    (LEASE, "lease-a"),
                    (DEADLINE, &deadline),
                ]),
                None,
                T0,
            )
            .unwrap();
        let sub = Submission {
            head: "h2\n".into(),
            log: "done".into(),
            ..Default::default()
        };
        store.submit(&ClaimId(born.id), &sub, T0 + 1).unwrap();
        assert_eq!(store.head(&ctx).unwrap().body, "h2\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
