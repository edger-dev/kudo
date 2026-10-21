//! What the shim needs to start.
//!
//! implements: 482bd420c2a2251c6d5c2fa311e8a14b2bb0898cf84cedfbef88fe6ba8a119ae@482bd420c2a2251c6d5c2fa311e8a14b2bb0898cf84cedfbef88fe6ba8a119ae

use crate::ShimError;

/// How the shim starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Startup {
    /// Dial the hub at this address.
    Dial(String),
    /// Serve without a hub; the job tools give this reason.
    WithoutHub(String),
    /// Do not start; say this.
    Refuse(String),
}

/// The start for a hub address (if any) and whether this is a worker session.
pub fn plan_startup(hub_addr: Option<String>, worker: bool) -> Startup {
    let _ = (hub_addr, worker);
    Startup::Refuse(String::new())
}

/// What to do when dialing the hub failed.
pub fn after_dial_failure(err: &ShimError, worker: bool) -> Startup {
    let _ = (err, worker);
    Startup::Refuse(String::new())
}
