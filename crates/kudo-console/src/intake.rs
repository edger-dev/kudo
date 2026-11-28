//! Intake-note presentation and implementation requests.

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImplementationRequest {
    pub title: String,
    pub head: String,
    pub ask: String,
    pub node: String,
}

pub fn implementation_request(
    _note: &kudo_intake::ReceivedNote,
    _node: &str,
) -> ImplementationRequest {
    ImplementationRequest::default()
}
