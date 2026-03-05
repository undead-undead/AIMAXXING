use std::sync::Arc;
use std::collections::HashMap;
use async_trait::async_trait;
use parking_lot::RwLock;
use crate::error::Result;
use crate::agent::swarm::manifest::AgentManifest;

/// Trait for discovering other agents in the swarm
#[async_trait]
pub trait Discovery: Send + Sync {
    /// Register or update an agent's manifest
    async fn register(&self, manifest: AgentManifest) -> Result<()>;

    /// Remove an agent from discovery
    async fn unregister(&self, agent_id: &str) -> Result<()>;

    /// Get a specific agent's manifest
    async fn get(&self, agent_id: &str) -> Result<Option<AgentManifest>>;

    /// List all discovered agents
    async fn list(&self) -> Result<Vec<AgentManifest>>;
    
    /// Find agents with a specific capability
    async fn find_by_capability(&self, capability: &str) -> Result<Vec<AgentManifest>>;
}

/// Local in-memory discovery mechanism
#[derive(Debug, Clone)]
pub struct LocalDiscovery {
    registry: Arc<RwLock<HashMap<String, AgentManifest>>>,
}

impl LocalDiscovery {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Discovery for LocalDiscovery {
    async fn register(&self, manifest: AgentManifest) -> Result<()> {
        self.registry.write().insert(manifest.id.clone(), manifest);
        Ok(())
    }

    async fn unregister(&self, agent_id: &str) -> Result<()> {
        self.registry.write().remove(agent_id);
        Ok(())
    }

    async fn get(&self, agent_id: &str) -> Result<Option<AgentManifest>> {
        Ok(self.registry.read().get(agent_id).cloned())
    }

    async fn list(&self) -> Result<Vec<AgentManifest>> {
        Ok(self.registry.read().values().cloned().collect())
    }

    async fn find_by_capability(&self, capability: &str) -> Result<Vec<AgentManifest>> {
        let registry = self.registry.read();
        let matches = registry.values()
            .filter(|m| m.capabilities.contains(capability))
            .cloned()
            .collect();
        Ok(matches)
    }
}
