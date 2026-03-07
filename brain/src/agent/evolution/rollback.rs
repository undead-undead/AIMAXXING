//! Phase 12-A: SOUL.md snapshot and rollback mechanism.
//!
//! Provides backup and restore capabilities for SOUL.md files,
//! enabling automatic rollback when observation windows fail.

use std::path::{Path, PathBuf};

/// A snapshot of a SOUL.md file for potential rollback
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SoulSnapshot {
    /// Role name this snapshot belongs to
    pub role: String,
    /// Original file content
    pub content: String,
    /// When the snapshot was taken
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Path to the original file
    pub original_path: PathBuf,
}

impl SoulSnapshot {
    /// Create a snapshot from a file on disk
    pub async fn create(role: &str, soul_path: &Path) -> anyhow::Result<Self> {
        let content = tokio::fs::read_to_string(soul_path).await?;
        Ok(Self {
            role: role.to_string(),
            content,
            timestamp: chrono::Utc::now(),
            original_path: soul_path.to_path_buf(),
        })
    }

    /// Rollback: restore the snapshot content to the original file
    pub async fn rollback(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.original_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.original_path, &self.content).await?;
        tracing::info!(
            role = %self.role,
            path = ?self.original_path,
            "SOUL.md rolled back to snapshot from {}",
            self.timestamp
        );
        Ok(())
    }
}

/// Guard that automatically rolls back on drop if not explicitly committed.
///
/// Usage:
/// ```ignore
/// let guard = AutoRollbackGuard::new(snapshot);
/// // ... apply changes ...
/// // If everything is fine:
/// guard.commit();
/// // If guard is dropped without commit, it triggers rollback
/// ```
pub struct AutoRollbackGuard {
    snapshot: Option<SoulSnapshot>,
    committed: bool,
}

impl AutoRollbackGuard {
    pub fn new(snapshot: SoulSnapshot) -> Self {
        Self {
            snapshot: Some(snapshot),
            committed: false,
        }
    }

    /// Mark the change as successful, preventing rollback on drop
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for AutoRollbackGuard {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(snapshot) = self.snapshot.take() {
                tracing::warn!(
                    role = %snapshot.role,
                    "AutoRollbackGuard dropped without commit, scheduling rollback"
                );
                // Spawn a blocking task to perform the rollback
                // This is a best-effort rollback since we're in a Drop impl
                let rt = tokio::runtime::Handle::try_current();
                if let Ok(handle) = rt {
                    handle.spawn(async move {
                        if let Err(e) = snapshot.rollback().await {
                            tracing::error!("Auto-rollback failed: {}", e);
                        }
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_snapshot_create_and_rollback() {
        // Create a temporary file with some content
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "original content").unwrap();
        let path = tmp.path().to_path_buf();

        // Create snapshot
        let snapshot = SoulSnapshot::create("test", &path).await.unwrap();
        assert_eq!(snapshot.content, "original content");
        assert_eq!(snapshot.role, "test");

        // Modify the file
        tokio::fs::write(&path, "modified content").await.unwrap();
        let modified = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(modified, "modified content");

        // Rollback
        snapshot.rollback().await.unwrap();
        let restored = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(restored, "original content");
    }
}
