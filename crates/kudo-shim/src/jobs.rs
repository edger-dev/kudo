//! Talking to a job connector on a node.
//!
//! Every function here is encode → route → decode, with the **engine's** request
//! and reply types. What crosses the hub is bytes and nothing else.
//!
//! **This module never names `facet::Facet`.** moco's wire types derive *moco's*
//! facet (0.43); this crate's `facet` is hub's (0.50), and they are different
//! traits with the same name. Writing a generic helper bounded by the local
//! `Facet` therefore does not compile against the engine's types — which is the
//! version skew showing up as a compile error, exactly where it should. Concrete
//! types, encoded and decoded by the engine's own functions, keep the bound
//! inside moco where it resolves correctly.
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

use hub_protocol::routing::RoutedCall;
use moco_job::wire::{
    ClearReply, ClearRequest, EnsureReply, EnsureRequest, KillReply, KillRequest, ListReply,
    MachineReply, MachineRequest, RestartRequest, StartNamedRequest, StartReply, StartRequest,
    TailReply, TailRequest, WireCaller,
};

use crate::error::ShimError;
use crate::hub::HubClient;
use crate::route::Target;

/// The connector id a node conventionally offers its job substrate under.
///
/// A default rather than a discovery rule: an explicit id is one argument a
/// caller can always override, whereas inferring which connector "is" the job
/// one would be a rule that has to live somewhere and be right everywhere.
pub const DEFAULT_JOB_CONNECTOR: &str = "jobs-0";

/// Who this shim is, for the engine's write scoping.
///
/// Always a **session**, identified by the directory the shim was launched in —
/// which is the session's project directory, inherited for free. There is
/// deliberately no way for this function to return `Console`: global write
/// authority belongs to the human console, and an agent face that could claim it
/// would make the whole scoping rule advisory.
///
/// The shim sends *where it is*. Turning that into a workspace is the engine's
/// rule, and a second face would need the same one, so it does not live here.
///
/// implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120
pub fn session_caller() -> WireCaller {
    WireCaller::Session {
        cwd: std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn encode_failed(detail: String) -> ShimError {
    ShimError::Hub(format!("could not encode the request: {detail}"))
}

fn decode_failed(detail: String) -> ShimError {
    ShimError::Hub(format!("could not decode the engine's reply: {detail}"))
}

async fn route(
    hub: &HubClient,
    target: Target,
    connector: &str,
    method: &str,
    payload: Vec<u8>,
) -> Result<Vec<u8>, ShimError> {
    let reply = hub
        .route(
            target,
            RoutedCall {
                connector: connector.to_string(),
                method: method.to_string(),
                payload,
            },
        )
        .await?;
    Ok(reply.payload)
}

/// Every job the node's substrate knows about.
pub async fn list(
    hub: &HubClient,
    target: Target,
    connector: &str,
) -> Result<ListReply, ShimError> {
    // `list` takes no arguments, but the engine still expects a well-formed
    // record rather than an empty body.
    let bytes = route(hub, target, connector, "list", b"{}".to_vec()).await?;
    moco_job::wire::decode::<ListReply>(&bytes).map_err(decode_failed)
}

/// Start a job on the node.
pub async fn start(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: StartRequest,
) -> Result<StartReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "start", payload).await?;
    moco_job::wire::decode::<StartReply>(&bytes).map_err(decode_failed)
}

/// Read a job's output from an offset, with its live status.
pub async fn tail(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: TailRequest,
) -> Result<TailReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "tail", payload).await?;
    moco_job::wire::decode::<TailReply>(&bytes).map_err(decode_failed)
}

/// Terminate a job.
pub async fn kill(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: KillRequest,
) -> Result<KillReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "kill", payload).await?;
    moco_job::wire::decode::<KillReply>(&bytes).map_err(decode_failed)
}

/// Start the job this workspace declares under `name`.
pub async fn start_named(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: StartNamedRequest,
) -> Result<StartReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "start_named", payload).await?;
    moco_job::wire::decode::<StartReply>(&bytes).map_err(decode_failed)
}

/// Stop a declared job and start it from its current declaration.
pub async fn restart(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: RestartRequest,
) -> Result<StartReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "restart", payload).await?;
    moco_job::wire::decode::<StartReply>(&bytes).map_err(decode_failed)
}

/// Start this workspace's `session` entries that are not already running.
pub async fn ensure(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: EnsureRequest,
) -> Result<EnsureReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "ensure", payload).await?;
    moco_job::wire::decode::<EnsureReply>(&bytes).map_err(decode_failed)
}

/// Remove this workspace's terminal entries.
pub async fn clear(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: ClearRequest,
) -> Result<ClearReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "clear", payload).await?;
    moco_job::wire::decode::<ClearReply>(&bytes).map_err(decode_failed)
}

/// Read a job through its machine lens.
pub async fn machine(
    hub: &HubClient,
    target: Target,
    connector: &str,
    request: MachineRequest,
) -> Result<MachineReply, ShimError> {
    let payload = moco_job::wire::encode(&request).map_err(encode_failed)?;
    let bytes = route(hub, target, connector, "machine", payload).await?;
    moco_job::wire::decode::<MachineReply>(&bytes).map_err(decode_failed)
}

/// Render a machine-lens read, saying which channel it came from.
///
/// The source label is not decoration: scrollback and a declared view need
/// reading differently, and a caller that cannot tell them apart will try to
/// parse the wrong one.
pub fn render_machine(reply: &MachineReply) -> String {
    let source = match reply.source {
        moco_job::LensSource::Machine => {
            if reply.format.is_empty() {
                "machine view".to_string()
            } else {
                format!("machine view ({})", reply.format)
            }
        }
        moco_job::LensSource::Scrollback => {
            "scrollback — this job declares no machine view".to_string()
        }
    };
    let body = String::from_utf8_lossy(&reply.bytes);
    if body.is_empty() {
        return format!(
            "source: {source}\nnext_offset: {}\n---\n(nothing yet)",
            reply.next_offset
        );
    }
    format!(
        "source: {source}\nnext_offset: {}\n---\n{body}",
        reply.next_offset
    )
}

/// Render a job listing compactly.
///
/// Shows each job's **owner**, because reads are node-global: a listing includes
/// other workspaces' jobs, and which ones are writable depends on that. The
/// owner is shown rather than reduced to "yours"/"theirs" — deciding that would
/// mean resolving a directory to a workspace here, which is the engine's rule.
pub fn render_jobs(reply: &ListReply) -> String {
    if reply.jobs.is_empty() {
        return "no jobs on this node".to_string();
    }
    reply
        .jobs
        .iter()
        .map(|j| {
            let mut line = format!("{}  {:?}", j.id, j.status);
            if !j.name.is_empty() {
                line.push_str(&format!("  {}", j.name));
            }
            if j.port != 0 {
                line.push_str(&format!("  :{}", j.port));
            }
            if j.restarts > 0 {
                line.push_str(&format!("  ({} restarts)", j.restarts));
            }
            if j.external {
                line.push_str("  [adopted]");
            }
            line.push_str(&format!("  [{}]", j.scope));
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Render a tail: the status, the next offset to resume from, then the output.
///
/// The bytes are shown lossily **at this last step only** — the engine carried
/// them verbatim, and this is the point where they become something to read.
pub fn render_tail(reply: &TailReply) -> String {
    format!(
        "status: {:?}\nnext_offset: {}\n---\n{}",
        reply.status,
        reply.next_offset,
        String::from_utf8_lossy(&reply.bytes)
    )
}
