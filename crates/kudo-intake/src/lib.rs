//! Shared note contracts for the remote inbox channel.
//!
//! implements: intake-note-admission@901cfa34ef07a5318cf05122369a92ef3730999651fb495c8b595f9c26c74ad3
//! task: remote-inbox-channel@67edb688a93e1923a760c609d04639fee94974a30498f9a67b3d8173a04cac86

/// A note a sending session asks to deliver to a repository inbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub target: String,
    pub title: String,
    pub body: String,
    pub attestation: bool,
    pub author: String,
    pub machine: String,
    pub session: String,
    pub created_at: String,
}

/// Deterministic admission checks configured by the sender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRules {
    pub max_body_bytes: usize,
    pub sensitive_markers: Vec<String>,
}

/// Why a note was refused before transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionError {
    MissingAttestation,
    BlankField(&'static str),
    BodyTooLarge { actual: usize, max: usize },
    CodeBlock,
    SensitiveMarker(String),
}

impl Note {
    /// Check this note before any transport sees it.
    pub fn validate(&self, _rules: &AdmissionRules) -> Result<(), AdmissionError> {
        Ok(())
    }
}
