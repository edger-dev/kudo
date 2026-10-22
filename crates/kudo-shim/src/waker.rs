//! The waker.
use kudo_context::{ContextId, ContextStore, Error, Now};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wake {
    pub context: ContextId,
    pub node: String,
}

pub fn due_contexts(
    _store: &ContextStore,
    _now: Now,
    _own_node: &str,
    _quiet_after: i64,
) -> Result<Vec<Wake>, Error> {
    Ok(Vec::new())
}
