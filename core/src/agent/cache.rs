//! Semantic caching for LLM responses
//!
//! Provides a mechanism to cache and reuse LLM completions based on prompt similarity.

use dashmap::DashMap;
use async_trait::async_trait;
use crate::agent::message::Message;
use crate::error::Result;

/// Trait for semantic caching
#[async_trait]
pub trait Cache: Send + Sync {
    /// Get a cached response for the given messages
    async fn get(&self, messages: &[Message]) -> Result<Option<String>>;
    
    /// Store a response in the cache
    async fn set(&self, messages: &[Message], response: String) -> Result<()>;
    
    /// Clear the cache
    async fn clear(&self) -> Result<()>;
}

/// A simple in-memory implementation of the Cache trait
/// 
/// Note: This is an exact-match cache for now. Truly 'semantic' caching 
/// (vector-based) should be implemented using aimaxxing_engram.
pub struct InMemoryCache {
    store: DashMap<String, String>,
}

impl InMemoryCache {
    /// Create a new in-memory cache
    pub fn new() -> Self {
        Self {
            store: DashMap::new(),
        }
    }
    
    /// Generate a simple key based on message content
    fn generate_key(&self, messages: &[Message]) -> String {
        let mut key = String::new();
        for msg in messages {
            key.push_str(msg.role.as_str());
            key.push_str(&msg.text());
        }
        // Hash it for a stable fixed-length key if content is huge
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish().to_string()
    }
}

#[async_trait]
impl Cache for InMemoryCache {
    async fn get(&self, messages: &[Message]) -> Result<Option<String>> {
        let key = self.generate_key(messages);
        Ok(self.store.get(&key).map(|v| v.value().clone()))
    }

    async fn set(&self, messages: &[Message], response: String) -> Result<()> {
        let key = self.generate_key(messages);
        self.store.insert(key, response);
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        self.store.clear();
        Ok(())
    }
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}
