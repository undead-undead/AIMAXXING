use crate::error::Result;
use crate::hybrid_search::HybridSearchResult;

/// Reranker Trait: Precision scoring for retrieved documents
///
/// This trait allows plugging in different reranking backends (e.g., Local Candle Models,
/// Cloud APIs) to improve the precision of the initial BM25/Vector retrieval.
pub trait Reranker: Send + Sync {
    /// Re-score and re-order the given documents based on the query.
    /// Returns the re-ranked list of results.
    fn rerank(
        &self,
        query: &str,
        documents: Vec<HybridSearchResult>,
    ) -> Result<Vec<HybridSearchResult>>;
}

/// A No-Op Reranker that performs no operations.
///
/// Used as a safe fallback when no model is available, ensuring the system
/// degrades gracefully to the coarse retrieval scores (e.g., RRF).
pub struct NoOpReranker;

impl NoOpReranker {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoOpReranker {
    fn default() -> Self {
        Self::new()
    }
}

impl Reranker for NoOpReranker {
    fn rerank(
        &self,
        _query: &str,
        documents: Vec<HybridSearchResult>,
    ) -> Result<Vec<HybridSearchResult>> {
        // Return documents exactly as they were provided by the coarse ranker
        Ok(documents)
    }
}
