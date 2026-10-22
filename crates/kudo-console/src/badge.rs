//! The approvals badge: a trustworthy zero told apart from unknown.
//!
//! implements: 85f973554ab08ddde3cb91944354db7fb14ff55a0da506daea7c4643c8646339@85f973554ab08ddde3cb91944354db7fb14ff55a0da506daea7c4643c8646339

/// What the badge says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    /// Every node answered and nothing waits.
    None,
    /// Jobs waiting for the author's verdict.
    Waiting(usize),
    /// Nothing queued, but this many nodes could not be read.
    Unknown(usize),
}

/// The badge for `waiting` queued jobs and `unknown` unreadable nodes: a
/// real count wins over trouble.
pub fn badge(waiting: usize, unknown: usize) -> Badge {
    if waiting > 0 {
        Badge::Waiting(waiting)
    } else if unknown > 0 {
        Badge::Unknown(unknown)
    } else {
        Badge::None
    }
}

impl Badge {
    /// The badge's words.
    pub fn label(self) -> String {
        match self {
            Badge::None => "approvals · none".to_owned(),
            Badge::Waiting(1) => "1 approval".to_owned(),
            Badge::Waiting(n) => format!("{n} approvals"),
            Badge::Unknown(1) => "approvals · unknown on 1 node".to_owned(),
            Badge::Unknown(n) => format!("approvals · unknown on {n} nodes"),
        }
    }

    /// Its state, for the stylesheet: `none`, `waiting` or `unknown`.
    pub fn state(self) -> &'static str {
        match self {
            Badge::None => "none",
            Badge::Waiting(_) => "waiting",
            Badge::Unknown(_) => "unknown",
        }
    }

    /// What hovering it explains.
    pub fn hint(self) -> &'static str {
        match self {
            Badge::None => "every node answered; nothing is waiting for you",
            Badge::Waiting(_) => "jobs are waiting for your verdict",
            Badge::Unknown(_) => "nothing queued, but some nodes did not answer",
        }
    }
}
