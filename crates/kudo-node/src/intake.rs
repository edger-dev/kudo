//! The home node's remote-note intake connector.
//!
//! implements: intake-online-delivery@4dc151b46e7718994552da678ec3e5ea91948e16c8c07abe79392393e851249c

use hub_protocol::{Connector as ConnectorDescriptor, ConnectorKind, NodeError};
use kudo_intake::{AdmissionRules, Receiver};
use node::connector::Connector;

pub const INTAKE_CONNECTOR: &str = "kudo-intake";

pub struct IntakeConnector {
    _receiver: Receiver,
    _rules: AdmissionRules,
}

impl IntakeConnector {
    pub fn new(receiver: Receiver, rules: AdmissionRules) -> Self {
        Self {
            _receiver: receiver,
            _rules: rules,
        }
    }
}

impl Connector for IntakeConnector {
    fn descriptor(&self) -> ConnectorDescriptor {
        ConnectorDescriptor {
            id: INTAKE_CONNECTOR.into(),
            kind: ConnectorKind::Other("intake".into()),
        }
    }

    fn call(&self, method: &str, _payload: &[u8]) -> Result<Vec<u8>, NodeError> {
        Err(NodeError::Rejected(format!(
            "intake method {method:?} is not implemented"
        )))
    }
}
