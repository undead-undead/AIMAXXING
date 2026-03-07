use crate::agent::message::Message;
use serde::{Deserialize, Serialize};

/// Status of an agent session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Agent is thinking or waiting for provider
    Thinking,
    /// Agent has generated tool calls and is waiting for execution/approval
    PendingTools,
    /// Agent is waiting for manual user approval for a specific tool
    AwaitingApproval {
        tool_name: String,
        arguments: String,
    },
    /// Agent is executing tools
    Executing,
    /// Agent has completed the task
    Completed,
    /// Agent has failed
    Failed(String),
}

/// A persistent session representing an agent's current state and history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    /// Unique identifier for the session
    pub id: String,
    /// Dialogue history
    pub messages: Vec<Message>,
    /// Current step in the reasoning loop
    pub step: usize,
    /// Current status of the agent
    pub status: SessionStatus,
    /// Timestamp of the last update
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl AgentSession {
    /// Create a new session
    pub fn new(id: String) -> Self {
        Self {
            id,
            messages: Vec::new(),
            step: 0,
            status: SessionStatus::Thinking,
            updated_at: chrono::Utc::now(),
        }
    }
}
