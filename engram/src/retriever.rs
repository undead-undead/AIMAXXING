//! Recursive Hierarchical Retrieval
//!
//! Implements multi-level retrieval that traverses context levels (L0->L1->L2).

use crate::error::Result;
use crate::hybrid_search::{HybridSearchEngine, HybridSearchResult};
use crate::intent::IntentAnalyzer;
use std::sync::Arc;
use std::collections::HashSet;
use tracing::{info, debug};

/// Retriever that performs recursive/hierarchical search
pub struct HierarchicalRetriever {
    engine: Arc<HybridSearchEngine>,
    analyzer: IntentAnalyzer,
}

impl HierarchicalRetriever {
    pub fn new(engine: Arc<HybridSearchEngine>) -> Self {
        Self {
            engine,
            analyzer: IntentAnalyzer::new(),
        }
    }

    /// Perform a recursive search based on intent analysis
    pub async fn search_recursive(&self, query: &str, limit: usize) -> Result<Vec<HybridSearchResult>> {
        info!("Starting hierarchical recursive search: '{}'", query);

        let _plan = self.analyzer.analyze(query).await?;

        // Broad hybrid search
        let raw_results = self.engine.search(query, limit * 3)?;

        let mut final_results = Vec::new();
        let mut seen_docids = HashSet::new();

        // Identify high-scoring structural landmarks and drill down
        let mut candidate_paths = Vec::new();
        for res in &raw_results {
            if res.rrf_score > 0.3 {
                if res.document.abstract_content.is_some() || res.document.overview_content.is_some() {
                    info!("Identified landmark: {}/{}", res.document.collection, res.document.path);
                    candidate_paths.push((res.document.collection.clone(), res.document.path.clone()));
                }
            }
        }

        // Drill down into landmarks
        if !candidate_paths.is_empty() {
            debug!("Drilling down into {} landmarks", candidate_paths.len());
            for (col, path) in candidate_paths.iter().take(3) {
                let prefix = format!("{}:{}", col, path);
                if let Ok(results) = self.engine.search_with_path(query, &prefix, 5) {
                    for mut res in results {
                        res.rrf_score *= 1.2;
                        if seen_docids.insert(res.document.docid.clone()) {
                            final_results.push(res);
                        }
                    }
                }
            }
        }

        // Inject remaining raw results
        for res in raw_results {
            if seen_docids.insert(res.document.docid.clone()) {
                final_results.push(res);
            }
        }

        final_results.sort_by(|a, b| b.rrf_score.partial_cmp(&a.rrf_score).unwrap_or(std::cmp::Ordering::Equal));
        final_results.truncate(limit);

        debug!("Hierarchical search: {} results", final_results.len());
        Ok(final_results)
    }
}
