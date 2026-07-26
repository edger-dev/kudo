//! Addressing a node, and keeping failures honest about where they happened.
//!
//! A node is named by its **registry name**, never by an address: there is no
//! node-local listener to reach
//! (`hub:bf406db5b660bd595cccdfcec5921121bb5a1ac9281a0d9a5c28be2d7ccc79b4`). The
//! link is optional, because reads are link-agnostic and the hub defaults to —
//! and falls back to — `stable`
//! (`hub:08c3ce357742ccf578caddcfb6578a92216b515adcfe1ac9de4543406be52cb0`). So a
//! solo, single-link setup names a node and nothing else.
//!
//! implements: 7c16a4ca840ff15005c5e90cca00d3d34e639ee346aadb8807863e860366118b

use hub_protocol::identity::{LinkLabel, NodeId};
use hub_protocol::targeting::LinkSelector;

/// Where a call should go: a node, and optionally which of its links.
///
/// This is the shim's ergonomic front for `LinkSelector` — one place that turns
/// the agent's strings into the protocol's types, so no tool builds a selector
/// by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub node: String,
    pub link: Option<String>,
}

impl Target {
    /// A node's default (`stable`) link.
    pub fn node(node: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            link: None,
        }
    }

    /// A specific link of a node. If it is not connected the hub falls back to
    /// `stable`, so this is a preference rather than a demand.
    pub fn link(node: impl Into<String>, link: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            link: Some(link.into()),
        }
    }

    /// Build the protocol's selector.
    pub fn selector(&self) -> LinkSelector {
        LinkSelector {
            node: NodeId(self.node.clone()),
            link: self.link.clone().map(LinkLabel),
        }
    }
}

/// Which side of the mesh a routed call failed on.
///
/// The distinction is the whole point: the two want different reactions. A hub
/// failure usually means this consumer's view of the mesh is stale — refresh and
/// retry. A node failure means the call arrived and the connector said no —
/// surface it, and do not blindly retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteFailure {
    /// The hub could not deliver: no such node, or no link to route to.
    Hub,
    /// It reached the node, and the connector refused.
    Node,
}
