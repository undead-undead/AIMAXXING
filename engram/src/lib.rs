//! # Engram — High-Performance Memory Engine for AIMAXXING
//!
//! Engram provides intelligent knowledge storage and retrieval:
//! - **Engram-KV**: Pure Rust KV storage engine based on `redb` (mmap, ACID, zero-copy)
//! - **BM25 Full-Text Search**: Pure Rust inverted index (no SQLite dependency)
//! - **Vector Similarity Search**: HNSW index with SIMD-accelerated distance computation
//! - **Product Quantization**: f32 → low-bitwidth compression (4-8x memory savings)
//! - **Content-Addressable Storage**: Automatic deduplication via SHA-256

// Core modules
pub mod content_hash;
pub mod error;
pub mod kv;
pub mod store;

// Search modules
pub mod fts;
pub mod hybrid_search;
pub mod rrf;

// Vector search (feature-gated)
#[cfg(feature = "vector")]
pub mod embedder;
#[cfg(feature = "vector")]
pub mod quant;
#[cfg(feature = "vector")]
pub mod simd_kernels;
#[cfg(feature = "vector")]
pub mod vector_store;

// Agent integration
pub mod agent_memory;
pub mod tool;

// File management
pub mod chunker;
pub mod indexer;
pub mod intent;
pub mod retriever;
pub mod virtual_path;
pub mod watcher;

// Re-exports
pub use error::{EngramError, Result};
pub use kv::EngramKV;
pub use store::{Collection, Document, EngramStore, StoreStats}; // Removed SearchResult

pub use hybrid_search::{
    HybridSearchConfig, HybridSearchEngine, HybridSearchResult, HybridSearchStats,
};
pub use rrf::{FusedResult, RrfConfig, RrfFusion};
pub use tool::KnowledgeSearchTool;

pub use agent_memory::EngramMemory;
pub use content_hash::{get_docid, hash_content, normalize_docid, validate_docid};
pub use virtual_path::VirtualPath;
pub use watcher::FileWatcher;

#[cfg(feature = "vector")]
pub use embedder::{Embedder, EmbedderConfig};
#[cfg(feature = "vector")]
pub use quant::{QuantLevel, Quantizer, ScalarQuantizer};
#[cfg(feature = "vector")]
pub use simd_kernels::dot_product_q4_f32;
#[cfg(feature = "vector")]
pub use vector_store::{VectorEntry, VectorStore}; // Removed VectorSearchResult

pub use chunker::{Chunk, ChunkStats, Chunker, ChunkerConfig};
pub use retriever::HierarchicalRetriever;
