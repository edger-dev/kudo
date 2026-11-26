//! File-backed direct messages shared by a session shim and its hook processes.
//!
//! implements: 223cbac51b156623cf7320eb634f7ff85ada8df1a4a19872d025903e7a1b74dc@4776113959dd917be1d720b101978004e3eafb22178fa137cf07ac9ac5ce7a60

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

    fn lock(&self) -> std::io::Result<std::fs::File> {
        std::fs::create_dir_all(&self.dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.dir.join(".lock"))?;
        file.lock()?;
        Ok(file)
    }

    fn seen_path(&self, id: &str) -> PathBuf {
        self.dir.join("seen").join(file_name(id))
    }

    fn check_seen(&self, message: &SessionMessage, text: &str) -> std::io::Result<bool> {
        match std::fs::read_to_string(self.seen_path(&message.id)) {
            Ok(content) if content == file_name(text) => Ok(true),
            Ok(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("message id {:?} already has different content", message.id),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Publish a message, treating the same id as an idempotent retry.
    pub fn deliver(&self, message: &SessionMessage) -> std::io::Result<()> {
        let _lock = self.lock()?;
        let target = self.dir.join(format!("{}.json", file_name(&message.id)));
        let text = facet_json::to_string(message)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if self.check_seen(message, &text)? {
            return Ok(());
        }
        let tmp = self.dir.join(format!(
            ".{}.{}.tmp",
            file_name(&message.id),
            crate::new_lease_id()?
        ));
        std::fs::write(&tmp, &text)?;
        match std::fs::hard_link(&tmp, &target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = std::fs::read_to_string(&target)?;
                std::fs::remove_file(&tmp)?;
                if existing == text {
                    return Ok(());
                }
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("message id {:?} already has different content", message.id),
                ));
            }
            Err(error) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(error);
            }
        }
        if self.check_seen(message, &text)? {
            std::fs::remove_file(target)?;
            std::fs::remove_file(tmp)?;
            return Ok(());
        }
        std::fs::remove_file(tmp)
    }

    /// Whether any message is pending.
    pub fn has_messages(&self) -> std::io::Result<bool> {
        let _lock = self.lock()?;
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

    /// Claim and drain pending messages across shim and hook processes.
    pub fn take(&self) -> std::io::Result<Vec<SessionMessage>> {
        let _lock = self.lock()?;
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
            let message: SessionMessage = facet_json::from_str(&text)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            let seen = self.seen_path(&message.id);
            if self.check_seen(&message, &text)? {
                std::fs::remove_file(path)?;
                continue;
            }
            std::fs::create_dir_all(seen.parent().unwrap_or(&self.dir))?;
            let seen_tmp = seen.with_extension(format!("{}.tmp", crate::new_lease_id()?));
            std::fs::write(&seen_tmp, file_name(&text))?;
            std::fs::rename(seen_tmp, seen)?;
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
