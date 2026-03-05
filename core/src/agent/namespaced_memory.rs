use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::agent::memory::{MemoryManager, Memory};
use crate::error::Result;

/// Metadata for namespaced memory entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The actual value stored
    pub value: String,
    /// When this entry was created
    pub created_at: DateTime<Utc>,
    /// When this entry expires (None = never)
    pub expires_at: Option<DateTime<Utc>>,
    /// Namespace this entry belongs to
    pub namespace: String,
    /// Optional author/source
    pub author: Option<String>,
}

impl MemoryEntry {
    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false
        }
    }
}

/// Namespaced memory wrapper providing isolation and TTL support
/// 
/// # Architecture
/// 
/// ```text
/// ┌─────────────────────────────────────────┐
/// │      NamespacedMemory                   │
/// ├─────────────────────────────────────────┤
/// │  Namespace: "market"                    │
/// │    - btc_price → $43,200 (TTL: 5m)     │
/// │    - eth_price → $2,300  (TTL: 5m)     │
/// ├─────────────────────────────────────────┤
/// │  Namespace: "news"                      │
/// │    - latest → "Fed announces..." (TTL: 1h) │
/// ├─────────────────────────────────────────┤
/// │  Namespace: "analysis"                  │
/// │    - btc_signal → "BUY" (TTL: 30m)     │
/// └─────────────────────────────────────────┘
/// ```
/// 
/// # Benefits
/// 
/// - **Isolation**: Each namespace is independent
/// - **TTL**: Automatic expiration of stale data
/// - **Performance**: Shared data avoids redundant computation
/// - **Security**: Namespaces prevent cross-contamination
pub struct NamespacedMemory {
    memory: Arc<MemoryManager>,
}

impl NamespacedMemory {
    /// Create a new namespaced memory wrapper
    pub fn new(memory: Arc<MemoryManager>) -> Self {
        Self { memory }
    }

    /// Store a value in a specific namespace with optional TTL
    /// 
    /// # Arguments
    /// 
    /// * `namespace` - Namespace for isolation (e.g., "market", "news", "analysis")
    /// * `key` - Unique key within the namespace
    /// * `value` - Value to store
    /// * `ttl` - Optional time-to-live duration
    /// * `author` - Optional author/source identifier
    /// 
    /// # Example
    /// 
    /// ```ignore
    /// // Store market price with 5-minute TTL
    /// memory.store(
    ///     "market",
    ///     "btc_price",
    ///     "$43,200",
    ///     Some(Duration::from_secs(300)),
    ///     Some("PriceAPI")
    /// ).await?;
    /// ```
    pub async fn store(
        &self,
        namespace: &str,
        key: &str,
        value: &str,
        ttl: Option<Duration>,
        author: Option<String>,
    ) -> Result<()> {
        let full_key = format!("{}::{}", namespace, key);
        
        let entry = MemoryEntry {
            value: value.to_string(),
            created_at: Utc::now(),
            expires_at: ttl.map(|d| Utc::now() + chrono::Duration::from_std(d).unwrap()),
            namespace: namespace.to_string(),
            author,
        };

        let serialized = serde_json::to_string(&entry)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to serialize entry: {}", e)))?;

        self.memory.store_knowledge("system", None, &full_key, &serialized, "namespaced_memory", false).await
    }

    /// Read a value from a specific namespace
    /// 
    /// Returns `None` if:
    /// - Key doesn't exist
    /// - Entry has expired
    /// 
    /// # Example
    /// 
    /// ```ignore
    /// if let Some(price) = memory.read("market", "btc_price").await? {
    ///     println!("BTC price: {}", price);
    /// } else {
    ///     println!("Price not available or expired");
    /// }
    /// ```
    pub async fn read(&self, namespace: &str, key: &str) -> Result<Option<String>> {
        let full_key = format!("{}::{}", namespace, key);
        
        let results = self.memory.search("system", None, &full_key, 1).await?;
        
        if results.is_empty() {
            return Ok(None);
        }

        // Get the first (most recent) result
        let content = &results[0].content;
        
        let entry: MemoryEntry = serde_json::from_str(content)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to deserialize entry: {}", e)))?;

        // Check expiration
        if entry.is_expired() {
            return Ok(None);
        }

        Ok(Some(entry.value))
    }

    /// Read with metadata (including timestamp, author, etc.)
    pub async fn read_with_metadata(&self, namespace: &str, key: &str) -> Result<Option<MemoryEntry>> {
        let full_key = format!("{}::{}", namespace, key);
        
        let results = self.memory.search("system", None, &full_key, 1).await?;
        
        if results.is_empty() {
            return Ok(None);
        }

        let content = &results[0].content;
        
        let entry: MemoryEntry = serde_json::from_str(content)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to deserialize entry: {}", e)))?;

        if entry.is_expired() {
            return Ok(None);
        }

        Ok(Some(entry))
    }

    /// List all keys in a namespace
    pub async fn list_keys(&self, namespace: &str) -> Result<Vec<String>> {
        let prefix = format!("{}::", namespace);
        let results = self.memory.search("system", None, &prefix, 100).await?;
        
        let mut keys = Vec::new();
        for result in results {
            if let Ok(entry) = serde_json::from_str::<MemoryEntry>(&result.content) {
                if !entry.is_expired() {
                    // Extract key from full_key (remove namespace prefix)
                    let key = result.title.strip_prefix(&prefix)
                        .unwrap_or(&result.title)
                        .to_string();
                    keys.push(key);
                }
            }
        }
        
        Ok(keys)
    }

    /// Delete a key from a namespace
    pub async fn delete(&self, namespace: &str, key: &str) -> Result<()> {
        // For now, we "delete" by storing an expired entry.
        // The Memory trait should eventually include a delete method.
        self.store(namespace, key, "", Some(Duration::from_secs(0)), None).await
    }

    /// Clear all entries in a namespace
    pub async fn clear_namespace(&self, namespace: &str) -> Result<()> {
        let keys = self.list_keys(namespace).await?;
        for key in keys {
            self.delete(namespace, &key).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_and_read() {
        // This test would require a real MemoryManager instance
        // Skipped for now
    }

    #[test]
    fn test_entry_expiration() {
        let entry = MemoryEntry {
            value: "test".to_string(),
            created_at: Utc::now(),
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            namespace: "test".to_string(),
            author: None,
        };

        assert!(entry.is_expired());
    }
}
