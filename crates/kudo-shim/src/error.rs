//! What can go wrong, said in a way the caller can act on.
//!
//! An unactionable error costs a human round-trip, which is precisely the cost
//! this whole system exists to remove — so every variant here names the thing
//! you would need to know next.

use std::fmt;

use crate::route::RouteFailure;

/// A failure reaching or talking to the mesh.
#[derive(Debug)]
pub enum ShimError {
    /// The hub could not be reached at all.
    ///
    /// This names the address because there is **nothing else to try**: a node
    /// is addressed only through the hub and has no local listener
    /// (`hub:bf406db5b660bd595cccdfcec5921121bb5a1ac9281a0d9a5c28be2d7ccc79b4`),
    /// so "the hub is down" is the entire story, and an empty result would be
    /// indistinguishable from a healthy but empty mesh.
    HubUnreachable {
        addr: String,
        source: std::io::Error,
    },
    /// The connection was reached but the session could not be established.
    HubHandshake { addr: String, detail: String },
    /// The hub answered, but with a failure.
    Hub(String),
    /// The call did not answer inside its budget.
    ///
    /// **This is not a cancellation.** The shim stopped waiting; the far side
    /// may still be running and may still finish. Reporting it as a plain
    /// failure is how a caller ends up starting the same work twice.
    TimedOut {
        what: String,
        after: std::time::Duration,
    },
    /// A routed call failed, and this says **which side** failed.
    ///
    /// Flattening the two into one string would be simpler and would erase the
    /// only thing a caller can act on: a hub failure usually means a stale view
    /// of the mesh, a node failure means the connector refused.
    Route {
        side: RouteFailure,
        node: String,
        connector: String,
        method: String,
        detail: String,
    },
}

impl ShimError {
    /// Which side a routed call failed on, if this was a routing failure.
    pub fn failure(&self) -> Option<RouteFailure> {
        match self {
            ShimError::Route { side, .. } => Some(*side),
            _ => None,
        }
    }
}

impl fmt::Display for ShimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShimError::HubUnreachable { addr, source } => write!(
                f,
                "cannot reach the hub at {addr}: {source}. \
                 Every node is addressed through the hub and none has a local \
                 listener, so there is no fallback path — start the hub, or point \
                 this shim at the right address."
            ),
            ShimError::HubHandshake { addr, detail } => write!(
                f,
                "connected to {addr} but could not open a consumer lane: {detail}"
            ),
            ShimError::Hub(detail) => write!(f, "the hub refused the request: {detail}"),
            ShimError::TimedOut { what, after } => write!(
                f,
                "{what} did not answer within {:.0}s. This is a timeout, not a \
                 cancellation — the work may still be running, and may still \
                 finish. Check what is actually there (job_list) before starting \
                 it again, or allow more time.",
                after.as_secs_f64()
            ),
            // Name the side in words, not just in a variant: whoever reads this
            // has to decide between "my view is stale" and "it said no".
            ShimError::Route {
                side: RouteFailure::Hub,
                node,
                detail,
                ..
            } => write!(
                f,
                "the hub could not deliver the call to node '{node}': {detail}. \
                 This is a delivery failure, not a refusal — the node may be \
                 gone or renamed, so check what is connected before retrying."
            ),
            ShimError::Route {
                side: RouteFailure::Node,
                node,
                connector,
                method,
                detail,
            } => write!(
                f,
                "node '{node}' received the call but connector '{connector}' \
                 refused '{method}': {detail}. This reached the far side and was \
                 rejected — retrying unchanged will fail the same way."
            ),
        }
    }
}

impl std::error::Error for ShimError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ShimError::HubUnreachable { source, .. } => Some(source),
            _ => None,
        }
    }
}
