//! File-backed direct messages shared by a session shim and its hook processes.
//!
//! implements: 223cbac51b156623cf7320eb634f7ff85ada8df1a4a19872d025903e7a1b74dc@223cbac51b156623cf7320eb634f7ff85ada8df1a4a19872d025903e7a1b74dc

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use hub_protocol::{SessionInfo, SessionMessage};

/// One session's local mailbox, stored as one atomically published file per id.
#[derive(Clone)]
pub struct SessionMailbox {
    dir: PathBuf,
}

impl SessionMailbox {
    pub fn new(parent: &Path, session: &str) -> Self {
        Self {
            dir: parent.join("session-messages").join(file_name(session)),
        }
    }

    /// Publish a message, treating the same id as an idempotent retry.
    pub fn deliver(&self, message: &SessionMessage) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let target = self.dir.join(format!("{}.json", file_name(&message.id)));
        let text = facet_json::to_string(message)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let tmp = self.dir.join(format!(
            ".{}.{}.tmp",
            file_name(&message.id),
            crate::new_lease_id()?
        ));
        std::fs::write(&tmp, text)?;
        match std::fs::hard_link(&tmp, &target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                std::fs::remove_file(&tmp)?;
                return Ok(());
            }
            Err(error) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(error);
            }
        }
        std::fs::remove_file(tmp)
    }

    /// Whether any message is pending.
    pub fn has_messages(&self) -> std::io::Result<bool> {
        match std::fs::read_dir(&self.dir) {
            Ok(entries) => {
                for entry in entries {
                    if entry?.path().extension().is_some_and(|ext| ext == "json") {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Drain pending messages, oldest first.
    pub fn take(&self) -> std::io::Result<Vec<SessionMessage>> {
        let mut paths = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        paths.retain(|path| path.extension().is_some_and(|ext| ext == "json"));
        paths.sort_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        });
        let mut messages = Vec::new();
        for path in paths {
            let text = std::fs::read_to_string(&path)?;
            let message = facet_json::from_str(&text)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            std::fs::remove_file(path)?;
            messages.push(message);
        }
        Ok(messages)
    }
}

/// Render pending messages with authenticated sender and reply addresses.
pub fn render_messages(messages: &[SessionMessage]) -> String {
    messages
        .iter()
        .map(|message| {
            format!(
                "Message {} from {} on node {}:\n\n{}\n\nReply with `session_send` to {} if useful.",
                message.id, message.from.id, message.from.node, message.body, message.from.id
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// Build this shim's stable public session identity.
pub fn session_info(
    lease: Option<&str>,
    node: String,
    harness: String,
    workspace: String,
    context: Option<String>,
) -> std::io::Result<SessionInfo> {
    let id = match lease {
        Some(lease) => lease.to_string(),
        None => crate::new_lease_id()?.to_string(),
    };
    Ok(SessionInfo {
        id,
        node,
        harness,
        workspace,
        context,
    })
}

fn file_name(id: &str) -> String {
    let mut hash = DefaultHasher::new();
    id.hash(&mut hash);
    format!("{:016x}", hash.finish())
}
