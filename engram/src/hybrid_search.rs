//! Hybrid search engine combining BM25 and vector similarity search
//!
//! Integrates keyword-based (BM25) and semantic (vector) search using RRF fusion.

#[cfg(feature = "vector")]
use crate::embedder::Embedder;
use crate::error::{EngramError, Result};
#[cfg(feature = "vector")]
use crate::quant::QuantLevel;
use crate::store::{Collection, Document, EngramStore};
#[cfg(feature = "vector")]
use crate::vector_store::VectorStore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for hybrid search
#[derive(Debug, Clone)]
pub struct HybridSearchConfig {
    pub db_path: PathBuf,
    pub vector_dimension: usize,
    pub max_vectors: usize,
    pub rrf_k: f64,
    pub bm25_weight: f64,
    pub vector_weight: f64,
    pub dedup_threshold: f32,
    pub vector_metric: crate::vector_store::VectorMetric,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("engram.db"),
            vector_dimension: 384,
            max_vectors: 100_000,
            rrf_k: 60.0,
            bm25_weight: 0.4,
            vector_weight: 0.6,
            dedup_threshold: 0.85,
            vector_metric: crate::vector_store::VectorMetric::Cosine,
        }
    }
}

/// Hybrid search result combining BM25 and vector search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub document: Document,
    pub rrf_score: f64,
    pub bm25_score: Option<f64>,
    pub vector_score: Option<f32>,
    pub causal_efficiency: f32,
    pub latency_ms: f32,
    pub rank: usize,
}

/// Hybrid search statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchStats {
    pub total_documents: u64,
    pub total_vectors: usize,
    pub total_collections: usize,
    pub database_path: String,
}

/// Hybrid search engine
pub struct HybridSearchEngine {
    store: Arc<EngramStore>,
    #[cfg(feature = "vector")]
    vector_store: Option<Arc<VectorStore>>,
    #[cfg(feature = "vector")]
    embedder: Option<Arc<Embedder>>,
    config: HybridSearchConfig,
}

impl HybridSearchEngine {
    /// Create a new hybrid search engine
    pub fn new(config: HybridSearchConfig) -> Result<Self> {
        let store = Arc::new(EngramStore::new(&config.db_path)?);

        #[cfg(feature = "vector")]
        let (vector_store, embedder) = {
            let vs_path = config.db_path.with_extension("vectors.bin");
            let vs = if vs_path.exists() {
                VectorStore::load(&vs_path).ok()
            } else {
                Some(VectorStore::new(
                    config.vector_dimension,
                    config.max_vectors,
                    config.vector_metric.into(), // Conversion if needed
                ))
            };

            // Note: This might be heavy, so we might want to make it optional or lazy
            let emb = Embedder::new().ok();
            (vs.map(Arc::new), emb.map(Arc::new))
        };

        Ok(Self {
            store,
            #[cfg(feature = "vector")]
            vector_store,
            #[cfg(feature = "vector")]
            embedder,
            config,
        })
    }

    /// Index a document with differentiated quantization (Soul vs Background)
    #[cfg(feature = "vector")]
    pub fn index_at_level(
        &self,
        collection: &str,
        path: &str,
        title: &str,
        content: &str,
        level: QuantLevel,
        unverified: bool,
    ) -> Result<()> {
        // 1. Text indexing (FTS)
        self.store
            .store_document(collection, path, title, content, unverified)?;

        // 2. Vector indexing
        if let (Some(vs), Some(emb)) = (&self.vector_store, &self.embedder) {
            let embedding = emb.embed(content)?;
            vs.add_at_level(collection, path, title, 0, embedding, level)?;
        }

        Ok(())
    }

    /// Create collection
    pub fn create_collection(&self, collection: Collection) -> Result<()> {
        self.store.create_collection(collection)
    }

    /// Index a document (stores in BM25 store)
    pub fn index_document(
        &self,
        collection: &str,
        path: &str,
        title: &str,
        content: &str,
        unverified: bool,
    ) -> Result<()> {
        self.store
            .store_document(collection, path, title, content, unverified)?;
        Ok(())
    }

    /// Index multiple documents in batch
    pub fn index_batch(&self, documents: Vec<(&str, &str, &str, &str, bool)>) -> Result<()> {
        for (collection, path, title, content, unverified) in documents {
            self.store
                .store_document(collection, path, title, content, unverified)?;
        }
        Ok(())
    }

    /// Hybrid search combining BM25 and vector search
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<HybridSearchResult>> {
        // 1. BM25 search
        let bm25_results = self.store.search_fts(query, limit * 2)?;

        // 2. Vector search (if enabled and embedding provided in the future)
        // For now, we simulate vector search or trigger it if query is already an embedding.
        // In a real flow, a separate `search_vector` would be called with embedding.

        let mut results: Vec<HybridSearchResult> = bm25_results
            .into_iter()
            .enumerate()
            .map(|(i, r)| HybridSearchResult {
                document: r.document,
                rrf_score: self.config.bm25_weight / (self.config.rrf_k + i as f64 + 1.0),
                bm25_score: Some(r.score),
                vector_score: None,
                causal_efficiency: 1.0,
                latency_ms: 0.0,
                rank: i + 1,
            })
            .collect();

        // 3. Fusion Logic (Simplified RRF)
        // If vector results were present, we would merge them here.

        // Sort by RRF score
        results.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        // Re-rank
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = i + 1;
        }

        Ok(results)
    }

    /// Full Hybrid Search with Vector Embedding
    #[cfg(feature = "vector")]
    pub fn search_hybrid(
        &self,
        query: &str,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let bm25_results = self.store.search_fts(query, limit * 2)?;
        let vs = self.vector_store.as_ref().ok_or(EngramError::InvalidInput(
            "Vector store not initialized".into(),
        ))?;
        let vector_results = vs.search(embedding, limit * 2)?;

        let mut fused: std::collections::HashMap<String, HybridSearchResult> =
            std::collections::HashMap::new();

        // Score BM25
        for (i, r) in bm25_results.into_iter().enumerate() {
            let docid = format!("{}:{}", r.document.collection, r.document.path);
            let rrf = self.config.bm25_weight / (self.config.rrf_k + i as f64 + 1.0);

            // Map utility_score to causal_efficiency (normalized to 0.0-1.0 range roughly, or just raw)
            // For now we use the raw score as beta is 0.5
            let causal_efficiency = r.document.utility_score;

            fused.insert(
                docid,
                HybridSearchResult {
                    document: r.document,
                    rrf_score: rrf,
                    bm25_score: Some(r.score),
                    vector_score: None,
                    causal_efficiency,
                    latency_ms: 0.0,
                    rank: 0,
                },
            );
        }

        // Score Vector with Advanced Formula: Score = α·Similarity + β·Causal_Efficiency - γ·Latency
        // Alpha, Beta, Gamma are currently fixed weights for Phase 14
        let alpha = 1.0;
        let beta = 0.5;
        let gamma = 0.001;

        for (i, v) in vector_results.into_iter().enumerate() {
            let docid = format!("{}:{}", v.collection, v.path);

            // Fetch the document to get the utility_score
            let utility_score =
                if let Ok(Some(doc)) = self.store.get_by_path(&v.collection, &v.path) {
                    doc.utility_score
                } else {
                    v.causal_efficiency // fallback to vector's default
                };

            // Dynamic Scoring
            let adj_vector_score = alpha * (1.0 - v.score as f64) + beta * utility_score as f64
                - gamma * v.latency_ms as f64;
            let rrf = self.config.vector_weight / (self.config.rrf_k + i as f64 + 1.0);

            if let Some(existing) = fused.get_mut(&docid) {
                existing.rrf_score += rrf;
                existing.vector_score = Some(adj_vector_score as f32);
                existing.causal_efficiency = v.causal_efficiency;
                existing.latency_ms = v.latency_ms;
            } else {
                // Fetch document metadata if not in BM25 results
                if let Some(doc) = self.store.get_by_path(&v.collection, &v.path)? {
                    fused.insert(
                        docid,
                        HybridSearchResult {
                            document: doc,
                            rrf_score: rrf,
                            bm25_score: None,
                            vector_score: Some(adj_vector_score as f32),
                            causal_efficiency: v.causal_efficiency,
                            latency_ms: v.latency_ms,
                            rank: 0,
                        },
                    );
                }
            }
        }

        let mut results: Vec<HybridSearchResult> = fused.into_values().collect();
        results.sort_by(|a, b| b.rrf_score.partial_cmp(&a.rrf_score).unwrap());
        results.truncate(limit);

        for (i, r) in results.iter_mut().enumerate() {
            r.rank = i + 1;
        }

        Ok(results)
    }

    /// Search within a specific collection
    pub fn search_in_collection(
        &self,
        query: &str,
        collection: &str,
        limit: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let bm25_results = self
            .store
            .search_fts_in_collection(query, collection, limit)?;

        let results: Vec<HybridSearchResult> = bm25_results
            .into_iter()
            .enumerate()
            .map(|(i, r)| HybridSearchResult {
                document: r.document,
                rrf_score: self.config.bm25_weight / (self.config.rrf_k + i as f64 + 1.0),
                bm25_score: Some(r.score),
                vector_score: None,
                causal_efficiency: 1.0,
                latency_ms: 0.0,
                rank: i + 1,
            })
            .collect();

        Ok(results)
    }

    /// Search with path prefix filter
    pub fn search_with_path(
        &self,
        query: &str,
        path_prefix: &str,
        limit: usize,
    ) -> Result<Vec<HybridSearchResult>> {
        let bm25_results = self.store.search_fts_with_path(query, path_prefix, limit)?;

        let results: Vec<HybridSearchResult> = bm25_results
            .into_iter()
            .enumerate()
            .map(|(i, r)| HybridSearchResult {
                document: r.document,
                rrf_score: self.config.bm25_weight / (self.config.rrf_k + i as f64 + 1.0),
                bm25_score: Some(r.score),
                vector_score: None,
                causal_efficiency: 1.0,
                latency_ms: 0.0,
                rank: i + 1,
            })
            .collect();

        Ok(results)
    }

    /// Commit changes to persistent storage
    pub fn commit(&self) -> Result<()> {
        #[cfg(feature = "vector")]
        if let Some(vs) = &self.vector_store {
            let path = self.config.db_path.with_extension("vectors.bin");
            vs.save(&path)?;
        }
        Ok(())
    }

    /// Update summary for a document
    pub fn update_summary(&self, collection: &str, path: &str, summary: &str) -> Result<()> {
        self.store.update_summary(collection, path, summary)
    }

    /// Get a document by collection and path
    pub fn get_by_path(&self, collection: &str, path: &str) -> Result<Option<Document>> {
        self.store.get_by_path(collection, path)
    }

    /// Get statistics
    pub fn stats(&self) -> HybridSearchStats {
        let store_stats = self.store.get_stats().unwrap_or_default();
        HybridSearchStats {
            total_documents: store_stats.total_documents,
            #[cfg(feature = "vector")]
            total_vectors: self.vector_store.as_ref().map(|vs| vs.len()).unwrap_or(0),
            #[cfg(not(feature = "vector"))]
            total_vectors: 0,
            total_collections: store_stats.total_collections,
            database_path: self.config.db_path.display().to_string(),
        }
    }

    /// Save vector store to disk
    pub fn save_vectors(&self) -> Result<()> {
        self.commit()
    }

    /// Vacuum the database
    pub fn vacuum(&self) -> Result<()> {
        self.store.vacuum()
    }

    /// Access the underlying store
    pub fn engram_store(&self) -> Arc<EngramStore> {
        Arc::clone(&self.store)
    }

    /// Delete stale sessions
    pub fn delete_stale_sessions(&self, max_age_days: u32) -> Result<usize> {
        self.store.delete_stale_sessions(max_age_days)
    }

    // ============ Unverified Document Management (Phase 12-B) ============

    /// List all unverified documents
    pub fn list_unverified(&self, limit: usize) -> Result<Vec<Document>> {
        self.store.list_unverified(limit)
    }

    /// Mark a document as verified
    pub fn mark_verified(&self, collection: &str, path: &str) -> Result<()> {
        self.store.mark_verified(collection, path)
    }

    /// Mark (delete) a document as pruned
    pub fn delete_document(&self, collection: &str, path: &str) -> Result<()> {
        self.store.delete_document(collection, path)
    }
}
