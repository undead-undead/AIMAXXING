//! Memory system for agents
//!
//! Provides short-term (conversation) and long-term (persistent) memory.

use std::collections::VecDeque;
use std::sync::Arc;
#[cfg(feature = "cron")]
use std::sync::Weak;

use dashmap::DashMap;

use crate::agent::message::Message;
use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;

#[cfg(feature = "cron")]
use crate::agent::scheduler::Scheduler;

/// Trait for memory implementations
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a message
    async fn store(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        message: Message,
    ) -> crate::error::Result<()>;

    /// Store multiple messages efficiently
    async fn store_batch(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        messages: Vec<Message>,
    ) -> crate::error::Result<()> {
        for msg in messages {
            self.store(user_id, agent_id, msg).await?;
        }
        Ok(())
    }

    /// Retrieve recent messages
    async fn retrieve(&self, user_id: &str, agent_id: Option<&str>, limit: usize) -> Vec<Message>;

    /// Search the memory for relevant content
    #[cfg(feature = "vector-db")]
    async fn search(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> crate::error::Result<Vec<crate::knowledge::rag::Document>> {
        let _ = (user_id, agent_id, query, limit);
        Ok(Vec::new())
    }

    /// Store a specific piece of knowledge (not just a message)
    #[cfg(feature = "vector-db")]
    async fn store_knowledge(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        title: &str,
        content: &str,
        collection: &str,
        unverified: bool,
    ) -> crate::error::Result<()> {
        let _ = (user_id, agent_id, title, content, collection, unverified);
        Ok(())
    }

    /// Clear memory for a user
    async fn clear(&self, user_id: &str, agent_id: Option<&str>) -> crate::error::Result<()>;

    /// Undo last message
    async fn undo(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
    ) -> crate::error::Result<Option<Message>>;

    /// Update summary for a piece of knowledge
    #[cfg(feature = "vector-db")]
    async fn update_summary(
        &self,
        collection: &str,
        path: &str,
        summary: &str,
    ) -> crate::error::Result<()> {
        let _ = (collection, path, summary);
        Ok(())
    }

    /// Link a scheduler for background tasks
    #[cfg(feature = "cron")]
    fn link_scheduler(&self, _scheduler: Weak<Scheduler>) {}

    /// Fetch a full document by path
    #[cfg(feature = "vector-db")]
    async fn fetch_document(
        &self,
        collection: &str,
        path: &str,
    ) -> crate::error::Result<Option<crate::knowledge::rag::Document>> {
        let _ = (collection, path);
        Ok(None)
    }

    /// Store an agent session state
    async fn store_session(
        &self,
        _session: crate::agent::session::AgentSession,
    ) -> crate::error::Result<()> {
        Ok(())
    }

    /// Retrieve an agent session state
    async fn retrieve_session(
        &self,
        _session_id: &str,
    ) -> crate::error::Result<Option<crate::agent::session::AgentSession>> {
        Ok(None)
    }

    /// Perform maintenance (hygiene) tasks
    async fn maintenance(&self) -> crate::error::Result<()> {
        Ok(())
    }

    /// Phase 11-B: Mark the current session's recent messages as cancelled
    async fn mark_cancelled(
        &self,
        _user_id: &str,
        _agent_id: Option<&str>,
        _reason: &str,
    ) -> crate::error::Result<()> {
        // Default no-op; implementations can append a cancellation marker message
        Ok(())
    }

    /// Phase 12-C: List unverified memory entries for sleep-consolidation
    async fn list_unverified(&self, _limit: usize) -> crate::error::Result<Vec<Message>> {
        Ok(Vec::new())
    }

    /// Phase 12-C: Mark a memory entry as verified
    async fn mark_verified(&self, _entry_id: &str) -> crate::error::Result<()> {
        Ok(())
    }

    /// Phase 12-C: Mark a memory entry as pruned
    async fn mark_pruned(&self, _entry_id: &str) -> crate::error::Result<()> {
        Ok(())
    }

    /// Phase 14: Update the utility score of a memory entry
    async fn update_utility(&self, _collection: &str, _path: &str, _increment: f32) -> crate::error::Result<()> {
        Ok(())
    }
}

/// Short-term memory - stores recent conversation history
/// Uses a fixed-size ring buffer per user for memory efficiency
/// Persists to disk (JSON) to allow restarts without losing context.
pub struct ShortTermMemory {
    /// Max messages to keep per user
    max_messages: usize,
    /// Max active users/contexts to keep in memory (DoS protection)
    max_users: usize,
    /// Storage: composite_key -> message ring buffer
    store: DashMap<String, VecDeque<Message>>,
    /// Track last access time for cleanup
    last_access: DashMap<String, std::time::Instant>,
    /// Persistence path
    path: PathBuf,
}

impl ShortTermMemory {
    /// Create with custom capacity and persistence path
    pub async fn new(max_messages: usize, max_users: usize, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let store = DashMap::new();
        let last_access = DashMap::new();

        let mem = Self {
            max_messages,
            max_users,
            store,
            last_access,
            path,
        };

        // Try to load existing state
        if let Err(e) = mem.load().await {
            tracing::warn!(
                "Failed to load short-term memory from {:?}: {}",
                mem.path,
                e
            );
        }

        mem
    }

    /// Create with default capacity (100 messages per user, 1000 active users)
    pub async fn default_capacity() -> Self {
        Self::new(100, 1000, "data/short_term_memory.json").await
    }

    /// Load state from disk
    async fn load(&self) -> crate::error::Result<()> {
        if !self.path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&self.path).await.map_err(|e| {
            crate::error::Error::Internal(format!("Failed to read memory file: {}", e))
        })?;

        if content.trim().is_empty() {
            return Ok(());
        }

        let data: HashMap<String, VecDeque<Message>> =
            serde_json::from_str(&content).map_err(|e| {
                crate::error::Error::Internal(format!("Failed to parse memory file: {}", e))
            })?;

        self.store.clear();
        for (k, v) in data {
            self.store.insert(k.clone(), v);
            self.last_access.insert(k, std::time::Instant::now());
        }

        tracing::info!("Loaded short-term memory for {} users", self.store.len());
        Ok(())
    }

    /// Save state to disk
    async fn save(&self) -> crate::error::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        // Convert DashMap to HashMap for serialization
        let data: HashMap<_, _> = self
            .store
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();

        let json = serde_json::to_string_pretty(&data).map_err(|e| {
            crate::error::Error::Internal(format!("Failed to serialize memory: {}", e))
        })?;

        // Atomic save: write to tmp then rename
        let tmp_path = self.path.with_extension("tmp");
        tokio::fs::write(&tmp_path, json).await.map_err(|e| {
            crate::error::Error::Internal(format!("Failed to write temporary memory file: {}", e))
        })?;

        tokio::fs::rename(tmp_path, &self.path).await.map_err(|e| {
            crate::error::Error::Internal(format!("Failed to rename memory file: {}", e))
        })?;

        Ok(())
    }

    /// Get current message count for a user/agent pair
    pub fn message_count(&self, user_id: &str, agent_id: Option<&str>) -> usize {
        let key = self.key(user_id, agent_id);
        self.store.get(&key).map(|v| v.len()).unwrap_or(0)
    }

    /// Generate composite key
    fn key(&self, user_id: &str, agent_id: Option<&str>) -> String {
        if let Some(agent) = agent_id {
            format!("{}:{}", user_id, agent)
        } else {
            user_id.to_string()
        }
    }

    /// Prune inactive users (older than duration) - Useful for manual cleanup
    pub fn prune_inactive(&self, duration: std::time::Duration) {
        let now = std::time::Instant::now();
        // DashMap retain is efficient
        self.last_access.retain(|key, last_time| {
            let keep = now.duration_since(*last_time) < duration;
            if !keep {
                self.store.remove(key);
            }
            keep
        });
    }

    /// Check and enforce total user capacity (LRU eviction)
    fn enforce_user_capacity(&self) {
        if self.store.len() < self.max_users {
            return;
        }

        let mut oldest_key = None;
        let mut oldest_time = std::time::Instant::now();

        for r in self.last_access.iter() {
            if *r.value() < oldest_time {
                oldest_time = *r.value();
                oldest_key = Some(r.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.store.remove(&key);
            self.last_access.remove(&key);
        }
    }

    /// Pop the oldest N messages for a user
    pub async fn pop_oldest(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        count: usize,
    ) -> Vec<Message> {
        let key = self.key(user_id, agent_id);
        let mut popped = Vec::new();

        if let Some(mut entry) = self.store.get_mut(&key) {
            for _ in 0..count {
                if let Some(msg) = entry.pop_front() {
                    popped.push(msg);
                } else {
                    break;
                }
            }
        }

        if !popped.is_empty() {
            // Save change immediately
            let _ = self.save().await;
        }

        popped
    }
}

#[async_trait]
impl Memory for ShortTermMemory {
    async fn store(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        message: Message,
    ) -> crate::error::Result<()> {
        let key = self.key(user_id, agent_id);

        // Enforce capacity before inserting new user
        if !self.store.contains_key(&key) {
            self.enforce_user_capacity();
        }

        {
            let mut entry = self.store.entry(key.clone()).or_default();

            // Ring buffer behavior: remove oldest if at capacity
            // NOTE: With Tiered Storage, MemoryManager should handle archiving BEFORE this limit is hit commonly.
            // But as a safety net, we still keep the hard limit.
            if entry.len() >= self.max_messages {
                entry.pop_front();
            }
            entry.push_back(message);
        } // Lock on DashMap bucket dropped here

        // Update access time
        self.last_access.insert(key, std::time::Instant::now());

        // Save immediately for safety (Async I/O)
        // With Tiered storage, this file stays small (KB), so atomic write is fast enough.
        if let Err(e) = self.save().await {
            tracing::error!("Failed to persist short-term memory: {}", e);
        }

        Ok(())
    }

    async fn retrieve(&self, user_id: &str, agent_id: Option<&str>, limit: usize) -> Vec<Message> {
        let key = self.key(user_id, agent_id);
        self.store
            .get(&key)
            .map(|v| {
                // Update access time on retrieval too
                self.last_access.insert(key, std::time::Instant::now());

                let skip = v.len().saturating_sub(limit);
                v.iter().skip(skip).cloned().collect()
            })
            .unwrap_or_default()
    }

    #[cfg(feature = "vector-db")]
    async fn store_knowledge(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        title: &str,
        content: &str,
        collection: &str,
        unverified: bool,
    ) -> crate::error::Result<()> {
        let text = format!("[{}] {}: {}", collection, title, content);
        let mut msg = Message::assistant(text);
        msg.unverified = unverified;
        self.store(user_id, agent_id, msg).await
    }

    async fn clear(&self, user_id: &str, agent_id: Option<&str>) -> crate::error::Result<()> {
        let key = self.key(user_id, agent_id);
        self.store.remove(&key);
        self.last_access.remove(&key);

        self.save().await
    }

    async fn undo(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
    ) -> crate::error::Result<Option<Message>> {
        let key = self.key(user_id, agent_id);
        let msg = {
            let mut entry = self.store.entry(key.clone()).or_default();
            entry.pop_back()
        };

        if msg.is_some() {
            self.save().await?;
        }

        Ok(msg)
    }

    #[cfg(feature = "vector-db")]
    async fn search(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> crate::error::Result<Vec<crate::knowledge::rag::Document>> {
        let query_lower = query.to_lowercase();
        let messages = self.retrieve(user_id, agent_id, 1000).await; // Search through all STM for this user

        let mut results = Vec::new();
        for (i, msg) in messages.iter().enumerate() {
            let content = msg.text();
            if content.to_lowercase().contains(&query_lower) {
                results.push(crate::knowledge::rag::Document {
                    id: format!("stm_{}_{}", self.key(user_id, agent_id), i),
                    title: format!("Recent conversation ({})", msg.role.as_str()),
                    content: content.to_string(),
                    summary: None,
                    collection: None,
                    path: None,
                    metadata: HashMap::new(),
                    score: 0.9, // STM matches are highly relevant but given a fixed sub-1.0 score to prioritize exact LTM matches if needed
                });
            }
            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    async fn list_unverified(&self, limit: usize) -> crate::error::Result<Vec<Message>> {
        let mut unverified = Vec::new();
        // Since DashMap is high-concurrency, we iterate over all user buckets
        for r in self.store.iter() {
            for msg in r.value() {
                if msg.unverified {
                    unverified.push(msg.clone());
                    if unverified.len() >= limit {
                        return Ok(unverified);
                    }
                }
            }
        }
        Ok(unverified)
    }

    async fn mark_verified(&self, entry_content: &str) -> crate::error::Result<()> {
        for mut r in self.store.iter_mut() {
            for msg in r.value_mut() {
                if msg.unverified && msg.text() == entry_content {
                    msg.unverified = false;
                }
            }
        }
        self.save().await
    }

    async fn mark_pruned(&self, entry_content: &str) -> crate::error::Result<()> {
        for mut r in self.store.iter_mut() {
            r.value_mut().retain(|msg| !(msg.unverified && msg.text() == entry_content));
        }
        self.save().await
    }

    async fn maintenance(&self) -> crate::error::Result<()> {
        // Prune inactive users (default 24h)
        // TODO: Make this configurable via config.toml
        self.prune_inactive(std::time::Duration::from_secs(24 * 3600));

        // Also enforce max user capacity
        self.enforce_user_capacity();

        // Save state
        self.save().await
    }
}

/// Simple in-memory storage for testing or fast ephemeral context
pub struct InMemoryMemory {
    store: DashMap<String, VecDeque<Message>>,
}

impl InMemoryMemory {
    /// Create a new in-memory storage
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }

    fn key(&self, user_id: &str, agent_id: Option<&str>) -> String {
        if let Some(agent) = agent_id {
            format!("{}:{}", user_id, agent)
        } else {
            user_id.to_string()
        }
    }
}

impl Default for InMemoryMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Memory for InMemoryMemory {
    async fn store(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        message: Message,
    ) -> crate::error::Result<()> {
        let key = self.key(user_id, agent_id);
        self.store.entry(key).or_default().push_back(message);
        Ok(())
    }

    async fn retrieve(&self, user_id: &str, agent_id: Option<&str>, limit: usize) -> Vec<Message> {
        let key = self.key(user_id, agent_id);
        self.store
            .get(&key)
            .map(|v| {
                let skip = v.len().saturating_sub(limit);
                v.iter().skip(skip).cloned().collect()
            })
            .unwrap_or_default()
    }

    async fn clear(&self, user_id: &str, agent_id: Option<&str>) -> crate::error::Result<()> {
        let key = self.key(user_id, agent_id);
        self.store.remove(&key);
        Ok(())
    }

    async fn undo(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
    ) -> crate::error::Result<Option<Message>> {
        let key = self.key(user_id, agent_id);
        Ok(self.store.get_mut(&key).and_then(|mut v| v.pop_back()))
    }
}

/// Combined memory manager for tiered storage
pub struct MemoryManager {
    /// Hot Storage Layer (e.g. In-memory or fast local cache)
    pub hot_tier: Arc<dyn Memory>,
    /// Cold Storage Layer (e.g. SQLite, Vector DB)
    pub cold_tier: Arc<dyn Memory>,
}

impl MemoryManager {
    /// Create a new MemoryManager with specific backends
    pub fn new(hot_tier: Arc<dyn Memory>, cold_tier: Arc<dyn Memory>) -> Self {
        Self {
            hot_tier,
            cold_tier,
        }
    }

    /// Tiered Storage Store
    /// Stores in Hot Tier, then auto-archives to Cold Tier if capacity exceeded
    pub async fn store(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        message: Message,
    ) -> crate::error::Result<()> {
        // 1. Write to Hot Storage - Fast
        self.hot_tier.store(user_id, agent_id, message).await?;

        // 2. Archive older messages if needed
        // Note: The specific logic for "when to archive" could be moved to a TieringPolicy
        // For now, we use a simple heuristic if the Hot Tier supports counting.
        // Since we are now using dyn Memory, we might need to add a 'count' method to the trait
        // if we want generic tiering logic here, or let the Hot Tier handle its own overflow.

        Ok(())
    }

    /// Unified Retrieve
    /// Fetches from Hot + Cold seamlessly
    pub async fn retrieve_unified(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        limit: usize,
    ) -> Vec<Message> {
        let mut messages = self.hot_tier.retrieve(user_id, agent_id, limit).await;

        if messages.len() < limit {
            let needed = limit - messages.len();
            let cold_messages = self.cold_tier.retrieve(user_id, agent_id, needed).await;

            let mut combined = cold_messages;
            combined.extend(messages);
            messages = combined;
        }

        messages
    }

    /// Union Search - searches both Hot and Cold tiers
    #[cfg(feature = "vector-db")]
    pub async fn search_unified(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> crate::error::Result<Vec<crate::knowledge::rag::Document>> {
        let hot_results = self
            .hot_tier
            .search(user_id, agent_id, query, limit)
            .await?;
        let cold_results = self
            .cold_tier
            .search(user_id, agent_id, query, limit)
            .await?;

        let mut combined = hot_results;
        for cold_res in cold_results {
            if !combined.iter().any(|r| r.content == cold_res.content) {
                combined.push(cold_res);
            }
        }

        combined.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        combined.truncate(limit);

        Ok(combined)
    }

    /// Undo last message
    pub async fn undo(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
    ) -> crate::error::Result<Option<Message>> {
        let hot_msg = self.hot_tier.undo(user_id, agent_id).await?;
        let _ = self.cold_tier.undo(user_id, agent_id).await?;
        Ok(hot_msg)
    }
}

/// Injects learned lessons from past experiences into the context
pub struct LearnedMemoryInjector {
    memory: Arc<dyn Memory>,
}

impl LearnedMemoryInjector {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl crate::agent::context::ContextInjector for LearnedMemoryInjector {
    #[cfg(feature = "vector-db")]
    async fn inject(&self, history: &[Message]) -> crate::error::Result<Vec<Message>> {
        // Only trigger learned memory if there is history to derive a query from
        let last_user_msg = history
            .iter()
            .rev()
            .find(|m| m.role == crate::agent::message::Role::User);

        if let Some(msg) = last_user_msg {
            let query = msg.text();

            // Search in the 'lessons' collection (implied convention)
            // Or just search through all long-term memory.
            // Let's search LTM with a limit.
            let lessons = self.memory.search("default", None, &query, 3).await?;

            match lessons {
                docs if !docs.is_empty() => {
                    let mut content = String::from("### Learned Lessons & Relevant Experience\n\n");
                    content.push_str("Below are relevant insights from your previous experiences that might apply to the current situation:\n\n");
                    for doc in docs {
                        content.push_str(&format!("- **{}**: {}\n", doc.title, doc.content));
                    }
                    Ok(vec![Message::system(content)])
                }
                _ => Ok(Vec::new()),
            }
        } else {
            Ok(Vec::new())
        }
    }

    #[cfg(not(feature = "vector-db"))]
    async fn inject(&self, _history: &[Message]) -> crate::error::Result<Vec<Message>> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl Memory for MemoryManager {
    async fn store(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        message: Message,
    ) -> crate::error::Result<()> {
        self.store(user_id, agent_id, message).await
    }

    async fn retrieve(&self, user_id: &str, agent_id: Option<&str>, limit: usize) -> Vec<Message> {
        self.retrieve_unified(user_id, agent_id, limit).await
    }

    #[cfg(feature = "vector-db")]
    async fn search(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        query: &str,
        limit: usize,
    ) -> crate::error::Result<Vec<crate::knowledge::rag::Document>> {
        self.search_unified(user_id, agent_id, query, limit).await
    }

    #[cfg(feature = "vector-db")]
    async fn store_knowledge(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        title: &str,
        content: &str,
        collection: &str,
        unverified: bool,
    ) -> crate::error::Result<()> {
        // Knowledge usually goes directly to Cold tier for permanence
        self.cold_tier
            .store_knowledge(user_id, agent_id, title, content, collection, unverified)
            .await
    }

    async fn clear(&self, user_id: &str, agent_id: Option<&str>) -> crate::error::Result<()> {
        self.hot_tier.clear(user_id, agent_id).await?;
        self.cold_tier.clear(user_id, agent_id).await?;
        Ok(())
    }

    async fn undo(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
    ) -> crate::error::Result<Option<Message>> {
        self.undo(user_id, agent_id).await
    }

    async fn store_session(
        &self,
        session: crate::agent::session::AgentSession,
    ) -> crate::error::Result<()> {
        self.cold_tier.store_session(session).await
    }

    async fn retrieve_session(
        &self,
        session_id: &str,
    ) -> crate::error::Result<Option<crate::agent::session::AgentSession>> {
        self.cold_tier.retrieve_session(session_id).await
    }

    #[cfg(feature = "vector-db")]
    async fn fetch_document(
        &self,
        collection: &str,
        path: &str,
    ) -> crate::error::Result<Option<crate::knowledge::rag::Document>> {
        // Cold tier usually holds the full documents
        // Cold tier usually holds the full documents
        self.cold_tier.fetch_document(collection, path).await
    }

    async fn maintenance(&self) -> crate::error::Result<()> {
        self.hot_tier.maintenance().await?;
        self.cold_tier.maintenance().await?;
        Ok(())
    }

    async fn list_unverified(&self, limit: usize) -> crate::error::Result<Vec<Message>> {
        // Search both tiers for unverified memories
        let mut results = self.hot_tier.list_unverified(limit).await?;
        if results.len() < limit {
            let cold_results = self.cold_tier.list_unverified(limit - results.len()).await?;
            results.extend(cold_results);
        }
        Ok(results)
    }

    async fn mark_verified(&self, entry_content: &str) -> crate::error::Result<()> {
        self.hot_tier.mark_verified(entry_content).await?;
        self.cold_tier.mark_verified(entry_content).await?;
        Ok(())
    }

    async fn mark_pruned(&self, entry_content: &str) -> crate::error::Result<()> {
        self.hot_tier.mark_pruned(entry_content).await?;
        self.cold_tier.mark_pruned(entry_content).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_short_term_memory() {
        let memory = ShortTermMemory::new(3, 10, "test_stm.json").await;

        memory
            .store("user1", None, Message::user("Hello"))
            .await
            .unwrap();
        memory
            .store("user1", None, Message::assistant("Hi there"))
            .await
            .unwrap();
        memory
            .store("user1", None, Message::user("How are you?"))
            .await
            .unwrap();
        // This should evict "Hello"
        memory
            .store("user1", None, Message::assistant("I'm good!"))
            .await
            .unwrap();

        let messages = memory.retrieve("user1", None, 10).await;
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].text(), "Hi there");

        let _ = std::fs::remove_file("test_stm.json");
    }
}
