//! The home node's remote-note intake connector.
//!
//! implements: intake-online-delivery@4dc151b46e7718994552da678ec3e5ea91948e16c8c07abe79392393e851249c

use std::collections::BTreeMap;
use std::path::PathBuf;

use hub_protocol::{Connector as ConnectorDescriptor, ConnectorKind, NodeError};
use kudo_intake::{AdmissionRules, IntakeList, IntakeReply, Note, ReceiveOutcome, Receiver};
use node::connector::Connector;

pub use kudo_intake::INTAKE_CONNECTOR;

pub struct IntakeConnector {
    receiver: Receiver,
    rules: AdmissionRules,
}

impl IntakeConnector {
    pub fn new(receiver: Receiver, rules: AdmissionRules) -> Self {
        Self { receiver, rules }
    }

    /// Configure intake from the node's environment; unset targets disable it.
    pub fn from_env() -> Result<Option<Self>, String> {
        let Ok(targets) = std::env::var("KUDO_INTAKE_TARGETS") else {
            return Ok(None);
        };
        let targets = parse_targets(&targets)?;
        let max_body_bytes = match std::env::var("KUDO_INTAKE_MAX_BODY_BYTES") {
            Ok(value) => value
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    format!("KUDO_INTAKE_MAX_BODY_BYTES {value:?} is not a positive integer")
                })?,
            Err(_) => 262_144,
        };
        let sensitive_markers = std::env::var("KUDO_INTAKE_SENSITIVE_MARKERS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|marker| !marker.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Some(Self::new(
            Receiver::new(targets),
            AdmissionRules {
                max_body_bytes,
                sensitive_markers,
            },
        )))
    }
}

impl Connector for IntakeConnector {
    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: INTAKE_CONNECTOR.into(),
            kind: ConnectorKind::Other("intake".into()),
        }
    }

    fn call(&self, method: &str, payload: &[u8]) -> Result<Vec<u8>, NodeError> {
        let reply = match method {
            "send_note" => {
                let note: Note = decode(payload)?;
                let received_at = jiff::Timestamp::now().to_string();
                match self.receiver.receive(&note, &self.rules, &received_at) {
                    Ok(ReceiveOutcome::Delivered { kino_id }) => IntakeReply::Delivered { kino_id },
                    Ok(ReceiveOutcome::Duplicate { kino_id }) => IntakeReply::Duplicate { kino_id },
                    Err(error) => IntakeReply::Refused {
                        reason: error.to_string(),
                    },
                }
            }
            "list" => {
                let request: IntakeList = decode(payload)?;
                match self.receiver.list_since(&request.target, &request.since) {
                    Ok(notes) => IntakeReply::Listed { notes },
                    Err(error) => IntakeReply::Refused {
                        reason: error.to_string(),
                    },
                }
            }
            other => {
                return Err(NodeError::Rejected(format!(
                    "unknown intake method {other:?}"
                )));
            }
        };
        facet_json::to_string(&reply)
            .map(String::into_bytes)
            .map_err(|error| NodeError::Rejected(format!("could not encode intake reply: {error}")))
    }
}

fn parse_targets(text: &str) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut targets = BTreeMap::new();
    for declaration in text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let (alias, path) = declaration
            .split_once('=')
            .ok_or_else(|| format!("intake target {declaration:?} must be alias=/absolute/path"))?;
        if alias.is_empty() || !path.starts_with('/') {
            return Err(format!(
                "intake target {declaration:?} must be alias=/absolute/path"
            ));
        }
        if targets
            .insert(alias.to_owned(), PathBuf::from(path))
            .is_some()
        {
            return Err(format!("duplicate intake target {alias:?}"));
        }
    }
    if targets.is_empty() {
        return Err("KUDO_INTAKE_TARGETS must name at least one alias".into());
    }
    Ok(targets)
}

fn decode<T: for<'a> facet::Facet<'a>>(payload: &[u8]) -> Result<T, NodeError> {
    let text = std::str::from_utf8(payload)
        .map_err(|error| NodeError::Rejected(format!("intake payload is not UTF-8: {error}")))?;
    facet_json::from_str(text)
        .map_err(|error| NodeError::Rejected(format!("could not decode intake payload: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_declarations_are_strict() {
        let targets = parse_targets("kudo=/home/yj/kudo, kinora=/home/yj/kinora").unwrap();
        assert_eq!(targets.len(), 2);
        for invalid in ["", "kudo", "kudo=relative", "= /tmp", "kudo=/a,kudo=/b"] {
            assert!(parse_targets(invalid).is_err(), "accepted {invalid:?}");
        }
    }
}
