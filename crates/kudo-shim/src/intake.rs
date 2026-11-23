//! Sending and listing remote notes through the intake connector.
//!
//! implements: intake-online-delivery@4dc151b46e7718994552da678ec3e5ea91948e16c8c07abe79392393e851249c

use hub_protocol::routing::RoutedCall;
use kudo_intake::{INTAKE_CONNECTOR, IntakeList, IntakeReply, Note};

use crate::error::ShimError;
use crate::hub::HubClient;
use crate::route::Target;

pub async fn send_note(hub: &HubClient, node: &str, note: &Note) -> Result<IntakeReply, ShimError> {
    let payload = facet_json::to_string(note)
        .map(String::into_bytes)
        .map_err(|error| ShimError::Hub(format!("could not encode the note: {error}")))?;
    route(hub, node, "send_note", payload).await
}

pub async fn list(
    hub: &HubClient,
    node: &str,
    target: &str,
    since: &str,
) -> Result<IntakeReply, ShimError> {
    let request = IntakeList {
        target: target.into(),
        since: since.into(),
    };
    let payload = facet_json::to_string(&request)
        .map(String::into_bytes)
        .map_err(|error| {
            ShimError::Hub(format!("could not encode the listing request: {error}"))
        })?;
    route(hub, node, "list", payload).await
}

async fn route(
    hub: &HubClient,
    node: &str,
    method: &str,
    payload: Vec<u8>,
) -> Result<IntakeReply, ShimError> {
    let reply = hub
        .route(
            Target::node(node),
            RoutedCall {
                connector: INTAKE_CONNECTOR.into(),
                method: method.into(),
                payload,
            },
        )
        .await?;
    let text = String::from_utf8(reply.payload)
        .map_err(|error| ShimError::Hub(format!("intake reply is not UTF-8: {error}")))?;
    facet_json::from_str(&text)
        .map_err(|error| ShimError::Hub(format!("could not decode the intake reply: {error}")))
}
