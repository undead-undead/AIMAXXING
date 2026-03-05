//! File watcher for active indexing
//!
//! Watches filesystem for changes and triggers re-indexing.

use crate::error::{EngramError, Result};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tracing::{info, warn};

/// File watcher that monitors directories for changes
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<Event>>,
    watched_paths: Vec<PathBuf>,
}

impl FileWatcher {
    /// Create a new file watcher
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )
        .map_err(|e| EngramError::Custom(format!("Failed to create watcher: {}", e)))?;

        Ok(Self {
            watcher,
            rx,
            watched_paths: Vec::new(),
        })
    }

    /// Watch a directory for changes
    pub fn watch(&mut self, path: &Path) -> Result<()> {
        self.watcher
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| {
                EngramError::Custom(format!("Failed to watch {}: {}", path.display(), e))
            })?;

        self.watched_paths.push(path.to_path_buf());
        info!("Watching: {}", path.display());
        Ok(())
    }

    /// Stop watching a directory
    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.watcher.unwatch(path).map_err(|e| {
            EngramError::Custom(format!("Failed to unwatch {}: {}", path.display(), e))
        })?;

        self.watched_paths.retain(|p| p != path);
        Ok(())
    }

    /// Get pending events (non-blocking)
    pub fn poll_events(&self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Ok(event) => events.push(event),
                Err(e) => warn!("Watch error: {}", e),
            }
        }
        events
    }

    /// Get watched paths
    pub fn watched_paths(&self) -> &[PathBuf] {
        &self.watched_paths
    }
}
