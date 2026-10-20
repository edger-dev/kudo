//! The engine's errors.

use std::fmt;

/// Why a context operation failed.
#[derive(Debug)]
pub enum Error {
    /// The store directory could not be read or written.
    Io(std::io::Error),
    /// kinora refused a store or resolve; the message is kinora's.
    Kinora(String),
    /// No context with this id exists in the store.
    UnknownContext(String),
    /// No claim with this id exists in the store.
    UnknownClaim(String),
    /// The claim was already settled (submitted or released).
    ClaimSettled(String),
    /// The claim's deadline passed; its item may already be claimed again.
    ClaimExpired(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "context store i/o: {e}"),
            Self::Kinora(m) => write!(f, "context store: {m}"),
            Self::UnknownContext(id) => write!(f, "no context {id}"),
            Self::UnknownClaim(id) => write!(f, "no claim {id}"),
            Self::ClaimSettled(id) => write!(f, "claim {id} is already settled"),
            Self::ClaimExpired(id) => write!(f, "claim {id} is past its deadline"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
