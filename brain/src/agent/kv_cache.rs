//! Low-latency Paged KV Cache with Two-Tier Quantization
//!
//! Provides efficient memory management for LLM inference by:
//! 1. **Two-Tier Storage**: Keeping the "tail" (recent tokens) in FP16 for
//!    precision, and compressing the "body" (older tokens) to Q4/Q1.58.
//! 2. **Paged Attention**: Managing KV tensors in fixed-size blocks to avoid
//!    fragmentation and enable efficient causal-aware loading.
//! 3. **Memory Pool**: Using pre-allocated, aligned buffers for zero-copy access.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Configuration for the KV cache
#[derive(Debug, Clone)]
pub struct KvCacheConfig {
    pub page_size: usize,       // Number of tokens per page
    pub num_pages: usize,       // Total pages in the pool
    pub head_dim: usize,        // Dimension per head
    pub num_heads: usize,       // Number of attention heads
    pub quant_threshold: usize, // Move to Q4 after this many tokens
}

impl Default for KvCacheConfig {
    fn default() -> Self {
        Self {
            page_size: 16,
            num_pages: 1024,
            head_dim: 128,
            num_heads: 32,
            quant_threshold: 128,
        }
    }
}

/// A single page in the KV cache
#[derive(Clone)]
pub struct KvPage {
    pub id: usize,
    /// Key cache: (num_heads, page_size, head_dim)
    pub k_data: Vec<u8>,
    /// Value cache: (num_heads, page_size, head_dim)
    pub v_data: Vec<u8>,
    pub is_compressed: bool,
    pub last_access: u64,
}

/// Two-tier KV Cache manager
pub struct TwoTierKvCache {
    config: KvCacheConfig,
    /// Physical page pool
    pages: Vec<Arc<RwLock<KvPage>>>,
    /// Mapping from request/session to logical page indices
    request_map: HashMap<String, Vec<usize>>,
    /// Free page list
    free_pages: Vec<usize>,
}

impl TwoTierKvCache {
    pub fn new(config: KvCacheConfig) -> Self {
        let mut pages = Vec::with_capacity(config.num_pages);
        let mut free_pages = Vec::with_capacity(config.num_pages);

        for i in 0..config.num_pages {
            let page = KvPage {
                id: i,
                k_data: vec![0u8; config.num_heads * config.page_size * config.head_dim * 2], // Default FP16 (2 bytes)
                v_data: vec![0u8; config.num_heads * config.page_size * config.head_dim * 2],
                is_compressed: false,
                last_access: 0,
            };
            pages.push(Arc::new(RwLock::new(page)));
            free_pages.push(i);
        }

        Self {
            config,
            pages,
            request_map: HashMap::new(),
            free_pages,
        }
    }

    /// Allocate a new page for a request
    pub fn allocate_page(&mut self, request_id: &str) -> Option<usize> {
        if let Some(page_id) = self.free_pages.pop() {
            let entry = self.request_map.entry(request_id.to_string()).or_default();
            entry.push(page_id);
            Some(page_id)
        } else {
            None
        }
    }

    /// Compress older pages to Q4 (stub for SIMD implementation)
    pub fn compress_old_pages(&mut self, request_id: &str) {
        if let Some(pages_ids) = self.request_map.get(request_id) {
            let num_pages = pages_ids.len();
            if num_pages > (self.config.quant_threshold / self.config.page_size) {
                // Compress all but the last few pages
                for &page_id in &pages_ids[..num_pages - 1] {
                    let mut page = self.pages[page_id].write().unwrap();
                    if !page.is_compressed {
                        // TODO: Call SIMD Q4 quantizer here
                        page.is_compressed = true;
                    }
                }
            }
        }
    }

    /// Retrieve multiple pages for a request as a logical sequence
    pub fn get_request_pages(&self, request_id: &str) -> Vec<Arc<RwLock<KvPage>>> {
        self.request_map
            .get(request_id)
            .map(|ids| ids.iter().map(|&id| Arc::clone(&self.pages[id])).collect())
            .unwrap_or_default()
    }

    /// Causal-aware KV Loading: Prioritize loading pages that are likely
    /// to be accessed based on the Causal_Efficiency score.
    pub fn prefetch_causal_pages(&mut self, _collection: &str, _query_vec: &[f32]) {
        // Implementation will interface with Engram's causal scores
        // for Phase 24: Intelligent Context Pre-warming.
    }

    /// Release pages for a request
    pub fn release_request(&mut self, request_id: &str) {
        if let Some(ids) = self.request_map.remove(request_id) {
            for id in ids {
                self.free_pages.push(id);
            }
        }
    }
}
