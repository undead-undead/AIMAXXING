use crate::error::Result;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "vector")]
use crate::embedder::Embedder;
#[cfg(feature = "vector")]
use crate::local_reranker::LocalCandleReranker;

/// Types of models managed by the pool
#[derive(Clone)]
pub enum ModelResource {
    #[cfg(feature = "vector")]
    Embedder(Arc<Embedder>),
    #[cfg(feature = "vector")]
    Reranker(Arc<LocalCandleReranker>),
}

impl ModelResource {
    pub fn memory_size(&self) -> usize {
        match self {
            #[cfg(feature = "vector")]
            ModelResource::Embedder(e) => e.memory_size(),
            #[cfg(feature = "vector")]
            ModelResource::Reranker(r) => r.memory_size(),
        }
    }

    pub fn is_gpu(&self) -> bool {
        match self {
            #[cfg(feature = "vector")]
            ModelResource::Embedder(e) => e.is_gpu(),
            #[cfg(feature = "vector")]
            ModelResource::Reranker(r) => r.is_gpu(),
        }
    }
}

struct PoolEntry {
    resource: ModelResource,
    last_used: Instant,
}

/// A centralized pool for managing local AI models with LRU eviction.
pub struct ModelPool {
    /// Max RAM budget in bytes
    max_ram: Mutex<usize>,
    /// Max VRAM budget in bytes
    max_vram: Mutex<usize>,
    /// Loaded models: key is a unique string (usually model path or ID)
    entries: Mutex<HashMap<String, PoolEntry>>,
}

impl ModelPool {
    /// Create a new model pool with dual memory budgets.
    pub fn new(max_ram: usize, max_vram: usize) -> Self {
        Self {
            max_ram: Mutex::new(max_ram),
            max_vram: Mutex::new(max_vram),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Update budgets at runtime
    pub fn set_budgets(&self, ram_bytes: usize, vram_bytes: usize) {
        *self.max_ram.lock() = ram_bytes;
        *self.max_vram.lock() = vram_bytes;
        // Trigger an eviction check for both to stay within new limits
        self.evict_for_space(0, false);
        self.evict_for_space(0, true);
    }

    /// Calculate current total memory usage of all loaded models
    pub fn current_usage(&self) -> (usize, usize) {
        let entries = self.entries.lock();
        let mut ram = 0;
        let mut vram = 0;
        for e in entries.values() {
            if e.resource.is_gpu() {
                vram += e.resource.memory_size();
            } else {
                ram += e.resource.memory_size();
            }
        }
        (ram, vram)
    }

    /// Evict models until enough space is available for a new model.
    fn evict_for_space(&self, required_size: usize, is_gpu: bool) {
        let limit = if is_gpu {
            *self.max_vram.lock()
        } else {
            *self.max_ram.lock()
        };

        if required_size > limit {
            tracing::warn!(
                "Model size ({}) exceeds total {} budget ({})",
                required_size,
                if is_gpu { "VRAM" } else { "RAM" },
                limit
            );
            return;
        }

        let mut entries = self.entries.lock();

        loop {
            let current_usage: usize = entries
                .values()
                .filter(|e| e.resource.is_gpu() == is_gpu)
                .map(|e| e.resource.memory_size())
                .sum();

            if current_usage + required_size <= limit {
                break;
            }

            // Find the least recently used entry FOR THIS DEVICE TYPE
            let lru_key = entries
                .iter()
                .filter(|(_, entry)| entry.resource.is_gpu() == is_gpu)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone());

            if let Some(key) = lru_key {
                if let Some(removed) = entries.remove(&key) {
                    tracing::info!(
                        "Evicting model '{}' ({} MB from {}) to free space",
                        key,
                        removed.resource.memory_size() / 1024 / 1024,
                        if is_gpu { "VRAM" } else { "RAM" }
                    );
                }
            } else {
                break;
            }
        }
    }

    #[cfg(feature = "vector")]
    pub fn get_embedder(
        &self,
        key: &str,
        loader: impl FnOnce() -> Result<Embedder>,
    ) -> Result<Arc<Embedder>> {
        let mut entries = self.entries.lock();

        // 1. Check if already in pool
        if let Some(entry) = entries.get_mut(key) {
            if let ModelResource::Embedder(ref emb) = entry.resource {
                entry.last_used = Instant::now();
                return Ok(Arc::clone(emb));
            }
        }

        // 2. Load model (drop lock during load to prevent deadlocks)
        drop(entries);
        let model = loader()?;
        let size = model.memory_size();
        let is_gpu = model.is_gpu();

        // 3. Evict space and insert
        self.evict_for_space(size, is_gpu);

        let mut entries = self.entries.lock();
        let arc_model = Arc::new(model);
        entries.insert(
            key.to_string(),
            PoolEntry {
                resource: ModelResource::Embedder(Arc::clone(&arc_model)),
                last_used: Instant::now(),
            },
        );

        Ok(arc_model)
    }

    #[cfg(feature = "vector")]
    pub fn get_reranker(
        &self,
        key: &str,
        loader: impl FnOnce() -> Result<LocalCandleReranker>,
    ) -> Result<Arc<LocalCandleReranker>> {
        let mut entries = self.entries.lock();

        if let Some(entry) = entries.get_mut(key) {
            if let ModelResource::Reranker(ref rerank) = entry.resource {
                entry.last_used = Instant::now();
                return Ok(Arc::clone(rerank));
            }
        }

        drop(entries);
        let model = loader()?;
        let size = model.memory_size();
        let is_gpu = model.is_gpu();

        self.evict_for_space(size, is_gpu);

        let mut entries = self.entries.lock();
        let arc_model = Arc::new(model);
        entries.insert(
            key.to_string(),
            PoolEntry {
                resource: ModelResource::Reranker(Arc::clone(&arc_model)),
                last_used: Instant::now(),
            },
        );

        Ok(arc_model)
    }
}
