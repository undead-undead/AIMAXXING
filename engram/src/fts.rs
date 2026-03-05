//! Pure Rust BM25 full-text search engine
//!
//! Provides inverted index and BM25 scoring without SQLite FTS5.
//! Index data is persisted in Engram-KV tables.

use crate::error::Result;
use crate::kv::EngramKV;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use jieba_rs::Jieba;
use once_cell::sync::Lazy;

static JIEBA: Lazy<Jieba> = Lazy::new(Jieba::new);

/// Term frequency entry for a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermFrequency {
    pub doc_key: String,
    pub term: String,
    pub count: u32,
    pub doc_length: u32,
}

/// Posting list entry (term -> list of documents containing it)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingList {
    pub term: String,
    pub entries: Vec<PostingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingEntry {
    pub doc_key: String,
    pub term_frequency: u32,
    pub doc_length: u32,
}

/// BM25 scoring parameters
pub struct Bm25Config {
    pub k1: f64,
    pub b: f64,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }
}

/// BM25 search result
#[derive(Debug, Clone)]
pub struct FtsResult {
    pub doc_key: String,
    pub score: f64,
}

/// Full-text search engine using BM25 on Engram-KV
pub struct FtsEngine {
    kv: Arc<EngramKV>,
    config: Bm25Config,
}

impl FtsEngine {
    pub fn new(kv: Arc<EngramKV>) -> Self {
        Self {
            kv,
            config: Bm25Config::default(),
        }
    }

    /// Tokenize text into terms. Supports CJK characters via Jieba segmentation.
    pub fn tokenize(text: &str) -> Vec<String> {
        let text_lower = text.to_lowercase();

        // Use Jieba for segmentation
        let raw_tokens = JIEBA.cut(&text_lower, false);

        raw_tokens
            .into_iter()
            .filter_map(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return None;
                }

                // Keep CJK characters even if length is 1 (often meaningful words)
                // Filter out single-character non-alphanumeric/non-CJK tokens
                let first_char = trimmed.chars().next().unwrap_or(' ');
                let is_cjk = ('\u{4e00}'..='\u{9fff}').contains(&first_char);
                let is_alphanum = first_char.is_alphanumeric();

                if (is_cjk && is_alphanum) || trimmed.len() >= 2 {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Index a document's text
    pub fn index_document(&self, doc_key: &str, text: &str) -> Result<()> {
        let terms = Self::tokenize(text);
        let doc_length = terms.len() as u32;

        // Count term frequencies
        let mut tf_map: HashMap<String, u32> = HashMap::new();
        for term in &terms {
            *tf_map.entry(term.clone()).or_insert(0) += 1;
        }

        // Store forward index (doc -> term frequencies)
        let forward_data = bincode::serialize(&tf_map)
            .map_err(|e| crate::error::EngramError::Serialization(e.to_string()))?;
        self.kv.put_fts_forward(doc_key, &forward_data)?;

        // Update inverted index (term -> posting list)
        for (term, count) in &tf_map {
            let mut posting_list = self.get_posting_list(term)?;
            // Remove old entry for this doc if exists
            posting_list.entries.retain(|e| e.doc_key != doc_key);
            posting_list.entries.push(PostingEntry {
                doc_key: doc_key.to_string(),
                term_frequency: *count,
                doc_length,
            });
            let data = bincode::serialize(&posting_list)
                .map_err(|e| crate::error::EngramError::Serialization(e.to_string()))?;
            self.kv.put_fts_inverted(term, &data)?;
        }

        Ok(())
    }

    /// Get posting list for a term
    fn get_posting_list(&self, term: &str) -> Result<PostingList> {
        match self.kv.get_fts_inverted(term)? {
            Some(data) => {
                let list: PostingList = bincode::deserialize(&data)
                    .map_err(|e| crate::error::EngramError::Serialization(e.to_string()))?;
                Ok(list)
            }
            None => Ok(PostingList {
                term: term.to_string(),
                entries: Vec::new(),
            }),
        }
    }

    /// Search using BM25 scoring
    pub fn search(&self, query: &str, total_docs: u64, limit: usize) -> Result<Vec<FtsResult>> {
        let query_terms = Self::tokenize(query);
        let mut doc_scores: HashMap<String, f64> = HashMap::new();

        // Calculate average document length (approximate)
        let avg_dl = 100.0_f64; // TODO: track this dynamically

        for term in &query_terms {
            let posting_list = self.get_posting_list(term)?;
            let df = posting_list.entries.len() as f64;
            let idf = ((total_docs as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

            for entry in &posting_list.entries {
                let tf = entry.term_frequency as f64;
                let dl = entry.doc_length as f64;
                let numerator = tf * (self.config.k1 + 1.0);
                let denominator =
                    tf + self.config.k1 * (1.0 - self.config.b + self.config.b * dl / avg_dl);
                let score = idf * numerator / denominator;
                *doc_scores.entry(entry.doc_key.clone()).or_insert(0.0) += score;
            }
        }

        let mut results: Vec<FtsResult> = doc_scores
            .into_iter()
            .map(|(doc_key, score)| FtsResult { doc_key, score })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        Ok(results)
    }

    /// Delete a document from the FTS index
    pub fn delete_document(&self, doc_key: &str) -> Result<()> {
        // 1. Get forward index to find terms to remove from inverted index
        if let Some(forward_data) = self.kv.get_fts_forward(doc_key)? {
            let tf_map: HashMap<String, u32> = bincode::deserialize(&forward_data)
                .map_err(|e| crate::error::EngramError::Serialization(e.to_string()))?;

            // 2. Remove entry from each term's posting list
            for (term, _) in tf_map {
                let mut posting_list = self.get_posting_list(&term)?;
                posting_list.entries.retain(|e| e.doc_key != doc_key);

                if posting_list.entries.is_empty() {
                    self.kv.delete_fts_inverted(&term)?;
                } else {
                    let data = bincode::serialize(&posting_list)
                        .map_err(|e| crate::error::EngramError::Serialization(e.to_string()))?;
                    self.kv.put_fts_inverted(&term, &data)?;
                }
            }
        }

        // 3. Delete forward index
        self.kv.delete_fts_forward(doc_key)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fts_tokenize_cjk() {
        let text = "AIMAXXING是一个强大的AI代理框架";
        let tokens = FtsEngine::tokenize(text);

        assert!(tokens.contains(&"aimaxxing".to_string()));
        assert!(tokens.contains(&"强大".to_string()));
        assert!(tokens.contains(&"代理".to_string()));
        assert!(tokens.contains(&"框架".to_string()));
    }

    #[test]
    fn test_fts_tokenize_english() {
        let text = "The quick brown fox";
        let tokens = FtsEngine::tokenize(text);
        assert_eq!(tokens.len(), 4);
        assert!(tokens.contains(&"fox".to_string()));
    }
}
