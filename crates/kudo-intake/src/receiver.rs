use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use jiff::Timestamp;
use kinora::assign::AssignEvent;
use kinora::commit::{CommitParams, read_root_pointer};
use kinora::config::Config;
use kinora::hash::Hash;
use kinora::kino::StoreKinoParams;
use kinora::root::{RootEntry, RootKinograph};
use kinora::store::ContentStore;

use crate::{AdmissionError, AdmissionRules, Note};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiveOutcome {
    Delivered { kino_id: String },
    Duplicate { kino_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedNote {
    pub note_id: String,
    pub kino_id: String,
    pub title: String,
    pub created_at: String,
    pub received_at: String,
    pub unreviewed: bool,
}

#[derive(Debug)]
pub enum ReceiveError {
    Admission(AdmissionError),
    UnknownTarget(String),
    CheckoutNotReady(String),
    IdConflict,
    InvalidTime(String),
    Backend(String),
}

impl std::fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => write!(f, "{error}"),
            Self::UnknownTarget(target) => write!(f, "unknown intake target {target:?}"),
            Self::CheckoutNotReady(why) => write!(f, "intake checkout is not ready: {why}"),
            Self::IdConflict => write!(f, "note id was already used with different content"),
            Self::InvalidTime(value) => write!(f, "{value:?} is not an RFC 3339 timestamp"),
            Self::Backend(error) => write!(f, "intake backend: {error}"),
        }
    }
}

impl std::error::Error for ReceiveError {}

/// Writes accepted notes into configured repository inboxes.
pub struct Receiver {
    targets: BTreeMap<String, PathBuf>,
}

impl Receiver {
    pub fn new(targets: BTreeMap<String, PathBuf>) -> Self {
        Self { targets }
    }

    pub fn receive(
        &self,
        note: &Note,
        rules: &AdmissionRules,
        received_at: &str,
    ) -> Result<ReceiveOutcome, ReceiveError> {
        note.validate(rules).map_err(ReceiveError::Admission)?;
        timestamp(&note.created_at)?;
        timestamp(received_at)?;
        let repo = self
            .targets
            .get(&note.target)
            .ok_or_else(|| ReceiveError::UnknownTarget(note.target.clone()))?;
        ensure_checkout_ready(repo)?;

        let fingerprint = fingerprint(note);
        if let Some(existing) = find_note(repo, &note.id)? {
            if existing
                .metadata
                .get("intake::fingerprint")
                .is_some_and(|current| current != &fingerprint)
            {
                return Err(ReceiveError::IdConflict);
            }
            return Ok(ReceiveOutcome::Duplicate {
                kino_id: existing.id,
            });
        }

        let root = repo.join(".kinora");
        let content = format!(
            "# {}\n\nRemote note id: {}\n\n{}\n",
            note.title, note.id, note.body
        );
        let kino_id = Hash::of_content(content.as_bytes()).as_hex().to_owned();
        let metadata = BTreeMap::from([
            ("name".into(), format!("remote-note-{}", &kino_id[..16])),
            ("status::unreviewed".into(), "true".into()),
            ("intake::note_id".into(), note.id.clone()),
            ("intake::fingerprint".into(), fingerprint),
            ("intake::target".into(), note.target.clone()),
            ("intake::title".into(), note.title.clone()),
            ("intake::machine".into(), note.machine.clone()),
            ("intake::session".into(), note.session.clone()),
            ("intake::created_at".into(), note.created_at.clone()),
            ("intake::received_at".into(), received_at.into()),
        ]);
        let stored = kinora::kino::store_kino(
            &root,
            StoreKinoParams {
                kind: "markdown".into(),
                content: content.into_bytes(),
                author: note.author.clone(),
                provenance: "remote-inbox".into(),
                ts: note.created_at.clone(),
                metadata,
                id: None,
                parents: vec![],
            },
        )
        .map_err(backend)?;
        kinora::assign::write_assign(
            &root,
            &AssignEvent {
                kino_id: stored.event.id.clone(),
                target_root: "inbox".into(),
                supersedes: vec![],
                author: note.author.clone(),
                ts: received_at.into(),
                provenance: "remote-inbox".into(),
            },
        )
        .map_err(backend)?;
        commit(
            &root,
            CommitParams {
                author: note.author.clone(),
                provenance: "remote-inbox".into(),
                ts: received_at.into(),
            },
        )?;
        git(repo, &["add", ".kinora"])?;
        git(
            repo,
            &[
                "commit",
                "-m",
                &format!("inbox: {} [{}]", note.title, note.id),
            ],
        )?;
        Ok(ReceiveOutcome::Delivered {
            kino_id: stored.event.id,
        })
    }

    pub fn list_since(&self, target: &str, since: &str) -> Result<Vec<ReceivedNote>, ReceiveError> {
        let repo = self
            .targets
            .get(target)
            .ok_or_else(|| ReceiveError::UnknownTarget(target.into()))?;
        let since = timestamp(since)?;
        let mut arrivals = Vec::new();
        for entry in read_root(repo, "inbox")? {
            let Some(note_id) = entry.metadata.get("intake::note_id") else {
                continue;
            };
            let received_at = required_metadata(&entry, "intake::received_at")?;
            let received = timestamp(received_at)?;
            if received < since {
                continue;
            }
            arrivals.push((
                received,
                ReceivedNote {
                    note_id: note_id.clone(),
                    kino_id: entry.id.clone(),
                    title: required_metadata(&entry, "intake::title")?.clone(),
                    created_at: required_metadata(&entry, "intake::created_at")?.clone(),
                    received_at: received_at.clone(),
                    unreviewed: entry
                        .metadata
                        .get("status::unreviewed")
                        .is_some_and(|value| value == "true"),
                },
            ));
        }
        arrivals.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(arrivals.into_iter().map(|(_, note)| note).collect())
    }
}

fn commit(root: &Path, params: CommitParams) -> Result<(), ReceiveError> {
    for (root_name, result) in kinora::commit::commit_all(root, params).map_err(backend)? {
        result.map_err(|error| ReceiveError::Backend(format!("{root_name}: {error}")))?;
    }
    Ok(())
}

fn timestamp(value: &str) -> Result<Timestamp, ReceiveError> {
    value
        .parse::<Timestamp>()
        .map_err(|_| ReceiveError::InvalidTime(value.into()))
}

fn backend(error: impl std::fmt::Display) -> ReceiveError {
    ReceiveError::Backend(error.to_string())
}

fn fingerprint(note: &Note) -> String {
    let mut canonical = String::new();
    for value in [
        &note.target,
        &note.title,
        &note.body,
        &note.author,
        &note.machine,
        &note.session,
        &note.created_at,
    ] {
        canonical.push_str(&format!("{}:{}\n", value.len(), value));
    }
    Hash::of_content(canonical.as_bytes()).as_hex().to_owned()
}

fn ensure_checkout_ready(repo: &Path) -> Result<(), ReceiveError> {
    let branch = git(repo, &["symbolic-ref", "--short", "HEAD"])?;
    if branch != "main" {
        return Err(ReceiveError::CheckoutNotReady(format!(
            "expected main, found {branch:?}"
        )));
    }
    let dirty = git(repo, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        return Err(ReceiveError::CheckoutNotReady(
            "the checkout has uncommitted changes".into(),
        ));
    }
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<String, ReceiveError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(backend)?;
    if !output.status.success() {
        return Err(ReceiveError::CheckoutNotReady(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn find_note(repo: &Path, note_id: &str) -> Result<Option<RootEntry>, ReceiveError> {
    let config = read_config(repo)?;
    for root in config.roots.keys() {
        for entry in read_root(repo, root)? {
            if entry
                .metadata
                .get("intake::note_id")
                .is_some_and(|id| id == note_id)
            {
                return Ok(Some(entry));
            }
        }
    }
    Ok(None)
}

fn read_config(repo: &Path) -> Result<Config, ReceiveError> {
    let text = std::fs::read_to_string(repo.join(".kinora/config.styx")).map_err(backend)?;
    Config::from_styx(&text).map_err(backend)
}

fn read_root(repo: &Path, root_name: &str) -> Result<Vec<RootEntry>, ReceiveError> {
    let root = repo.join(".kinora");
    let Some(hash) = read_root_pointer(&root, root_name).map_err(backend)? else {
        return Ok(vec![]);
    };
    let content = ContentStore::new(&root).read(&hash).map_err(backend)?;
    Ok(RootKinograph::parse(&content).map_err(backend)?.entries)
}

fn required_metadata<'a>(entry: &'a RootEntry, key: &str) -> Result<&'a String, ReceiveError> {
    entry
        .metadata
        .get(key)
        .ok_or_else(|| ReceiveError::Backend(format!("note {} lacks {key}", entry.id)))
}
