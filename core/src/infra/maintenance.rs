//! Background maintenance tasks for resource cleanup

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::info;

use crate::agent::memory::ShortTermMemory;

/// Configuration for background tasks
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    /// Interval for memory cleanup (in seconds)
    pub memory_cleanup_interval_secs: u64,
    /// Inactive timeout for short-term memory (in seconds)
    pub memory_inactive_timeout_secs: u64,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            memory_cleanup_interval_secs: 300, // 5 minutes
            memory_inactive_timeout_secs: 3600, // 1 hour
        }
    }
}

/// Manager for background maintenance tasks
pub struct MaintenanceManager {
    tasks: Vec<JoinHandle<()>>,
}

impl MaintenanceManager {
    /// Create a new maintenance manager
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
        }
    }

    /// Start memory cleanup task
    pub fn start_memory_cleanup(
        &mut self,
        memory: Arc<ShortTermMemory>,
        config: MaintenanceConfig,
    ) {
        let handle = tokio::spawn(async move {
            let interval = Duration::from_secs(config.memory_cleanup_interval_secs);
            let inactive_timeout = Duration::from_secs(config.memory_inactive_timeout_secs);
            
            loop {
                tokio::time::sleep(interval).await;
                info!("Running scheduled short-term memory cleanup");
                memory.prune_inactive(inactive_timeout);
            }
        });
        self.tasks.push(handle);
    }


    /// Shutdown all background tasks
    pub async fn shutdown(self) {
        info!("Shutting down {} background maintenance tasks", self.tasks.len());
        
        for task in self.tasks {
            task.abort();
        }
        
        info!("All maintenance tasks stopped");
    }
}

impl Default for MaintenanceManager {
    fn default() -> Self {
        Self::new()
    }
}
