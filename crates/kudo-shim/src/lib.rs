//! kudo-shim — the agent face for the job substrate.
//!
//! A stdio MCP server, launched as a child of the agent session, that proxies
//! capabilities running on any node in the mesh. It is a **face**: a thin
//! per-consumer adapter that holds no logic a second face would need
//! (`kudo:a9a94ad3464ebf50c30bffcfe64d4596384d5d122c5dfb3fb992b1aa73cd7eba`), and
//! no state of its own — every answer comes from an engine, so two readers never
//! see different worlds.
//!
//! v1 goes as far as the far side allows. There is no job connector yet, so the
//! job tools are deliberately absent rather than stubbed: a face that fakes a
//! capability is a face holding engine logic.
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

pub mod error;
pub mod hub;
pub mod jobs;
pub mod mesh;
pub mod route;
pub mod server;

pub use error::ShimError;
pub use hub::HubClient;
pub use mesh::render_mesh;
pub use route::{RouteFailure, Target};
pub use server::ShimServer;

/// The environment variable naming the hub to dial.
pub const HUB_ADDR_ENV: &str = "KUDO_HUB_ADDR";

/// The hub address this shim should use.
///
/// One prefix namespaces everything the platform injects, so the variable can
/// never be confused with something the surrounding tooling already sets.
pub fn hub_addr_from_env() -> Option<String> {
    std::env::var(HUB_ADDR_ENV).ok()
}
