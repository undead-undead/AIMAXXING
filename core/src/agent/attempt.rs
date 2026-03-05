use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Context construction strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Strategy {
    /// Standard mode: Use default configuration (max_history_messages)
    #[default]
    Standard,
    /// Compressed mode: Halve max_history_messages, enable smart pruning, add concise directive
    Compressed,
    /// Fallback mode: Minimal context (System Prompt + Last User Message only)
    Fallback,
}

/// Represents a single attempt to generate a response
#[derive(Debug, Clone)]
pub struct Attempt {
    /// Unique ID for this attempt chain
    pub id: Uuid,
    /// Current strategy being used
    pub strategy: Strategy,
    /// Current retry count (0-indexed)
    pub retry_count: u32,
    /// Maximum allowed retries for network/server errors
    pub max_retries: u32,
}

impl Attempt {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            strategy: Strategy::Standard,
            retry_count: 0,
            max_retries: 3,
        }
    }

    /// Check if we can retry based on current count
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment retry count
    pub fn next(&mut self) {
        self.retry_count += 1;
    }

    /// Downgrade strategy for context overflow recovery
    pub fn downgrade(&mut self) -> bool {
        match self.strategy {
            Strategy::Standard => {
                self.strategy = Strategy::Compressed;
                true
            }
            Strategy::Compressed => {
                self.strategy = Strategy::Fallback;
                true
            }
            Strategy::Fallback => false, // Cannot downgrade further
        }
    }
}

impl Default for Attempt {
    fn default() -> Self {
        Self::new()
    }
}
