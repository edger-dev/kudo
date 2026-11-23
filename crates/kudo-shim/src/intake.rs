//! Sending and listing remote notes through the intake connector.

use kudo_intake::{IntakeReply, Note};

use crate::error::ShimError;
use crate::hub::HubClient;

pub async fn send_note(
    _hub: &HubClient,
    _node: &str,
    _note: &Note,
) -> Result<IntakeReply, ShimError> {
    Err(ShimError::Hub("intake is not implemented".into()))
}

pub async fn list(
    _hub: &HubClient,
    _node: &str,
    _target: &str,
    _since: &str,
) -> Result<IntakeReply, ShimError> {
    Err(ShimError::Hub("intake is not implemented".into()))
}
