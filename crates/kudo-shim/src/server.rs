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

use rmcp::model::{CallToolResult, ContentBlock, ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};

use crate::hub::HubClient;
use crate::mesh::render_mesh;

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
