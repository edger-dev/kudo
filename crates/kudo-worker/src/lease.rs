//! The per-lease record: how many items the lease has been handed, and whether
//! (and why) it has ended. One small file per lease, on the node — every hook
//! runs as its own process, so the record must outlive each of them.

use std::path::{Path, PathBuf};

/// Why a lease ended; the Stop hook then lets the session end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseEnd {
    /// Nothing ready under the bound context.
    QueueEmpty,
    /// PreCompact fired: no item is worked on a summarised transcript.
    Compaction,
    /// The lease's turn budget is spent.
    Budget,
    /// A human typed in the pane; the session is theirs now.
    Takeover,
    /// The session would not submit or release its claim through repeated
    /// refused stops; the claim was given back.
    Stuck,
}

impl LeaseEnd {
    fn as_str(self) -> &'static str {
        match self {
            Self::QueueEmpty => "queue-empty",
            Self::Compaction => "compaction",
            Self::Budget => "budget",
            Self::Takeover => "takeover",
            Self::Stuck => "stuck",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "queue-empty" => Some(Self::QueueEmpty),
            "compaction" => Some(Self::Compaction),
            "budget" => Some(Self::Budget),
            "takeover" => Some(Self::Takeover),
            "stuck" => Some(Self::Stuck),
            _ => None,
        }
    }
}

/// What the book knows about one lease.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaseRecord {
    /// Items handed over by the Stop hook so far.
    pub turns: u32,
    /// Set once the lease has ended.
    pub ended: Option<LeaseEnd>,
}

/// Lease records kept in a directory, one file per lease.
pub struct LeaseBook {
    dir: PathBuf,
}

impl LeaseBook {
    /// A book kept in `dir` (created on first write).
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// The record for `lease`; a lease never seen has the default record.
    pub fn get(&self, lease: &str) -> std::io::Result<LeaseRecord> {
        let text = match std::fs::read_to_string(self.path(lease)) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LeaseRecord::default()),
            Err(e) => return Err(e),
        };
        let mut record = LeaseRecord::default();
        for line in text.lines() {
            match line.split_once('=') {
                Some(("turns", n)) => record.turns = n.parse().unwrap_or(0),
                Some(("ended", why)) => record.ended = LeaseEnd::parse(why),
                _ => {}
            }
        }
        Ok(record)
    }

    /// Replace the record for `lease`.
    pub fn put(&self, lease: &str, record: &LeaseRecord) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let mut text = format!("turns={}\n", record.turns);
        if let Some(why) = record.ended {
            text.push_str(&format!("ended={}\n", why.as_str()));
        }
        // Write then rename, so a reader never sees half a record.
        let tmp = self.dir.join(format!(".{}.tmp", file_name(lease)));
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, self.path(lease))
    }

    fn path(&self, lease: &str) -> PathBuf {
        self.dir.join(format!("{}.lease", file_name(lease)))
    }
}

/// A lease id as a file name: anything outside `[A-Za-z0-9_-]` becomes `_`.
fn file_name(lease: &str) -> String {
    lease
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
