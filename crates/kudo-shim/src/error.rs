//! What can go wrong, said in a way the caller can act on.
//!
//! An unactionable error costs a human round-trip, which is precisely the cost
//! this whole system exists to remove — so every variant here names the thing
//! you would need to know next.

use std::fmt;

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
