//! The consumer end of the hub.
//!
//! The shim dials `ConsumerApi` — the audience-split service a reader/router
//! peer is meant to see
//! (`hub:ba47c87abc0f961967d5ccfa9254945075c16c1e9a3b8eaed1e8195e1663c241`). It
//! never dials `NodeIngress`, and it never reaches a node directly: a node is
//! addressed only by registry name through the hub
//! (`hub:bf406db5b660bd595cccdfcec5921121bb5a1ac9281a0d9a5c28be2d7ccc79b4`).
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

use hub_protocol::discovery::TopologySnapshot;
use hub_protocol::service::ConsumerApiClient;

use crate::error::ShimError;

/// A live consumer connection to one hub.
///
/// Holds no mesh state of its own: every read goes to the hub, so two readers
/// never disagree about the world.
pub struct HubClient {
    addr: String,
    client: ConsumerApiClient,
}

impl std::fmt::Debug for HubClient {
    /// Only the address. The underlying lane is a live connection, not
    /// information, and printing it would put transport internals in messages
    /// that are meant to be read by a person or an agent.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubClient")
            .field("addr", &self.addr)
            .finish()
    }
}

impl HubClient {
    /// Dial the hub at `addr` and open a consumer lane.
    ///
    /// A failure here is terminal for the session rather than something to
    /// degrade around — there is no node-local path to fall back to.
    pub async fn dial(addr: &str) -> Result<Self, ShimError> {
        let socket = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| ShimError::HubUnreachable {
                addr: addr.to_string(),
                source,
            })?;

        let client = vox::initiator_on(vox::transport::tcp::StreamLink::tcp(socket))
            .establish_connection()
            .await
            .map_err(|e| ShimError::HubHandshake {
                addr: addr.to_string(),
                detail: e.to_string(),
            })?
            .open_lane()
            .await
            .map_err(|e| ShimError::HubHandshake {
                addr: addr.to_string(),
                detail: e.to_string(),
            })?;

        Ok(Self {
            addr: addr.to_string(),
            client,
        })
    }

    /// The hub address this client is talking to.
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// A point-in-time view of the connected mesh.
    ///
    /// Reads are link-agnostic, so no link selection is involved
    /// (`hub:08c3ce357742ccf578caddcfb6578a92216b515adcfe1ac9de4543406be52cb0`).
    pub async fn topology(&self) -> Result<TopologySnapshot, ShimError> {
        self.client
            .topology()
            .await
            .map_err(|e| ShimError::Hub(e.to_string()))
    }
}
