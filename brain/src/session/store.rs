use async_trait::async_trait;
use crate::agent::message::Message;
use crate::error::Result;

/// Trait for persisting and retrieving conversation sessions.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Save session messages to persistent storage.
    async fn save(&self, id: &str, messages: &[Message]) -> Result<()>;
    
    /// Load session messages from persistent storage.
    async fn load(&self, id: &str) -> Result<Option<Vec<Message>>>;
    
    /// Delete sessions older than a specific duration.
    async fn delete_stale(&self, max_age_days: u32) -> Result<usize>;
}
