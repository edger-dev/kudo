//! The shim's acknowledged local session mailbox.

use std::sync::{Arc, Mutex};

use hub_protocol::{HubError, SessionMessage, SessionService};
use kudo_worker::{SessionMailbox, WorkerSetup};

#[derive(Clone)]
enum Storage {
    Memory(Arc<Mutex<Vec<SessionMessage>>>),
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
                &mut *messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            )),
            Storage::Files(mailbox) => mailbox.take().map_err(|error| error.to_string()),
        }
    }
}

// implements: 223cbac51b156623cf7320eb634f7ff85ada8df1a4a19872d025903e7a1b74dc@223cbac51b156623cf7320eb634f7ff85ada8df1a4a19872d025903e7a1b74dc
impl SessionService for SessionInbox {
    async fn deliver(&self, message: SessionMessage) -> Result<(), HubError> {
        match &self.storage {
            Storage::Memory(messages) => messages
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(message),
            Storage::Files(mailbox) => mailbox
                .deliver(&message)
                .map_err(|error| HubError::Rejected(error.to_string()))?,
        }
        Ok(())
    }
}
