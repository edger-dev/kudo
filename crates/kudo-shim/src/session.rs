//! The shim's acknowledged local session mailbox.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use hub_protocol::{HubError, SessionMessage, SessionService};
use kudo_worker::{SessionMailbox, WorkerSetup};

#[derive(Default)]
struct Memory {
    pending: Vec<SessionMessage>,
    seen: HashMap<String, u64>,
}

#[derive(Clone)]
enum Storage {
    Memory(Arc<Mutex<Memory>>),
    Files(SessionMailbox),
}

/// The receiving end of `SessionService` for one shim.
#[derive(Clone)]
pub struct SessionInbox {
    storage: Storage,
}

impl SessionInbox {
    pub fn new(worker: Option<&WorkerSetup>) -> Self {
        match worker {
            Some(setup) => Self {
                storage: Storage::Files(SessionMailbox::new(&setup.leases_dir(), &setup.lease)),
            },
            None => Self {
                storage: Storage::Memory(Arc::default()),
            },
        }
    }

    pub fn take(&self) -> Result<Vec<SessionMessage>, String> {
        match &self.storage {
            Storage::Memory(messages) => Ok(std::mem::take(
                &mut messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pending,
            )),
            Storage::Files(mailbox) => mailbox.take().map_err(|error| error.to_string()),
        }
    }
}

// implements: 223cbac51b156623cf7320eb634f7ff85ada8df1a4a19872d025903e7a1b74dc@4776113959dd917be1d720b101978004e3eafb22178fa137cf07ac9ac5ce7a60
impl SessionService for SessionInbox {
    async fn deliver(&self, message: SessionMessage) -> Result<(), HubError> {
        match &self.storage {
            Storage::Memory(messages) => {
                let text = facet_json::to_string(&message)
                    .map_err(|error| HubError::Rejected(error.to_string()))?;
                let mut hasher = DefaultHasher::new();
                text.hash(&mut hasher);
                let fingerprint = hasher.finish();
                let mut messages = messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match messages.seen.get(&message.id) {
                    Some(existing) if *existing == fingerprint => return Ok(()),
                    Some(_) => {
                        return Err(HubError::Rejected(format!(
                            "message id {:?} already has different content",
                            message.id
                        )));
                    }
                    None => {
                        messages.seen.insert(message.id.clone(), fingerprint);
                        messages.pending.push(message);
                    }
                }
            }
            Storage::Files(mailbox) => mailbox
                .deliver(&message)
                .map_err(|error| HubError::Rejected(error.to_string()))?,
        }
        Ok(())
    }
}
