//! Shared note contracts for the remote inbox channel.
//!
//! implements: intake-note-admission@901cfa34ef07a5318cf05122369a92ef3730999651fb495c8b595f9c26c74ad3
//! task: remote-inbox-channel@67edb688a93e1923a760c609d04639fee94974a30498f9a67b3d8173a04cac86

pub const INTAKE_CONNECTOR: &str = "kudo-intake";

mod receiver;

pub use receiver::{ReceiveError, ReceiveOutcome, ReceivedNote, Receiver};

/// A note a sending session asks to deliver to a repository inbox.
#[derive(facet::Facet, Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub target: String,
    pub title: String,
    pub body: String,
    pub attestation: bool,
    pub author: String,
    pub machine: String,
    pub session: String,
    pub created_at: String,
}

/// A request to list one target's arrivals.
#[derive(facet::Facet, Debug, Clone, PartialEq, Eq)]
pub struct IntakeList {
    pub target: String,
    pub since: String,
}

/// A connector's reply to a send or list call.
#[repr(u8)]
#[derive(facet::Facet, Debug, Clone, PartialEq, Eq)]
pub enum IntakeReply {
    Delivered { kino_id: String },
    Duplicate { kino_id: String },
    Refused { reason: String },
    Listed { notes: Vec<ReceivedNote> },
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

impl std::fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAttestation => write!(
                f,
                "the sender must attest that the note contains no code or employer-specific information"
            ),
            Self::BlankField(field) => write!(f, "{field} must not be blank"),
            Self::BodyTooLarge { actual, max } => {
                write!(f, "body is {actual} bytes, over the {max}-byte limit")
            }
            Self::CodeBlock => write!(f, "code blocks are not accepted"),
            Self::SensitiveMarker(marker) => {
                write!(f, "sensitive marker {marker:?} was found")
            }
        }
    }
}

impl std::error::Error for AdmissionError {}

impl Note {
    /// Check this note before any transport sees it.
    pub fn validate(&self, rules: &AdmissionRules) -> Result<(), AdmissionError> {
        if !self.attestation {
            return Err(AdmissionError::MissingAttestation);
        }
        for (field, value) in [
            ("id", &self.id),
            ("target", &self.target),
            ("title", &self.title),
            ("body", &self.body),
            ("author", &self.author),
            ("machine", &self.machine),
            ("session", &self.session),
            ("created_at", &self.created_at),
        ] {
            if value.trim().is_empty() {
                return Err(AdmissionError::BlankField(field));
            }
        }
        if self.body.len() > rules.max_body_bytes {
            return Err(AdmissionError::BodyTooLarge {
                actual: self.body.len(),
                max: rules.max_body_bytes,
            });
        }
        if self
            .body
            .lines()
            .any(|line| line.starts_with("    ") || line.starts_with('\t'))
            || self.body.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("```") || line.starts_with("~~~")
            })
        {
            return Err(AdmissionError::CodeBlock);
        }
        let title = self.title.to_lowercase();
        let body = self.body.to_lowercase();
        if let Some(marker) = rules.sensitive_markers.iter().find(|marker| {
            if marker.trim().is_empty() {
                return false;
            }
            let marker = marker.to_lowercase();
            title.contains(&marker) || body.contains(&marker)
        }) {
            return Err(AdmissionError::SensitiveMarker(marker.clone()));
        }
        Ok(())
    }
}
