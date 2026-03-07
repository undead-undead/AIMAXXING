//! Session Storage and Branching
//!
//! Manages conversation sessions with:
//! - Message history persistence to JSON files
//! - Conversation branching (fork from any point)
//! - Session metadata (creation time, last active, message count)
//! - Auto-save on message append
//! - Session discovery and cleanup

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::agent::message::Message;

/// Session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// Currently active.
    Active,
    /// Paused (user switched to different session).
    Paused,
    /// Archived (no longer accessible without restore).
    Archived,
}

/// Session configuration.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Base directory for session storage.
    pub storage_dir: PathBuf,
    /// Maximum messages before auto-archiving (0 = unlimited).
    pub max_messages: usize,
    /// Whether to auto-save on each message.
    pub auto_save: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            storage_dir: PathBuf::from(".aimaxxing/sessions"),
            max_messages: 0,
            auto_save: true,
        }
    }
}

/// Session metadata (persisted alongside messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Unique session ID.
    pub id: String,
    /// Human-readable title (auto-generated or user-set).
    pub title: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// Last activity time.
    pub last_active: DateTime<Utc>,
    /// Number of messages.
    pub message_count: usize,
    /// Parent session ID (for branches).
    pub parent_id: Option<String>,
    /// Branch point (message index in parent where this was forked).
    pub branch_point: Option<usize>,
    /// Current state.
    pub state: SessionState,
    /// Arbitrary metadata tags.
    pub tags: HashMap<String, String>,
}

/// A conversation session.
pub struct Session {
    meta: SessionMeta,
    messages: Vec<Message>,
    config: SessionConfig,
    dirty: bool,
}

impl Session {
    /// Create a new session.
    pub fn new(title: impl Into<String>, config: SessionConfig) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        Self {
            meta: SessionMeta {
                id,
                title: title.into(),
                created_at: now,
                last_active: now,
                message_count: 0,
                parent_id: None,
                branch_point: None,
                state: SessionState::Active,
                tags: HashMap::new(),
            },
            messages: Vec::new(),
            config,
            dirty: true,
        }
    }

    /// Get the session ID.
    pub fn id(&self) -> &str {
        &self.meta.id
    }

    /// Get the session title.
    pub fn title(&self) -> &str {
        &self.meta.title
    }

    /// Set the session title.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.meta.title = title.into();
        self.dirty = true;
    }

    /// Get the session state.
    pub fn state(&self) -> SessionState {
        self.meta.state
    }

    /// Get all messages.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the message count.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get the metadata.
    pub fn meta(&self) -> &SessionMeta {
        &self.meta
    }

    /// Append a message to the session.
    pub fn push_message(&mut self, message: Message) {
        self.messages.push(message);
        self.meta.message_count = self.messages.len();
        self.meta.last_active = Utc::now();
        self.dirty = true;

        if self.config.auto_save {
            if let Err(e) = self.save() {
                warn!(session = %self.meta.id, error = %e, "Auto-save failed");
            }
        }
    }

    /// Fork this session at the given message index (creates a branch).
    pub fn fork(&self, at_message: usize, branch_title: impl Into<String>) -> anyhow::Result<Self> {
        if at_message > self.messages.len() {
            anyhow::bail!(
                "Cannot fork at message {} (session has {} messages)",
                at_message,
                self.messages.len()
            );
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        Ok(Self {
            meta: SessionMeta {
                id,
                title: branch_title.into(),
                created_at: now,
                last_active: now,
                message_count: at_message,
                parent_id: Some(self.meta.id.clone()),
                branch_point: Some(at_message),
                state: SessionState::Active,
                tags: HashMap::new(),
            },
            messages: self.messages[..at_message].to_vec(),
            config: self.config.clone(),
            dirty: true,
        })
    }

    /// Rewind to a specific message index (truncates history).
    pub fn rewind(&mut self, to_message: usize) -> anyhow::Result<()> {
        if to_message > self.messages.len() {
            anyhow::bail!(
                "Cannot rewind to message {} (session has {} messages)",
                to_message,
                self.messages.len()
            );
        }

        self.messages.truncate(to_message);
        self.meta.message_count = self.messages.len();
        self.meta.last_active = Utc::now();
        self.dirty = true;
        Ok(())
    }

    /// Archive this session.
    pub fn archive(&mut self) {
        self.meta.state = SessionState::Archived;
        self.dirty = true;
    }

    /// Save session to disk.
    pub fn save(&mut self) -> anyhow::Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let dir = self.config.storage_dir.join(&self.meta.id);
        std::fs::create_dir_all(&dir)?;

        // Save metadata
        let meta_path = dir.join("meta.json");
        let meta_json = serde_json::to_string_pretty(&self.meta)?;
        std::fs::write(&meta_path, meta_json)?;

        // Save messages (using JSONL for append-friendliness)
        let messages_path = dir.join("messages.jsonl");
        let mut content = String::new();
        for msg in &self.messages {
            content.push_str(&serde_json::to_string(msg)?);
            content.push('\n');
        }
        std::fs::write(&messages_path, content)?;

        self.dirty = false;
        debug!(session = %self.meta.id, "Session saved");
        Ok(())
    }

    /// Load a session from disk.
    pub fn load(session_dir: &Path, config: SessionConfig) -> anyhow::Result<Self> {
        let meta_path = session_dir.join("meta.json");
        let meta_json = std::fs::read_to_string(&meta_path)?;
        let meta: SessionMeta = serde_json::from_str(&meta_json)?;

        let messages_path = session_dir.join("messages.jsonl");
        let mut messages = Vec::new();
        if messages_path.exists() {
            let content = std::fs::read_to_string(&messages_path)?;
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Message>(line) {
                    Ok(msg) => messages.push(msg),
                    Err(e) => {
                        warn!(error = %e, "Skipping corrupted message line");
                    }
                }
            }
        }

        Ok(Self {
            meta,
            messages,
            config,
            dirty: false,
        })
    }
}

/// Manages multiple sessions.
pub struct SessionManager {
    config: SessionConfig,
    /// Currently active session ID.
    active_session: Option<String>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new(config: SessionConfig) -> Self {
        std::fs::create_dir_all(&config.storage_dir).ok();
        Self {
            config,
            active_session: None,
        }
    }

    /// Create and activate a new session.
    pub fn create_session(&mut self, title: impl Into<String>) -> Session {
        let session = Session::new(title, self.config.clone());
        self.active_session = Some(session.id().to_string());
        info!(session = %session.id(), title = %session.title(), "Created new session");
        session
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> anyhow::Result<Vec<SessionMeta>> {
        let mut sessions = Vec::new();
        let dir = &self.config.storage_dir;

        if !dir.exists() {
            return Ok(sessions);
        }

        for entry in std::fs::read_dir(dir)?.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let meta_path = entry.path().join("meta.json");
            if meta_path.exists() {
                match std::fs::read_to_string(&meta_path).and_then(|s| {
                    serde_json::from_str::<SessionMeta>(&s)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                }) {
                    Ok(meta) => sessions.push(meta),
                    Err(e) => {
                        warn!(path = ?meta_path, error = %e, "Failed to read session metadata");
                    }
                }
            }
        }

        // Sort by last_active descending
        sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));
        Ok(sessions)
    }

    /// Load a specific session.
    pub fn load_session(&mut self, session_id: &str) -> anyhow::Result<Session> {
        let dir = self.config.storage_dir.join(session_id);
        if !dir.exists() {
            anyhow::bail!("Session '{}' not found", session_id);
        }
        let session = Session::load(&dir, self.config.clone())?;
        self.active_session = Some(session_id.to_string());
        Ok(session)
    }

    /// Delete a session.
    pub fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        let dir = self.config.storage_dir.join(session_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
            info!(session = session_id, "Session deleted");
        }
        Ok(())
    }

    /// Cleanup archived sessions older than the given duration.
    pub fn cleanup_old_sessions(&self, max_age: chrono::Duration) -> anyhow::Result<usize> {
        let cutoff = Utc::now() - max_age;
        let sessions = self.list_sessions()?;
        let mut cleaned = 0;

        for meta in sessions {
            if meta.state == SessionState::Archived && meta.last_active < cutoff {
                self.delete_session(&meta.id)?;
                cleaned += 1;
            }
        }

        if cleaned > 0 {
            info!(count = cleaned, "Cleaned up old sessions");
        }
        Ok(cleaned)
    }

    /// Get the currently active session ID.
    pub fn active_session_id(&self) -> Option<&str> {
        self.active_session.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(dir: &Path) -> SessionConfig {
        SessionConfig {
            storage_dir: dir.to_path_buf(),
            auto_save: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_session_create_and_push() {
        let tmp = TempDir::new().unwrap();
        let mut session = Session::new("Test Session", test_config(tmp.path()));

        assert_eq!(session.message_count(), 0);
        session.push_message(Message::user("Hello"));
        assert_eq!(session.message_count(), 1);
        session.push_message(Message::assistant("Hi!"));
        assert_eq!(session.message_count(), 2);
    }

    #[test]
    fn test_session_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());

        let mut session = Session::new("Persist Test", config.clone());
        session.push_message(Message::user("Hello"));
        session.push_message(Message::assistant("World"));
        session.save().unwrap();

        let session_dir = tmp.path().join(session.id());
        let loaded = Session::load(&session_dir, config).unwrap();
        assert_eq!(loaded.title(), "Persist Test");
        assert_eq!(loaded.message_count(), 2);
    }

    #[test]
    fn test_session_fork() {
        let tmp = TempDir::new().unwrap();
        let mut session = Session::new("Main", test_config(tmp.path()));
        session.push_message(Message::user("Message 1"));
        session.push_message(Message::assistant("Response 1"));
        session.push_message(Message::user("Message 2"));

        let branch = session.fork(2, "Branch").unwrap();
        assert_eq!(branch.message_count(), 2);
        assert_eq!(branch.meta().parent_id, Some(session.id().to_string()));
        assert_eq!(branch.meta().branch_point, Some(2));
    }

    #[test]
    fn test_session_rewind() {
        let tmp = TempDir::new().unwrap();
        let mut session = Session::new("Rewind", test_config(tmp.path()));
        session.push_message(Message::user("1"));
        session.push_message(Message::assistant("2"));
        session.push_message(Message::user("3"));

        session.rewind(1).unwrap();
        assert_eq!(session.message_count(), 1);
    }

    #[test]
    fn test_session_manager() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(tmp.path());
        let mut manager = SessionManager::new(config);

        let mut s = manager.create_session("Test");
        s.push_message(Message::user("Hello"));
        s.save().unwrap();

        let sessions = manager.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "Test");
    }
}
