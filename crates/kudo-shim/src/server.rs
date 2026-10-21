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

use moco_job::wire::{
    ClearRequest, EnsureRequest, KillRequest, MachineRequest, RestartRequest, ScreenRequest,
    StartNamedRequest, StartRequest, StatsRequest, TailRequest,
};
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
use crate::worker_tools;
use kudo_context::Now;
use kudo_worker::WorkerSetup;

/// Seconds since the Unix epoch.
fn now() -> Now {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

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

/// Start a job this workspace declares.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct StartNamedArgs {
    pub node: String,
    pub link: Option<String>,
    pub connector: Option<String>,
    /// The name the workspace's manifest declares it under.
    pub name: String,
}

/// Restart a declared job.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct RestartArgs {
    pub node: String,
    pub link: Option<String>,
    pub connector: Option<String>,
    /// The job id, from job_list.
    pub id: String,
}

/// Start a job ad-hoc.
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

/// Submit the work for a claim.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct SubmitArgs {
    /// The claim id, from `pull` or the item you were handed.
    pub claim: String,
    /// The **full** new head, as you leave it: where it stands (verdict
    /// first), the next action and who takes it, what is established with its
    /// evidence, and what is still open. It replaces the old head whole.
    pub head: String,
    /// The log entry: what you established, with evidence, and what you ruled out.
    pub log: String,
    /// Only after a refused submit: the head version(s) the refusal gave you,
    /// which your new head builds on.
    #[serde(default)]
    pub base: Option<String>,
}

/// Head versions as a refusal gives them: comma-separated.
fn split_versions(base: &str) -> Vec<String> {
    base.split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

/// File a rough edge.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct InboxArgs {
    /// The rough edge in one line: what got in the way.
    pub line: String,
    /// More detail, if one line is not enough.
    pub body: Option<String>,
}

/// Give a claim back.
#[derive(Deserialize, schemars::JsonSchema, Debug)]
pub struct ReleaseArgs {
    /// The claim id.
    pub claim: String,
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
    /// Set when this session is a worker (the launcher's environment says so).
    worker: Option<WorkerSetup>,
}

#[tool_router]
impl ShimServer {
    /// Build a server over an already-dialed hub connection.
    pub fn new(hub: Arc<HubClient>) -> Self {
        Self { hub, worker: None }
    }

    /// The same server, answering the worker tools for `setup`.
    pub fn with_worker(mut self, setup: Option<WorkerSetup>) -> Self {
        self.worker = setup;
        self
    }

    fn worker_call(
        &self,
        f: impl FnOnce(&WorkerSetup, Now) -> Result<String, String>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = match &self.worker {
            Some(setup) => f(setup, now()),
            None => Err(
                "this session is not a kudo worker: it was not started under a \
                         context (KUDO_CONTEXT is unset)"
                    .to_string(),
            ),
        };
        Ok(match result {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
        })
    }

    #[tool(
        name = "context_read",
        description = "Read the head of the context this worker is bound to: where the work \
                       stands, the next action, what is established and what is open."
    )]
    async fn context_read(&self) -> Result<CallToolResult, ErrorData> {
        self.worker_call(|s, _| worker_tools::context_read(s))
    }

    #[tool(
        name = "pull",
        description = "Claim the next item queued under this worker's context. Returns the \
                       ask and a claim id; work it, then `submit` with that id. Says so when \
                       nothing is ready."
    )]
    async fn pull(&self) -> Result<CallToolResult, ErrorData> {
        self.worker_call(worker_tools::pull)
    }

    #[tool(
        name = "submit",
        description = "Hand back the work for a claim: the full new head of the context and a \
                       log entry citing your evidence. The head is superseded, never edited; \
                       the claim is settled."
    )]
    async fn submit(
        &self,
        Parameters(args): Parameters<SubmitArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.worker_call(|s, now| {
            let submission = kudo_context::Submission {
                head: args.head.clone(),
                log: args.log.clone(),
                base: args.base.as_deref().map(split_versions),
                ..Default::default()
            };
            worker_tools::submit(s, &args.claim, &submission, now)
        })
    }

    #[tool(
        name = "inbox",
        description = "File a rough edge you hit while working — a tool that misbehaved, a \
                       missing capability, an instruction that did not fit — in one line, \
                       with an optional body. It lands in this context's inbox for triage; \
                       it does not change the work."
    )]
    async fn inbox(
        &self,
        Parameters(args): Parameters<InboxArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.worker_call(|s, now| worker_tools::inbox(s, &args.line, args.body.as_deref(), now))
    }

    #[tool(
        name = "release",
        description = "Give a claim back unfinished; its item becomes ready again for the next \
                       pull. Use this rather than leaving a claim to expire."
    )]
    async fn release(
        &self,
        Parameters(args): Parameters<ReleaseArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.worker_call(|s, now| worker_tools::release(s, &args.claim, now))
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
        description = "Start a job this workspace declares in its manifest, by name. Prefer \
                       this over job_start_adhoc: a declaration is reproducible, is reviewed in \
                       version control, and carries its own lifetime, restart policy and port. \
                       The manifest is re-read on every start, so an edit takes effect \
                       immediately."
    )]
    async fn job_start(
        &self,
        Parameters(args): Parameters<StartNamedArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::start_named(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                StartNamedRequest {
                    name: args.name,
                    caller: jobs::session_caller(),
                },
            )
            .await
            .map(|r| format!("started job {}", r.id)),
        )
    }

    #[tool(
        name = "job_restart",
        description = "Stop a declared job and start it again from its *current* declaration. \
                       The manifest is re-read and re-authorized, so an edit takes effect and \
                       a newly-disallowed command is refused. Ad-hoc jobs cannot be restarted \
                       — there is no declaration to re-read."
    )]
    async fn job_restart(
        &self,
        Parameters(args): Parameters<RestartArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::restart(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                RestartRequest {
                    id: args.id,
                    caller: jobs::session_caller(),
                },
            )
            .await
            .map(|r| format!("restarted as job {}", r.id)),
        )
    }

    #[tool(
        name = "job_ensure",
        description = "Start this workspace's session-autostart jobs that are not already \
                       running. Idempotent and safe to re-run: it starts what is missing and \
                       never stops or restarts anything. Run it once at the start of a session."
    )]
    async fn job_ensure(
        &self,
        Parameters(args): Parameters<NodeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::ensure(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                EnsureRequest {
                    caller: jobs::session_caller(),
                },
            )
            .await
            .map(|r| {
                if r.started.is_empty() {
                    "everything declared for this session is already running".to_string()
                } else {
                    format!("started: {}", r.started.join(", "))
                }
            }),
        )
    }

    #[tool(
        name = "job_clear",
        description = "Forget this workspace's finished jobs. Only finished ones — a running \
                       job is left strictly alone, so this can never stop anything. Their \
                       output goes with them, so read anything you still need first."
    )]
    async fn job_clear(
        &self,
        Parameters(args): Parameters<NodeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::clear(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                ClearRequest {
                    caller: jobs::session_caller(),
                },
            )
            .await
            .map(|r| format!("cleared {} finished job(s)", r.removed)),
        )
    }

    #[tool(
        name = "job_start_adhoc",
        description = "Start an arbitrary command as a job, without a declaration. Use \
                       job_start instead when the workspace declares what you want — this is \
                       for one-off work. Ad-hoc jobs cannot be restarted."
    )]
    async fn job_start_adhoc(
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
                    caller: jobs::session_caller(),
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
        name = "job_read",
        description = "Read a job's findings through its machine view — the structured sidecar \
                       it declares, if it declares one. **Prefer this over job_tail**: it is a \
                       few hundred bytes of answer instead of the whole terminal stream. The \
                       reply says which channel it came from, and falls back to scrollback for \
                       a job that declares no machine view."
    )]
    async fn job_read(
        &self,
        Parameters(args): Parameters<TailArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::machine(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                MachineRequest {
                    id: args.id,
                    offset: args.offset.unwrap_or(0),
                },
            )
            .await
            .map(|r| jobs::render_machine(&r)),
        )
    }

    #[tool(
        name = "job_screen",
        description = "What this job's terminal looks like right now — the visible screen, not \
                       its history. Use this for anything that redraws (progress bars, TUIs, \
                       test runners): job_tail would give you every superseded frame, this \
                       gives you the current one."
    )]
    async fn job_screen(
        &self,
        Parameters(args): Parameters<KillArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::screen(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                ScreenRequest { id: args.id },
            )
            .await
            .map(|r| jobs::render_screen(&r)),
        )
    }

    #[tool(
        name = "job_stats",
        description = "What a job is consuming — CPU and memory, now and at peak over the last \
                       minute. Use this to find what is loading a machine. Limits a job declares \
                       are advisory: a breach is reported here and nothing acts on it."
    )]
    async fn job_stats(
        &self,
        Parameters(args): Parameters<KillArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        reply(
            jobs::stats(
                &self.hub,
                target(&args.node, &args.link),
                &connector_id(&args.connector),
                StatsRequest { id: args.id },
            )
            .await
            .map(|r| jobs::render_stats(&r)),
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
                KillRequest {
                    id: args.id,
                    caller: jobs::session_caller(),
                },
            )
            .await
            .map(|r| format!("kill requested for job {}", r.id)),
        )
    }
}

/// The `tools/list` reply. Written out rather than left to the macro, which
/// omits the freshness fields MCP 2026-07-28 requires (see the test below).
/// The list depends on this session's environment, so it is private to it;
/// a zero ttl asks the client to re-list rather than trust a cache.
fn tool_list(tools: Vec<rmcp::model::Tool>) -> rmcp::model::ListToolsResult {
    let mut reply = rmcp::model::ListToolsResult::with_all_items(tools);
    reply.ttl_ms = Some(0);
    reply.cache_scope = Some(rmcp::model::CacheScope::Private);
    reply
}

/// The server's own identity and hints.
///
/// Named for **this** shim, not for the SDK that carries it: the agent sees this
/// name when choosing a tool, and "rmcp" would tell it nothing about what is on
/// the other side.
#[tool_handler]
impl ServerHandler for ShimServer {
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        Ok(tool_list(Self::tool_router().list_all()))
    }

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
    /// The agent CLI negotiates MCP 2026-07-28, which requires `ttlMs` and
    /// `cacheScope` on a `tools/list` reply. Without them it rejects the reply
    /// and the session silently has no kudo tools at all.
    #[test]
    fn the_tool_list_carries_what_2026_07_28_requires() {
        let reply = serde_json::to_value(tool_list(ShimServer::tool_router().list_all())).unwrap();
        assert!(reply["ttlMs"].is_u64(), "{reply}");
        assert!(
            matches!(reply["cacheScope"].as_str(), Some("public" | "private")),
            "{reply}"
        );
        assert!(reply["tools"].as_array().is_some_and(|t| !t.is_empty()));
    }

    /// A worker session sees the worker tools beside the job tools.
    #[test]
    fn the_worker_tools_are_routed() {
        let names: Vec<String> = ShimServer::tool_router()
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        for tool in ["context_read", "pull", "submit", "release", "inbox"] {
            assert!(
                names.iter().any(|n| n == tool),
                "{tool} missing from {names:?}"
            );
        }
    }

    /// The submit tool takes the typed submission, not one free-text log.
    // implements: 4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9@4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9
    #[test]
    fn the_submit_tool_takes_the_typed_submission() {
        let submit = ShimServer::tool_router()
            .list_all()
            .into_iter()
            .find(|t| t.name == "submit")
            .unwrap();
        let schema = serde_json::to_value(&submit.input_schema).unwrap();
        for field in [
            "claim",
            "head",
            "log",
            "established",
            "claimed",
            "ruled_out",
            "gaps",
            "base",
        ] {
            assert!(
                schema["properties"].get(field).is_some(),
                "{field} missing: {schema}"
            );
        }
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(required, ["claim", "head", "log"], "{schema}");
    }

    #[test]
    fn the_macro_generates_a_server_handler() {
        fn assert_handler<T: rmcp::ServerHandler>() {}
        assert_handler::<ShimServer>();
    }
}
