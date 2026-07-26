//! The MCP surface the agent sees.
//!
//! Every tool here is a **proxy**: it asks the mesh and returns the answer. No
//! tool computes something an engine could have answered, and none caches — two
//! readers must never see different worlds.
//!
//! The surface is deliberately small. Agent tool surface is a scarce resource,
//! and near-identical tools are a real cost rather than clutter: each one is a
//! chance to pick the wrong one
//! (`kudo:a9a94ad3464ebf50c30bffcfe64d4596384d5d122c5dfb3fb992b1aa73cd7eba`).
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

use std::sync::Arc;

use moco_job::wire::{KillRequest, StartRequest, TailRequest};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::schemars;
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;

use crate::error::ShimError;
use crate::hub::HubClient;
use crate::jobs::{self, DEFAULT_JOB_CONNECTOR};
use crate::mesh::render_mesh;
use crate::route::Target;

/// Which node (and optionally which link) a call is addressed to.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct NodeArgs {
    /// The node's registry name, as shown by `nodes`.
    pub node: String,
    /// Which link of that node. Omit for the node's default (`stable`); the hub
    /// falls back to `stable` if the named link is not connected.
    pub link: Option<String>,
    /// The job connector's id on that node. Omit for the conventional default.
    pub connector: Option<String>,
}

/// Start a job.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct StartArgs {
    pub node: String,
    pub link: Option<String>,
    pub connector: Option<String>,
    /// The command as an **argument vector** — `["cargo", "test"]`, never a
    /// shell string. Shell metacharacters are inert data, so quoting tricks do
    /// nothing; a command needing a pipeline ships a wrapper program as its
    /// first element.
    pub argv: Vec<String>,
    /// Working directory. Omit to use this session's own directory, which is
    /// almost always what you want.
    pub cwd: Option<String>,
    /// Give up after this many milliseconds. Omit for no deadline.
    pub timeout_ms: Option<u64>,
}

/// Read a job's output.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct TailArgs {
    pub node: String,
    pub link: Option<String>,
    pub connector: Option<String>,
    /// The job id returned by `job_start`.
    pub id: String,
    /// Resume from this byte offset; use the `next_offset` from the previous
    /// read to get only what is new. Omit to read from the beginning.
    pub offset: Option<u64>,
}

/// Terminate a job.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct KillArgs {
    pub node: String,
    pub link: Option<String>,
    pub connector: Option<String>,
    pub id: String,
}

fn target(node: &str, link: &Option<String>) -> Target {
    match link {
        Some(link) => Target::link(node, link),
        None => Target::node(node),
    }
}

fn connector_id(connector: &Option<String>) -> String {
    connector
        .clone()
        .unwrap_or_else(|| DEFAULT_JOB_CONNECTOR.to_string())
}

/// Turn a result into a tool reply, keeping a failure a *failure*.
///
/// A refusal rendered as ordinary output would read as success to an agent,
/// which is how a caller ends up believing work happened.
fn reply(result: Result<String, crate::ShimError>) -> Result<CallToolResult, ErrorData> {
    Ok(match result {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
    })
}

/// The agent-facing server.
///
/// Holds a hub connection and nothing else — no job state, no mesh cache.
#[derive(Clone)]
pub struct ShimServer {
    hub: Arc<HubClient>,
}

#[tool_router]
impl ShimServer {
    /// Build a server over an already-dialed hub connection.
    pub fn new(hub: Arc<HubClient>) -> Self {
        Self { hub }
    }

    /// The hub this server proxies to.
    pub fn hub(&self) -> &HubClient {
        &self.hub
    }

    #[tool(
        name = "nodes",
        description = "List the machines currently reachable through the hub, each link they \
                       have connected, and the capabilities (connectors) that link offers. Use \
                       this to find out what exists before addressing work to a node."
    )]
    async fn nodes(&self) -> Result<CallToolResult, ErrorData> {
        // No arguments: reads are link-agnostic, so there is no link to select
        // and nothing for the caller to get wrong.
        match self.hub.topology().await {
            Ok(snapshot) => Ok(CallToolResult::success(vec![ContentBlock::text(
                render_mesh(&snapshot),
            )])),
            // Surface the failure as a tool error rather than as empty output:
            // "nothing is connected" and "the hub did not answer" must never
            // look alike to the agent.
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(
                e.to_string(),
            )])),
        }
    }

    #[tool(
        name = "job_list",
        description = "List the jobs a node's substrate knows about, with each one's state. \
                       Jobs outlive the session that started them, so work started earlier \
                       shows up here too."
    )]
    async fn job_list(
        &self,
        Parameters(args): Parameters<NodeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::list(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
            )
            .await
            .map(|r| jobs::render_jobs(&r)),
        )
    }

    #[tool(
        name = "job_start",
        description = "Start a command as a job on a node and get its id back immediately. The \
                       job keeps running after this call returns and after this session ends. \
                       Read its output with job_tail; stop it with job_kill."
    )]
    async fn job_start(
        &self,
        Parameters(args): Parameters<StartArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        // The working directory defaults to **this shim's own**, which is the
        // session's project directory, inherited for free because the shim runs
        // as a child of the session. That is the whole reason workspace identity
        // needs no configuration and cannot be set to the wrong thing by
        // accident.
        let cwd = match args.cwd {
            Some(cwd) => cwd,
            None => match std::env::current_dir() {
                Ok(cwd) => cwd.to_string_lossy().into_owned(),
                Err(e) => {
                    return reply(Err(ShimError::Hub(format!(
                        "no cwd was given and this session's own directory could not be read: {e}"
                    ))));
                }
            },
        };

        reply(
            jobs::start(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                StartRequest {
                    argv: args.argv,
                    cwd,
                    deadline_ms: args.timeout_ms.unwrap_or(0),
                },
            )
            .await
            .map(|r| format!("started job {}", r.id)),
        )
    }

    #[tool(
        name = "job_tail",
        description = "Read a job's output and its current state. Pass the previous reply's \
                       next_offset to get only what is new. This is also how you learn a job \
                       finished — the state comes back with every read."
    )]
    async fn job_tail(
        &self,
        Parameters(args): Parameters<TailArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::tail(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                TailRequest {
                    id: args.id,
                    offset: args.offset.unwrap_or(0),
                },
            )
            .await
            .map(|r| jobs::render_tail(&r)),
        )
    }

    #[tool(
        name = "job_kill",
        description = "Stop a running job. Its record and its output stay readable afterwards, \
                       so you can still see how it ended."
    )]
    async fn job_kill(
        &self,
        Parameters(args): Parameters<KillArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::kill(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                KillRequest { id: args.id },
            )
            .await
            .map(|r| format!("kill requested for job {}", r.id)),
        )
    }
}

/// The server's own identity and hints.
///
/// Named for **this** shim, not for the SDK that carries it: the agent sees this
/// name when choosing a tool, and "rmcp" would tell it nothing about what is on
/// the other side.
#[tool_handler]
impl ServerHandler for ShimServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info.name = "kudo-shim".to_string();
        info.server_info.version = env!("CARGO_PKG_VERSION").to_string();
        info.instructions = Some(
            "Reaches capabilities running on machines in the kudo mesh, through the hub. \
             Start with `nodes` to see which machines are connected and what each can do; \
             address later work to a node by name."
                .to_string(),
        );
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A compile-time assertion that the macro produced a real MCP server:
    /// without this, a change to the attribute could silently leave the tools
    /// unrouted and nothing would fail until a client connected.
    #[test]
    fn the_macro_generates_a_server_handler() {
        fn assert_handler<T: rmcp::ServerHandler>() {}
        assert_handler::<ShimServer>();
    }
}
