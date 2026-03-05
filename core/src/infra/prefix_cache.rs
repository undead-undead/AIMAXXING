//! Phase 17: Prefix Cache — KV state caching for SOUL/IDENTITY prompts.
//!
//! Caches the tokenized/processed representation of static system prompts
//! (SOUL.md + IDENTITY.md) to enable near-instant agent cold starts.
//! Implements consistency through hash-based invalidation.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// A cached prefix entry
#[derive(Debug, Clone)]
pub struct CachedPrefix {
    /// Hash of the source content for invalidation
    pub content_hash: u64,
    /// The processed/tokenized content
    pub processed: String,
    /// When this entry was last validated
    pub last_validated: std::time::Instant,
    /// How many times this cache has been hit
    pub hit_count: u64,
}

/// Statistics for the prefix cache
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub entries: usize,
    pub total_hits: u64,
    pub total_misses: u64,
    pub hit_rate: f64,
}

/// Thread-safe prefix cache for system prompts.
///
/// Stores pre-processed system prompts keyed by role name.
/// Invalidation is hash-based: if the SOUL.md or IDENTITY.md
/// content changes, the old cache entry is automatically replaced.
pub struct PrefixCache {
    cache: Arc<RwLock<HashMap<String, CachedPrefix>>>,
    hits: Arc<RwLock<u64>>,
    misses: Arc<RwLock<u64>>,
}

impl Default for PrefixCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PrefixCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            hits: Arc::new(RwLock::new(0)),
            misses: Arc::new(RwLock::new(0)),
        }
    }

    /// Get a cached prefix, or return None if stale/missing
    pub fn get(&self, role: &str, current_content_hash: u64) -> Option<String> {
        let result = {
            let cache = self.cache.read();
            cache.get(role).and_then(|entry| {
                if entry.content_hash == current_content_hash {
                    Some(entry.processed.clone())
                } else {
                    None
                }
            })
        };

        if result.is_some() {
            // Update hit count (write lock acquired after read lock is dropped)
            let mut cache_w = self.cache.write();
            if let Some(entry) = cache_w.get_mut(role) {
                entry.hit_count += 1;
                entry.last_validated = std::time::Instant::now();
            }
            *self.hits.write() += 1;
        } else {
            *self.misses.write() += 1;
        }

        result
    }

    /// Store a processed prefix in the cache
    pub fn put(&self, role: &str, content_hash: u64, processed: String) {
        let mut cache = self.cache.write();
        cache.insert(
            role.to_string(),
            CachedPrefix {
                content_hash,
                processed,
                last_validated: std::time::Instant::now(),
                hit_count: 0,
            },
        );
    }

    /// Invalidate a specific role's cache
    pub fn invalidate(&self, role: &str) {
        let mut cache = self.cache.write();
        cache.remove(role);
        tracing::info!(role = %role, "Prefix cache invalidated");
    }

    /// Invalidate all cached entries
    pub fn invalidate_all(&self) {
        let mut cache = self.cache.write();
        cache.clear();
        tracing::info!("All prefix caches invalidated");
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read();
        let hits = *self.hits.read();
        let misses = *self.misses.read();
        let total = hits + misses;

        CacheStats {
            entries: cache.len(),
            total_hits: hits,
            total_misses: misses,
            hit_rate: if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            },
        }
    }

    /// Compute a content hash for invalidation checks
    pub fn hash_content(content: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }
}

/// Monitors configuration changes and triggers cache invalidation.
pub struct CacheInvalidator {
    cache: Arc<PrefixCache>,
    /// Known hashes for tracking changes
    known_hashes: HashMap<String, u64>,
}

impl CacheInvalidator {
    pub fn new(cache: Arc<PrefixCache>) -> Self {
        Self {
            cache,
            known_hashes: HashMap::new(),
        }
    }

    /// Check if a role's content has changed and invalidate if needed
    pub fn check_and_invalidate(&mut self, role: &str, current_content: &str) -> bool {
        let new_hash = PrefixCache::hash_content(current_content);
        let old_hash = self.known_hashes.get(role).copied();

        if old_hash != Some(new_hash) {
            self.cache.invalidate(role);
            self.known_hashes.insert(role.to_string(), new_hash);
            true // Cache was invalidated
        } else {
            false // No change
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_miss_then_hit() {
        let cache = PrefixCache::new();
        let hash = PrefixCache::hash_content("Be helpful.");

        // Miss
        assert!(cache.get("assistant", hash).is_none());

        // Put
        cache.put("assistant", hash, "processed_prompt".into());

        // Hit
        let result = cache.get("assistant", hash);
        assert_eq!(result, Some("processed_prompt".into()));
    }

    #[test]
    fn test_cache_invalidation_on_content_change() {
        let cache = PrefixCache::new();
        let hash1 = PrefixCache::hash_content("Version 1");
        let hash2 = PrefixCache::hash_content("Version 2");

        cache.put("assistant", hash1, "v1_processed".into());
        assert!(cache.get("assistant", hash1).is_some());

        // Different hash = stale
        assert!(cache.get("assistant", hash2).is_none());
    }

    #[test]
    fn test_explicit_invalidation() {
        let cache = PrefixCache::new();
        let hash = PrefixCache::hash_content("content");
        cache.put("test", hash, "processed".into());
        assert!(cache.get("test", hash).is_some());

        cache.invalidate("test");
        assert!(cache.get("test", hash).is_none());
    }

    #[test]
    fn test_cache_stats() {
        let cache = PrefixCache::new();
        let hash = PrefixCache::hash_content("x");
        cache.put("a", hash, "p".into());

        cache.get("a", hash); // Hit
        cache.get("b", hash); // Miss

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.total_hits, 1);
        assert_eq!(stats.total_misses, 1);
        assert!((stats.hit_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_cache_invalidator() {
        let cache = Arc::new(PrefixCache::new());
        let mut invalidator = CacheInvalidator::new(cache.clone());

        // First check always invalidates
        assert!(invalidator.check_and_invalidate("role", "content v1"));

        // Same content, no invalidation
        assert!(!invalidator.check_and_invalidate("role", "content v1"));

        // Changed content, invalidates
        assert!(invalidator.check_and_invalidate("role", "content v2"));
    }
}
