use crate::agent::multi_agent::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Status of an agent in the swarm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Online,
    Busy,
    Offline,
    Error,
}

/// Metadata about an agent in the swarm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub id: String,
    pub name: String,
    pub role: AgentRole,
    pub capabilities: Vec<String>,
    pub address: Option<String>,
    pub status: AgentStatus,
    pub last_seen: DateTime<Utc>,
    pub version: String,
}

impl AgentManifest {
    pub fn new(id: impl Into<String>, name: impl Into<String>, role: AgentRole) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role,
            capabilities: Vec::new(),
            address: None,
            status: AgentStatus::Online,
            last_seen: Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }
}
