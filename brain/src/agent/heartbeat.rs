use crate::agent::multi_agent::MultiAgent;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

/// Monitors a HEARTBEAT.md file for pending tasks and executes them
pub struct HeartbeatWatcher {
    agent: Arc<dyn MultiAgent>,
    path: PathBuf,
    interval: Duration,
}

impl HeartbeatWatcher {
    pub fn new(agent: Arc<dyn MultiAgent>, path: PathBuf, interval_secs: u64) -> Self {
        Self {
            agent,
            path,
            interval: Duration::from_secs(interval_secs),
        }
    }

    /// Start the watcher loop
    pub async fn run(&self) {
        info!("Starting HeartbeatWatcher for {:?}", self.path);
        let mut interval = interval(self.interval);

        loop {
            interval.tick().await;

            if !self.path.exists() {
                continue;
            }

            match self.process_tasks().await {
                Ok(count) if count > 0 => {
                    info!("Completed {} tasks from heartbeat", count);
                }
                Err(e) => {
                    error!("Heartbeat processing error: {}", e);
                }
                _ => {}
            }
        }
    }

    async fn process_tasks(&self) -> anyhow::Result<usize> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let mut tasks = Vec::new();
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut changed = false;

        // Simple markdown task extraction: - [ ] Task description
        for (i, line) in lines.iter_mut().enumerate() {
            if line.trim().starts_with("- [ ]") {
                let task = line.trim_start_matches("- [ ]").trim().to_string();
                if !task.is_empty() {
                    tasks.push((i, task));
                }
            }
        }

        if tasks.is_empty() {
            return Ok(0);
        }

        for (idx, task) in tasks {
            info!("Executing heartbeat task: {}", task);

            match self.agent.process(&task).await {
                Ok(_response) => {
                    info!("Heartbeat task completed successfully.");
                    // Mark as done in the file [x]
                    lines[idx] = lines[idx].replace("- [ ]", "- [x]");
                    changed = true;
                }
                Err(e) => {
                    warn!("Heartbeat task failed: {}", e);
                    // Optionally mark as failed [-] but here we'll just leave it for retry
                }
            }
        }

        if changed {
            let new_content = lines.join("\n");
            tokio::fs::write(&self.path, new_content).await?;
        }

        Ok(changed as usize)
    }
}
