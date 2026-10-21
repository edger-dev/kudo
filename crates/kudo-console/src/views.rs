//! The console server's reads: plain functions over the context store.

use kudo_context::{ContextStore, Error, Now};

use crate::model::{ContextCard, ContextDetail};

/// Every context, with its counts and live workers (stub).
pub fn contexts(store: &ContextStore, now: Now) -> Result<Vec<ContextCard>, Error> {
    let _ = (store, now);
    Ok(Vec::new())
}

/// One context's head and trace (stub).
pub fn detail(store: &ContextStore, id: &str) -> Result<ContextDetail, Error> {
    let _ = (store, id);
    Ok(ContextDetail::default())
}
