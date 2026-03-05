//! Reciprocal Rank Fusion (RRF) algorithm for hybrid search
//!
//! Combines multiple ranked lists of search results into a single unified ranking.
//! Used to merge BM25 (keyword-based) and vector (semantic) search results.

use std::collections::HashMap;

/// RRF configuration parameters
#[derive(Debug, Clone)]
pub struct RrfConfig {
    /// RRF constant (k parameter, typically 60)
    /// Higher values reduce the impact of high-ranking items
    pub k: usize,
    /// Weight for BM25 results (default: 2.0 - keyword search is more precise)
    pub bm25_weight: f64,
    /// Weight for vector results (default: 1.0 - semantic search is more recall-oriented)
    pub vector_weight: f64,
}

impl Default for RrfConfig {
    fn default() -> Self {
        Self {
            k: 60,
            bm25_weight: 2.0,
            vector_weight: 1.0,
        }
    }
}

/// Result from fusion
#[derive(Debug, Clone)]
pub struct FusedResult {
    /// Document ID
    pub docid: String,
    /// Combined RRF score
    pub rrf_score: f64,
    /// Original BM25 rank (if present)
    pub bm25_rank: Option<usize>,
    /// Original vector rank (if present)
    pub vector_rank: Option<usize>,
    /// Original BM25 score (if present)
    pub bm25_score: Option<f64>,
    /// Original vector score (if present)
    pub vector_score: Option<f64>,
}

/// Reciprocal Rank Fusion implementation
pub struct RrfFusion {
    config: RrfConfig,
}

impl RrfFusion {
    /// Create a new RRF fusion with default configuration
    pub fn new() -> Self {
        Self::with_config(RrfConfig::default())
    }

    /// Create RRF fusion with custom configuration
    pub fn with_config(config: RrfConfig) -> Self {
        Self { config }
    }

    /// Fuse BM25 and vector search results
    ///
    /// # Arguments
    ///
    /// * `bm25_results` - Results from BM25 search (ordered by relevance)
    /// * `vector_results` - Results from vector search (ordered by similarity)
    ///
    /// # Returns
    ///
    /// Fused results ordered by combined RRF score (highest first)
    ///
    /// # Examples
    ///
    /// ```
    /// use aimaxxing_engram::RrfFusion;
    ///
    /// let fusion = RrfFusion::new();
    ///
    /// let bm25_results = vec![
    ///     ("doc1".to_string(), 10.5),
    ///     ("doc2".to_string(), 8.2),
    /// ];
    ///
    /// let vector_results = vec![
    ///     ("doc3".to_string(), 0.95),
    ///     ("doc1".to_string(), 0.88),
    /// ];
    ///
    /// let fused = fusion.fuse(&bm25_results, &vector_results);
    ///
    /// // doc1 appears in both lists, so it gets highest combined score
    /// assert_eq!(fused[0].docid, "doc1");
    /// ```
    pub fn fuse(
        &self,
        bm25_results: &[(String, f64)],   // (docid, score)
        vector_results: &[(String, f64)], // (docid, score)
    ) -> Vec<FusedResult> {
        let mut scores: HashMap<String, FusedResultBuilder> = HashMap::new();

        // Process BM25 results
        for (rank, (docid, score)) in bm25_results.iter().enumerate() {
            let rrf_score = self.config.bm25_weight / (self.config.k + rank + 1) as f64;

            scores
                .entry(docid.clone())
                .or_insert_with(|| FusedResultBuilder::new(docid.clone()))
                .add_bm25(rank, *score, rrf_score);
        }

        // Process vector results
        for (rank, (docid, score)) in vector_results.iter().enumerate() {
            let rrf_score = self.config.vector_weight / (self.config.k + rank + 1) as f64;

            scores
                .entry(docid.clone())
                .or_insert_with(|| FusedResultBuilder::new(docid.clone()))
                .add_vector(rank, *score, rrf_score);
        }

        // Convert to results and sort
        let mut results: Vec<FusedResult> = scores.into_values().map(|b| b.build()).collect();

        results.sort_by(|a, b| {
            b.rrf_score
                .partial_cmp(&a.rrf_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }

    /// Fuse with custom weights for this specific query
    pub fn fuse_weighted(
        &self,
        bm25_results: &[(String, f64)],
        vector_results: &[(String, f64)],
        bm25_weight: f64,
        vector_weight: f64,
    ) -> Vec<FusedResult> {
        let custom_config = RrfConfig {
            k: self.config.k,
            bm25_weight,
            vector_weight,
        };

        let fusion = RrfFusion::with_config(custom_config);
        fusion.fuse(bm25_results, vector_results)
    }
}

impl Default for RrfFusion {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for FusedResult
struct FusedResultBuilder {
    docid: String,
    rrf_score: f64,
    bm25_rank: Option<usize>,
    vector_rank: Option<usize>,
    bm25_score: Option<f64>,
    vector_score: Option<f64>,
}

impl FusedResultBuilder {
    fn new(docid: String) -> Self {
        Self {
            docid,
            rrf_score: 0.0,
            bm25_rank: None,
            vector_rank: None,
            bm25_score: None,
            vector_score: None,
        }
    }

    fn add_bm25(&mut self, rank: usize, score: f64, rrf_contribution: f64) {
        self.bm25_rank = Some(rank);
        self.bm25_score = Some(score);
        self.rrf_score += rrf_contribution;
    }

    fn add_vector(&mut self, rank: usize, score: f64, rrf_contribution: f64) {
        self.vector_rank = Some(rank);
        self.vector_score = Some(score);
        self.rrf_score += rrf_contribution;
    }

    fn build(self) -> FusedResult {
        FusedResult {
            docid: self.docid,
            rrf_score: self.rrf_score,
            bm25_rank: self.bm25_rank,
            vector_rank: self.vector_rank,
            bm25_score: self.bm25_score,
            vector_score: self.vector_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_basic() {
        let fusion = RrfFusion::new();

        let bm25 = vec![
            ("doc1".to_string(), 10.0),
            ("doc2".to_string(), 8.0),
            ("doc3".to_string(), 6.0),
        ];

        let vector = vec![
            ("doc3".to_string(), 0.95),
            ("doc1".to_string(), 0.88),
            ("doc4".to_string(), 0.75),
        ];

        let results = fusion.fuse(&bm25, &vector);

        // doc1 appears in both (rank 0 in BM25, rank 1 in vector)
        // doc3 appears in both (rank 2 in BM25, rank 0 in vector)
        // Should have highest combined scores

        assert!(results.len() >= 3);

        // doc1 or doc3 should be first (both appear in both lists)
        assert!(results[0].docid == "doc1" || results[0].docid == "doc3");

        // Check that appearing in both lists increases score
        let doc1 = results.iter().find(|r| r.docid == "doc1").unwrap();
        assert!(doc1.bm25_rank.is_some());
        assert!(doc1.vector_rank.is_some());
    }

    #[test]
    fn test_rrf_bm25_only() {
        let fusion = RrfFusion::new();

        let bm25 = vec![("doc1".to_string(), 10.0), ("doc2".to_string(), 8.0)];

        let vector = vec![];

        let results = fusion.fuse(&bm25, &vector);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].docid, "doc1");
        assert!(results[0].bm25_rank.is_some());
        assert!(results[0].vector_rank.is_none());
    }

    #[test]
    fn test_rrf_vector_only() {
        let fusion = RrfFusion::new();

        let bm25 = vec![];

        let vector = vec![("doc1".to_string(), 0.95), ("doc2".to_string(), 0.88)];

        let results = fusion.fuse(&bm25, &vector);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].docid, "doc1");
        assert!(results[0].bm25_rank.is_none());
        assert!(results[0].vector_rank.is_some());
    }

    #[test]
    fn test_rrf_empty() {
        let fusion = RrfFusion::new();
        let results = fusion.fuse(&[], &[]);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_rrf_custom_weights() {
        let fusion = RrfFusion::new();

        let bm25 = vec![("doc1".to_string(), 10.0)];
        let vector = vec![("doc2".to_string(), 0.95)];

        // Equal weights
        let _results_equal = fusion.fuse_weighted(&bm25, &vector, 1.0, 1.0);

        // Favor BM25
        let results_bm25 = fusion.fuse_weighted(&bm25, &vector, 10.0, 1.0);

        // With equal weights, order might vary
        // With BM25 weight 10x higher, doc1 should be first
        assert_eq!(results_bm25[0].docid, "doc1");
    }

    #[test]
    fn test_rrf_ranking_formula() {
        let config = RrfConfig {
            k: 60,
            bm25_weight: 1.0,
            vector_weight: 1.0,
        };

        let fusion = RrfFusion::with_config(config);

        let bm25 = vec![("doc1".to_string(), 10.0)];
        let vector = vec![("doc1".to_string(), 0.95)];

        let results = fusion.fuse(&bm25, &vector);

        // doc1 is rank 0 in both
        // RRF score = 1/(60+0+1) + 1/(60+0+1) = 2/61
        let expected_score = 2.0 / 61.0;

        assert_eq!(results[0].docid, "doc1");
        assert!((results[0].rrf_score - expected_score).abs() < 1e-10);
    }

    #[test]
    fn test_preserve_original_scores() {
        let fusion = RrfFusion::new();

        let bm25 = vec![("doc1".to_string(), 10.5)];
        let vector = vec![("doc1".to_string(), 0.88)];

        let results = fusion.fuse(&bm25, &vector);

        assert_eq!(results[0].bm25_score, Some(10.5));
        assert_eq!(results[0].vector_score, Some(0.88));
        assert_eq!(results[0].bm25_rank, Some(0));
        assert_eq!(results[0].vector_rank, Some(0));
    }

    #[test]
    fn test_realistic_scenario() {
        let fusion = RrfFusion::new();

        // BM25 found exact keyword matches
        let bm25 = vec![
            ("exact_match".to_string(), 15.0),
            ("partial_match".to_string(), 8.0),
            ("weak_match".to_string(), 2.0),
        ];

        // Vector found semantic matches
        let vector = vec![
            ("semantic_similar".to_string(), 0.92),
            ("exact_match".to_string(), 0.85), // Also in BM25
            ("related_concept".to_string(), 0.78),
        ];

        let results = fusion.fuse(&bm25, &vector);

        // "exact_match" should rank high (appears in both)
        let exact_match_result = results.iter().find(|r| r.docid == "exact_match").unwrap();

        assert!(exact_match_result.bm25_rank.is_some());
        assert!(exact_match_result.vector_rank.is_some());

        // Combined score should be higher than items in only one list
        let semantic_only = results
            .iter()
            .find(|r| r.docid == "semantic_similar")
            .unwrap();

        assert!(exact_match_result.rrf_score > semantic_only.rrf_score);
    }
}
