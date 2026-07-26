//! Phase 1 tests — the shim is a real hub consumer.
//!
//! Two properties: what the agent sees of the mesh is cheap to read, and a hub
//! that is not there says so in a way you can act on. The second matters because
//! a node has no local path by contract — if the hub is down there is nothing
//! else to try, and an empty list would look exactly like a healthy empty mesh.
//!
//! implements: 121ac6ebe48b717b93e775f5a0526076a9230ec0e10e748dbcbaf181bf758120

use hub_protocol::connector::{Connector, ConnectorKind};
use hub_protocol::discovery::{LinkInfo, NodeInfo, TopologySnapshot};
use hub_protocol::identity::{LinkLabel, NodeId};

use kudo_shim::hub::HubClient;
use kudo_shim::mesh::render_mesh;

fn snapshot() -> TopologySnapshot {
    TopologySnapshot {
        nodes: vec![
            NodeInfo {
                node: NodeId("alpha".to_string()),
                links: vec![
                    LinkInfo {
                        link: LinkLabel::stable(),
                        connectors: vec![
                            Connector {
                                id: "pm-0".to_string(),
                                kind: ConnectorKind::ProcessManager,
                            },
                            Connector {
                                id: "files-0".to_string(),
                                kind: ConnectorKind::Files,
                            },
                        ],
                    },
                    LinkInfo {
                        link: LinkLabel::dev(),
                        connectors: vec![Connector {
                            id: "pm-0".to_string(),
                            kind: ConnectorKind::ProcessManager,
                        }],
                    },
                ],
            },
            NodeInfo {
                node: NodeId("beta".to_string()),
                links: vec![LinkInfo {
                    link: LinkLabel::stable(),
                    connectors: vec![Connector {
                        id: "custom-0".to_string(),
                        kind: ConnectorKind::Other("weather".to_string()),
                    }],
                }],
            },
        ],
    }
}

/// Every node, every link, and what each link can actually do.
#[test]
fn the_mesh_view_names_nodes_links_and_connectors() {
    let view = render_mesh(&snapshot());

    assert!(view.contains("alpha"), "got:\n{view}");
    assert!(view.contains("beta"), "got:\n{view}");
    assert!(view.contains("stable"), "got:\n{view}");
    assert!(view.contains("dev"), "got:\n{view}");
    assert!(view.contains("pm-0"), "got:\n{view}");
    assert!(view.contains("files-0"), "got:\n{view}");
}

/// A connector kind this protocol version does not name still renders — the
/// wire keeps the set open, so the view must not swallow what it cannot label.
#[test]
fn an_unnamed_connector_kind_still_renders_its_label() {
    let view = render_mesh(&snapshot());
    assert!(
        view.contains("weather"),
        "an Other(..) kind must show its label, got:\n{view}"
    );
}

/// An empty mesh says so in words. It must never be mistaken for a failure, and
/// a failure must never be mistaken for it.
#[test]
fn an_empty_mesh_is_stated_not_blank() {
    let view = render_mesh(&TopologySnapshot { nodes: vec![] });
    assert!(
        !view.trim().is_empty(),
        "an empty mesh must still say something"
    );
    assert!(view.to_lowercase().contains("no nodes"), "got:\n{view}");
}

/// The hub being unreachable is an actionable error that **names the address**,
/// not an empty result. There is no node-local fallback by contract, so this is
/// the whole story when it happens.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_hub_names_the_address_it_tried() {
    // Port 1 on loopback: reserved, and nothing of ours ever listens there.
    let addr = "127.0.0.1:1";
    let err = HubClient::dial(addr)
        .await
        .expect_err("dialing a closed port must fail");

    let message = err.to_string();
    assert!(
        message.contains(addr),
        "the error must name the address it tried, got: {message}"
    );
}
