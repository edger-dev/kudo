//! The per-lease record: how many items the lease has been handed, and whether
//! (and why) it has ended. One small file per lease, on the node.

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
    #[allow(dead_code, reason = "phase 1 stub; the implementation reads it")]
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
    pub fn get(&self, _lease: &str) -> std::io::Result<LeaseRecord> {
        Ok(LeaseRecord::default())
    }

    /// Replace the record for `lease`.
    pub fn put(&self, _lease: &str, _record: &LeaseRecord) -> std::io::Result<()> {
        Ok(())
    }
}
