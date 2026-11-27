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
//! implements: 1d90deaa708a39eb606d0190ebbbab5169902bb5fb78a8e76acff87904e91414
//! implements: deda3d32832baaaaeb5814a0f1a859f80451d081d5f8e70d1def0a3d989c9dfe
//! implements: websocket-client@474e30c5309a55eb09aab1757f4a9c69717004aaabf715b091e989efe2a6e7d9

use std::sync::Arc;
use std::time::Duration;

use hub_protocol::discovery::TopologySnapshot;
use hub_protocol::errors::RouteError;
use hub_protocol::routing::{RoutedCall, RoutedReply};
use hub_protocol::service::{
    ConsumerApiClient, DiagnosticsApiClient, SessionIngressClient, SessionServiceDispatcher,
};
use hub_protocol::{SessionInfo, SessionMessage, SigningKey};
use kudo_worker::harness::HARNESS_ENV;
use kudo_worker::{WorkerSetup, session_info};
use tokio::sync::RwLock;

use crate::error::ShimError;
use crate::route::{RouteFailure, Target};
use crate::session::SessionInbox;

/// How long a call waits before the shim gives up **waiting**.
///
/// Generous, because the two ways of being wrong are not symmetric: too long
/// merely delays, while too short reports a timeout on work that was about to
/// succeed — and a timeout is the answer a caller is most likely to mishandle.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);

struct ClientLane {
    connection: vox::ConnectionHandle,
    client: ConsumerApiClient,
    diagnostics: Option<DiagnosticsApiClient>,
}

#[derive(Clone)]
struct SessionEndpoint {
    info: SessionInfo,
    inbox: SessionInbox,
}

impl SessionEndpoint {
    fn new(info: SessionInfo) -> Self {
        let inbox = SessionInbox::new(&info);
        Self { info, inbox }
    }

    fn current() -> Result<Self, ShimError> {
        let worker = WorkerSetup::from_env();
        let session = worker
            .as_ref()
            .map(|setup| setup.lease.clone())
            .or_else(|| {
                std::env::var(crate::SESSION_ID_ENV)
                    .ok()
                    .map(|id| id.trim().to_owned())
                    .filter(|id| !id.is_empty())
            });
        let info = session_info(
            session.as_deref(),
            kudo_worker::env::node_name(),
            std::env::var(HARNESS_ENV).unwrap_or_else(|_| "unknown".into()),
            std::env::current_dir()
                .map_err(|error| ShimError::Hub(error.to_string()))?
                .display()
                .to_string(),
            worker.as_ref().map(|setup| setup.context.0.clone()),
        )
        .map_err(|error| ShimError::Hub(error.to_string()))?;
        let inbox = SessionInbox::shared(worker.as_ref(), &info);
        Ok(Self { info, inbox })
    }
}

/// A live consumer connection to one hub.
///
/// Holds no mesh state of its own: every read goes to the hub, so two readers
/// never disagree about the world. The lane itself is replaceable — a hub that
/// restarts is re-dialed rather than ending the session.
// implements: b37a9e70ffbc60eea24a22bf9bb6d90244e1368ef7e4f2006893f71cc6317b07
pub struct HubClient {
    addr: String,
    /// Behind a lock so a dead lane can be swapped for a fresh one. The read
    /// lock is held only long enough to clone the handle, never across a call,
    /// so concurrent calls do not serialize behind each other.
    client: RwLock<Arc<ClientLane>>,
    session: Option<SessionEndpoint>,
    token: Option<String>,
    signing_key: Option<Arc<SigningKey>>,
    timeout: Duration,
}

impl Drop for HubClient {
    fn drop(&mut self) {
        let _ = self.client.get_mut().connection.shutdown();
    }
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

/// Open one consumer lane to `addr`, retaining its connection driver.
async fn dial_lane(
    addr: &str,
    token: Option<String>,
    session: Option<&SessionEndpoint>,
    signing_key: Option<Arc<SigningKey>>,
) -> Result<ClientLane, ShimError> {
    let connection = if addr.starts_with("ws://") || addr.starts_with("wss://") {
        dial_websocket(addr, session).await?
    } else {
        let socket = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| ShimError::HubUnreachable {
                addr: addr.to_string(),
                source,
            })?;

        let initiator = vox::initiator_on(vox::transport::tcp::StreamLink::tcp(socket));
        match session {
            Some(session) => {
                initiator
                    .on_lane(SessionServiceDispatcher::new(session.inbox.clone()))
                    .establish_connection()
                    .await
            }
            None => initiator.establish_connection().await,
        }
        .map_err(|e| ShimError::HubHandshake {
            addr: addr.to_string(),
            detail: e.to_string(),
        })?
    };
    let client: ConsumerApiClient =
        connection
            .open_lane()
            .await
            .map_err(|e| ShimError::HubHandshake {
                addr: addr.to_string(),
                detail: e.to_string(),
            })?;
    if let Some(key) = &signing_key {
        let challenge =
            client
                .authentication_challenge()
                .await
                .map_err(|e| ShimError::HubHandshake {
                    addr: addr.to_string(),
                    detail: format!("{e:?}"),
                })?;
        let proof = key.sign(&challenge).map_err(ShimError::Hub)?;
        client
            .authenticate_key(proof)
            .await
            .map_err(|e| ShimError::HubHandshake {
                addr: addr.to_string(),
                detail: format!("{e:?}"),
            })?;
    } else if let Some(token) = &token {
        client
            .authenticate(token.clone())
            .await
            .map_err(|e| ShimError::HubHandshake {
                addr: addr.to_string(),
                detail: format!("{e:?}"),
            })?;
    }
    if let Some(session) = session {
        let ingress: SessionIngressClient =
            connection
                .open_lane()
                .await
                .map_err(|e| ShimError::HubHandshake {
                    addr: addr.to_string(),
                    detail: format!("{e:?}"),
                })?;
        if let Some(key) = &signing_key {
            let challenge =
                ingress
                    .authentication_challenge()
                    .await
                    .map_err(|e| ShimError::HubHandshake {
                        addr: addr.to_string(),
                        detail: format!("{e:?}"),
                    })?;
            let proof = key.sign(&challenge).map_err(ShimError::Hub)?;
            ingress
                .authenticate_key(proof)
                .await
                .map_err(|e| ShimError::HubHandshake {
                    addr: addr.to_string(),
                    detail: format!("{e:?}"),
                })?;
        } else if let Some(token) = token.clone() {
            ingress
                .authenticate(token)
                .await
                .map_err(|e| ShimError::HubHandshake {
                    addr: addr.to_string(),
                    detail: format!("{e:?}"),
                })?;
        }
        ingress
            .register(session.info.clone())
            .await
            .map_err(|e| ShimError::HubHandshake {
                addr: addr.to_string(),
                detail: format!("{e:?}"),
            })?;
    }
    let diagnostics: Option<DiagnosticsApiClient> = connection.open_lane().await.ok();
    if let Some(diagnostics) = &diagnostics {
        if let Some(key) = &signing_key {
            let challenge = diagnostics.authentication_challenge().await.map_err(|e| {
                ShimError::HubHandshake {
                    addr: addr.to_string(),
                    detail: format!("{e:?}"),
                }
            })?;
            let proof = key.sign(&challenge).map_err(ShimError::Hub)?;
            diagnostics
                .authenticate_key(proof)
                .await
                .map_err(|e| ShimError::HubHandshake {
                    addr: addr.to_string(),
                    detail: format!("{e:?}"),
                })?;
        } else if let Some(token) = &token {
            diagnostics
                .authenticate(token.clone())
                .await
                .map_err(|e| ShimError::HubHandshake {
                    addr: addr.to_string(),
                    detail: format!("{e:?}"),
                })?;
        }
    }
    Ok(ClientLane {
        connection,
        client,
        diagnostics,
    })
}

/// Open a WebSocket Vox connection, including optional Cloudflare Access credentials.
async fn dial_websocket(
    addr: &str,
    session: Option<&SessionEndpoint>,
) -> Result<vox::ConnectionHandle, ShimError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let access = cloudflare_access_from_env().map_err(|detail| ShimError::HubHandshake {
        addr: addr.to_string(),
        detail,
    })?;
    let request = websocket_request(
        addr,
        access
            .as_ref()
            .map(|(id, secret)| (id.as_str(), secret.as_str())),
    )
    .map_err(|detail| ShimError::HubHandshake {
        addr: addr.to_string(),
        detail,
    })?;
    let (stream, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|error| ShimError::HubHandshake {
            addr: addr.to_string(),
            detail: error.to_string(),
        })?;
    let link = vox::transport::websocket::WsLink::new(stream);
    let initiator = vox::initiator_on(link);
    match session {
        Some(session) => {
            initiator
                .on_lane(SessionServiceDispatcher::new(session.inbox.clone()))
                .establish_connection()
                .await
        }
        None => initiator.establish_connection().await,
    }
    .map_err(|error| ShimError::HubHandshake {
        addr: addr.to_string(),
        detail: error.to_string(),
    })
}

fn cloudflare_access_from_env() -> Result<Option<(String, String)>, String> {
    let id = std::env::var("CF_ACCESS_CLIENT_ID").ok();
    let secret = std::env::var("CF_ACCESS_CLIENT_SECRET").ok();
    match (id, secret) {
        (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => {
            Ok(Some((id, secret)))
        }
        (None, None) => Ok(None),
        _ => Err(
            "CF_ACCESS_CLIENT_ID and CF_ACCESS_CLIENT_SECRET must both be nonempty and set together"
                .into(),
        ),
    }
}

fn websocket_request(
    addr: &str,
    access: Option<(&str, &str)>,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};

    let mut request = addr
        .into_client_request()
        .map_err(|error| format!("invalid WebSocket URL: {error}"))?;
    if let Some((id, secret)) = access {
        for (name, value) in [
            (HeaderName::from_static("cf-access-client-id"), id),
            (HeaderName::from_static("cf-access-client-secret"), secret),
        ] {
            let value = HeaderValue::from_str(value)
                .map_err(|error| format!("invalid Cloudflare Access credential: {error}"))?;
            request.headers_mut().insert(name, value);
        }
    }
    Ok(request)
}

///
/// Only a transport failure is worth re-dialing for. `VoxError::User` carries a
/// **domain** answer — a refusal, an unknown node — which reached someone and
/// will say the same thing again.
fn is_transport_failure<E>(e: &vox::VoxError<E>) -> bool {
    !matches!(e, vox::VoxError::User(_))
}

fn signing_key_from_env() -> Result<Option<Arc<SigningKey>>, ShimError> {
    let Ok(path) = std::env::var("HUB_PRIVATE_KEY_FILE") else {
        return Ok(None);
    };
    let key = SigningKey::from_private_file(kudo_worker::env::node_name(), path)
        .map_err(ShimError::Hub)?;
    Ok(Some(Arc::new(key)))
}

impl HubClient {
    /// Dial and register this agent session.
    pub async fn dial(addr: &str) -> Result<Self, ShimError> {
        Self::dial_with_endpoint(
            addr,
            std::env::var("HUB_AUTH_TOKEN").ok(),
            Some(SessionEndpoint::current()?),
            signing_key_from_env()?,
        )
        .await
    }

    /// Dial as a consumer without appearing as an agent session.
    pub async fn dial_consumer(addr: &str) -> Result<Self, ShimError> {
        Self::dial_with_endpoint(
            addr,
            std::env::var("HUB_AUTH_TOKEN").ok(),
            None,
            signing_key_from_env()?,
        )
        .await
    }

    /// Dial with explicit session metadata and credential.
    pub async fn dial_session_with_token(
        addr: &str,
        token: &str,
        info: hub_protocol::SessionInfo,
    ) -> Result<Self, ShimError> {
        Self::dial_with_endpoint(
            addr,
            Some(token.to_owned()),
            Some(SessionEndpoint::new(info)),
            None,
        )
        .await
    }

    /// Dial and register this agent session with an explicit credential.
    pub async fn dial_with_token(addr: &str, token: &str) -> Result<Self, ShimError> {
        Self::dial_with_endpoint(
            addr,
            Some(token.to_owned()),
            Some(SessionEndpoint::current()?),
            None,
        )
        .await
    }

    async fn dial_with_endpoint(
        addr: &str,
        token: Option<String>,
        session: Option<SessionEndpoint>,
        signing_key: Option<Arc<SigningKey>>,
    ) -> Result<Self, ShimError> {
        Ok(Self {
            addr: addr.to_string(),
            client: RwLock::new(Arc::new(
                dial_lane(addr, token.clone(), session.as_ref(), signing_key.clone()).await?,
            )),
            session,
            token,
            signing_key,
            timeout: DEFAULT_CALL_TIMEOUT,
        })
    }

    /// Bound every call by `timeout` instead of the default.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The hub address this client is talking to.
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// How long a call waits before giving up waiting.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The lane as it stands now.
    async fn lane(&self) -> Arc<ClientLane> {
        self.client.read().await.clone()
    }

    /// Replace the lane — unless someone else already did.
    ///
    /// Two calls failing at once must not produce two re-dials: the second finds
    /// the stored lane is no longer the one it was holding, and takes that.
    async fn redial(&self, stale: &Arc<ClientLane>) -> Result<Arc<ClientLane>, ShimError> {
        let mut guard = self.client.write().await;
        if !Arc::ptr_eq(&guard, stale) {
            return Ok(guard.clone());
        }
        let fresh = Arc::new(
            dial_lane(
                &self.addr,
                self.token.clone(),
                self.session.as_ref(),
                self.signing_key.clone(),
            )
            .await?,
        );
        let old = std::mem::replace(&mut *guard, fresh.clone());
        let _ = old.connection.shutdown();
        Ok(fresh)
    }

    /// Bound a call by this client's timeout.
    ///
    /// This stops the **waiting**, not the work — which is why `ShimError`'s
    /// timeout variant says so in as many words.
    async fn bounded<T>(
        &self,
        what: &str,
        future: impl std::future::Future<Output = T>,
    ) -> Result<T, ShimError> {
        tokio::time::timeout(self.timeout, future)
            .await
            .map_err(|_| ShimError::TimedOut {
                what: what.to_string(),
                after: self.timeout,
            })
    }

    /// A point-in-time view of the connected mesh.
    ///
    /// Reads are link-agnostic, so no link selection is involved
    /// (`hub:08c3ce357742ccf578caddcfb6578a92216b515adcfe1ac9de4543406be52cb0`).
    pub async fn topology(&self) -> Result<TopologySnapshot, ShimError> {
        let lane = self.lane().await;
        match self.bounded("topology", lane.client.topology()).await? {
            Ok(snapshot) => Ok(snapshot),
            Err(e) if is_transport_failure(&e) => {
                // The lane died. Re-dial once and ask again; if the dial fails,
                // that error names the address, which is what to act on.
                let fresh = self.redial(&lane).await?;
                self.bounded("topology", fresh.client.topology())
                    .await?
                    .map_err(|e| ShimError::Hub(format!("{e:?}")))
            }
            Err(e) => Err(ShimError::Hub(format!("{e:?}"))),
        }
    }

    /// Probe a session's registration and callback without touching its mailbox.
    pub async fn session_probe(
        &self,
        target: String,
        probe_id: String,
    ) -> Result<hub_protocol::DiagnosticReport, ShimError> {
        let lane = self.lane().await;
        let Some(diagnostics) = lane.diagnostics.clone() else {
            let mut report =
                hub_protocol::DiagnosticReport::new(probe_id, self.addr.clone(), target);
            report.observed_at_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            report.checks.push(hub_protocol::DiagnosticCheck {
                stage: "hub".into(),
                status: hub_protocol::DiagnosticStatus::Unsupported,
                detail: "DiagnosticsApi is unavailable".into(),
            });
            return Ok(report.finish());
        };
        self.bounded("session probe", diagnostics.session_probe(target, probe_id))
            .await?
            .map_err(|error| ShimError::Hub(format!("{error:?}")))
    }

    /// List live local and one-hop peer sessions.
    pub async fn sessions(&self) -> Result<Vec<SessionInfo>, ShimError> {
        let lane = self.lane().await;
        let result = self.bounded("session list", lane.client.sessions()).await;
        match result {
            Err(ShimError::TimedOut { .. }) => {
                let fresh = self.redial(&lane).await?;
                self.bounded("session list", fresh.client.sessions())
                    .await?
                    .map_err(|error| ShimError::Hub(format!("{error:?}")))
            }
            Err(error) => Err(error),
            Ok(Ok(sessions)) => Ok(sessions),
            Ok(Err(error)) if is_transport_failure(&error) => {
                let fresh = self.redial(&lane).await?;
                self.bounded("session list", fresh.client.sessions())
                    .await?
                    .map_err(|error| ShimError::Hub(format!("{error:?}")))
            }
            Ok(Err(error)) => Err(ShimError::Hub(format!("{error:?}"))),
        }
    }

    /// Send a direct message; transport failure is not retried implicitly.
    pub async fn send_session(
        &self,
        target: String,
        message_id: String,
        body: String,
    ) -> Result<(), ShimError> {
        let lane = self.lane().await;
        let result = self
            .bounded(
                "session message",
                lane.client.send_session(target, message_id, body),
            )
            .await;
        let result = match result {
            Err(error @ ShimError::TimedOut { .. }) => {
                let _ = tokio::time::timeout(Duration::from_secs(3), self.redial(&lane)).await;
                return Err(error);
            }
            Err(error) => return Err(error),
            Ok(result) => result,
        };
        match result {
            Ok(()) => Ok(()),
            Err(error) if is_transport_failure(&error) => {
                // Refresh the registration for the caller's retry, but never replay
                // an ambiguous send: the first attempt may already have landed.
                let _ = tokio::time::timeout(Duration::from_secs(3), self.redial(&lane)).await;
                Err(ShimError::Hub(format!("{error:?}")))
            }
            Err(error) => Err(ShimError::Hub(format!("{error:?}"))),
        }
    }

    /// Drain this registered session's local mailbox.
    pub fn take_session_messages(&self) -> Result<Vec<SessionMessage>, ShimError> {
        self.session
            .as_ref()
            .ok_or_else(|| ShimError::Hub("this connection is not a session".into()))?
            .inbox
            .take()
            .map_err(ShimError::Hub)
    }

    /// Route one call to a connector on the selected node/link.
    ///
    /// The payload is **opaque here** — its meaning is a private agreement
    /// between the connector and its callers, and neither the hub nor this shim
    /// decodes it
    /// (`hub:d78fb95fddc31aa42bc927da1eaed44292529b3671d1ef9ef58ec8cd0858e51c`).
    /// That is also what keeps a component-version mismatch harmless: bytes
    /// cross the boundary, never `Facet`-derived types.
    ///
    /// A failure keeps **which side failed**, because that is the only part a
    /// caller can act on
    /// (`hub:45af8b01ab1929e94682ed82013a4aad0b48af81c61b3dea89d7b8323cf94752`).
    /// A **domain** failure is returned as-is and never retried: it reached
    /// someone and was answered, and retrying a `start` would run the work
    /// twice.
    pub async fn route(&self, target: Target, call: RoutedCall) -> Result<RoutedReply, ShimError> {
        let what = format!(
            "{}.{} on node '{}'",
            call.connector, call.method, target.node
        );
        let lane = self.lane().await;

        let error = match self
            .bounded(&what, lane.client.route(target.selector(), call.clone()))
            .await?
        {
            Ok(reply) => return Ok(reply),
            Err(e) if is_transport_failure(&e) => {
                let fresh = self.redial(&lane).await?;
                match self
                    .bounded(&what, fresh.client.route(target.selector(), call.clone()))
                    .await?
                {
                    Ok(reply) => return Ok(reply),
                    Err(e) => e,
                }
            }
            Err(e) => e,
        };

        let (side, detail) = match &error {
            vox::VoxError::User(route_error) => match route_error.as_ref() {
                RouteError::Hub(hub) => (RouteFailure::Hub, format!("{hub:?}")),
                RouteError::Node(node) => (RouteFailure::Node, format!("{node:?}")),
            },
            // Transport-level, and a re-dial did not help. From the caller's
            // point of view the hub could not deliver.
            other => (RouteFailure::Hub, format!("{other:?}")),
        };
        Err(ShimError::Route {
            side,
            node: target.node,
            connector: call.connector,
            method: call.method,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_service_tokens_are_websocket_headers() {
        let request = websocket_request(
            "wss://hub_edger.yjpark.org",
            Some(("client-id", "client-secret")),
        )
        .unwrap();
        assert_eq!(request.headers()["cf-access-client-id"], "client-id");
        assert_eq!(
            request.headers()["cf-access-client-secret"],
            "client-secret"
        );
        let plain = websocket_request("ws://127.0.0.1:15400", None).unwrap();
        assert!(!plain.headers().contains_key("cf-access-client-id"));
    }
}
