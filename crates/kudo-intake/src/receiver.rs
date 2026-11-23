use std::collections::BTreeMap;
use std::path::PathBuf;

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

/// Writes accepted notes into configured repository inboxes.
pub struct Receiver;

impl Receiver {
    pub fn new(_targets: BTreeMap<String, PathBuf>) -> Self {
        Self
    }

    pub fn receive(
        &self,
        _note: &Note,
        _rules: &AdmissionRules,
        _received_at: &str,
    ) -> Result<ReceiveOutcome, ReceiveError> {
        Err(ReceiveError::UnknownTarget("not implemented".into()))
    }

    pub fn list_since(
        &self,
        target: &str,
        _since: &str,
    ) -> Result<Vec<ReceivedNote>, ReceiveError> {
        Err(ReceiveError::UnknownTarget(target.into()))
    }
}
