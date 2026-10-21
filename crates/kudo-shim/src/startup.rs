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
    let hub_addr = hub_addr.filter(|a| !a.trim().is_empty());
    match (hub_addr, worker) {
        (Some(addr), _) => Startup::Dial(addr),
        (None, true) => Startup::WithoutHub(format!("{} is not set", crate::HUB_ADDR_ENV)),
        (None, false) => Startup::Refuse(format!(
            "{env} is not set. Every node is reached through the hub and none has a local \
             listener, so there is no default to fall back to — set {env} to the hub's \
             address (e.g. 127.0.0.1:15400).",
            env = crate::HUB_ADDR_ENV
        )),
    }
}

/// What to do when dialing the hub failed.
///
/// A worker session goes on without it: its loop needs the context store, and
/// a hub that is down must not stop the work. A session that is only job tools
/// has nothing to offer without a hub, and says so at once rather than looking
/// healthy and failing every call.
pub fn after_dial_failure(err: &ShimError, worker: bool) -> Startup {
    if worker {
        Startup::WithoutHub(format!("the hub did not answer ({err})"))
    } else {
        Startup::Refuse(err.to_string())
    }
}
