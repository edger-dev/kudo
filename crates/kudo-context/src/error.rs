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
    /// The prefix names more than one context: (the prefix, how many).
    AmbiguousContext(String, usize),
    /// No claim with this id exists in the store.
    UnknownClaim(String),
    /// The claim was already settled (submitted or released).
    ClaimSettled(String),
    /// The claim's deadline passed; its item may already be claimed again.
    ClaimExpired(String),
    /// The head moved since the submission's base: build on `current` (the
    /// head's versions now) and submit again. The claim stays live.
    HeadMoved { claim: String, current: Vec<String> },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "context store i/o: {e}"),
            Self::Kinora(m) => write!(f, "context store: {m}"),
            Self::UnknownContext(id) => write!(f, "no context {id}"),
            Self::AmbiguousContext(prefix, n) => {
                write!(f, "{prefix} names {n} contexts: give more of the id")
            }
            Self::UnknownClaim(id) => write!(f, "no claim {id}"),
            Self::ClaimSettled(id) => write!(f, "claim {id} is already settled"),
            Self::ClaimExpired(id) => write!(f, "claim {id} is past its deadline"),
            Self::HeadMoved { claim, current } => write!(
                f,
                "the head moved since claim {claim} was taken: it is now {}",
                current.join(", ")
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
