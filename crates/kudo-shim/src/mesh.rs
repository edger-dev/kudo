//! Rendering the mesh for an agent to read.
//!
//! Deliberately compact. A view an agent pays tens of thousands of tokens to
//! read is a view it learns to avoid, and a tool the agent avoids is dead
//! weight — so this is a few lines per node, not a dump.
//!
//! Pure over its input, so it is host-tested without a hub.
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

use std::fmt::Write as _;

use hub_protocol::connector::{Connector, ConnectorKind};
use hub_protocol::discovery::TopologySnapshot;

/// A short label for a connector kind.
///
/// `Other` carries a name this protocol version does not know; it is shown
/// rather than flattened to "other", because the whole point of the open set is
/// that a node may offer something the protocol has not been taught yet.
fn kind_label(kind: &ConnectorKind) -> &str {
    match kind {
        ConnectorKind::ProcessManager => "process-manager",
        ConnectorKind::Sessions => "sessions",
        ConnectorKind::Files => "files",
        ConnectorKind::Commands => "commands",
        ConnectorKind::Other(name) => name,
    }
}

fn render_connector(c: &Connector) -> String {
    format!("{} ({})", c.id, kind_label(&c.kind))
}

/// Render a topology snapshot as a compact node/link/connector listing.
///
/// An empty mesh renders as a **statement**, never as blank output: "nothing is
/// connected" and "the call failed" must not look alike to a reader.
pub fn render_mesh(snapshot: &TopologySnapshot) -> String {
    if snapshot.nodes.is_empty() {
        return "no nodes are connected to this hub".to_string();
    }

    let mut out = String::new();
    for node in &snapshot.nodes {
        let _ = writeln!(out, "{}", node.node.0);
        for link in &node.links {
            let connectors = if link.connectors.is_empty() {
                "(no connectors)".to_string()
            } else {
                link.connectors
                    .iter()
                    .map(render_connector)
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(out, "  {:<8} {}", link.link.0, connectors);
        }
    }
    out
}
