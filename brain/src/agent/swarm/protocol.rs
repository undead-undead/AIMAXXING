use crate::agent::swarm::manifest::AgentManifest;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages exchanged between agents in the swarm
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SwarmMessage {
    /// Announcement of an agent's presence
    Announcement(AgentManifest),

    /// Request for a task to be performed
    TaskRequest {
        request_id: String,
        requester_id: String,
        task: String,
        required_capabilities: Vec<String>,
    },

    /// Bid from an agent willing to perform the task
    Bid {
        request_id: String,
        bidder_id: String,
        bid_amount: f64,
    },

    /// Assignment of a task to a specific bidder
    TaskAssignment {
        request_id: String,
        assigned_to: String,
        task_context: String,
    },

    /// Result of a performed task
    Result {
        request_id: String,
        performer_id: String,
        output: String,
        success: bool,
    },
}

impl SwarmMessage {
    pub fn new_request(
        requester_id: impl Into<String>,
        task: impl Into<String>,
        required_capabilities: Vec<String>,
    ) -> Self {
        Self::TaskRequest {
            request_id: Uuid::new_v4().to_string(),
            requester_id: requester_id.into(),
            task: task.into(),
            required_capabilities,
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::TaskRequest { request_id, .. } => Some(request_id),
            Self::Bid { request_id, .. } => Some(request_id),
            Self::TaskAssignment { request_id, .. } => Some(request_id),
            Self::Result { request_id, .. } => Some(request_id),
            Self::Announcement(_) => None,
        }
    }
}
