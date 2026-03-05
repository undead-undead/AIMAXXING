//! EngramStore: High-level document storage API
//!
//! Built on top of Engram-KV, providing document CRUD, collection management,
//! content-addressable storage, session persistence, and BM25 full-text search.

use crate::content_hash::{get_docid, hash_content};
use crate::error::{EngramError, Result};
use crate::fts::FtsEngine;
use crate::kv::EngramKV;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub docid: String,
    pub collection: String,
    pub path: String,
    pub title: String,
    pub body: Option<String>,
    pub summary: Option<String>,
    pub abstract_content: Option<String>,
    pub overview_content: Option<String>,
    pub content_hash: String,
    pub created_at: String,
    pub updated_at: String,
    /// Phase 12-B: Whether this document is unverified (quarantined)
    pub unverified: bool,
    /// Phase 14: Causal Efficiency score (utility-based ranking)
    pub utility_score: f32,
}

/// Search result with score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub document: Document,
    pub score: f64,
    pub snippet: Option<String>,
}

/// Collection metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub description: Option<String>,
    pub glob_pattern: String,
    pub root_path: Option<String>,
}

/// Store statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreStats {
    pub total_documents: u64,
    pub total_content_blobs: u64,
    pub total_collections: usize,
}

const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024; // 10MB limit

/// EngramStore - Core storage engine built on Engram-KV
pub struct EngramStore {
    kv: Arc<EngramKV>,
}

impl EngramStore {
    /// Create or open an EngramStore at the given path
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self> {
        let kv = EngramKV::open(db_path)?;
        Ok(Self { kv: Arc::new(kv) })
    }

    /// Get the underlying KV engine
    pub fn kv(&self) -> &EngramKV {
        &self.kv
    }

    /// Get shared reference to KV engine
    pub fn kv_arc(&self) -> Arc<EngramKV> {
        Arc::clone(&self.kv)
    }

    /// Store a document with content-addressable storage
    pub fn store_document(
        &self,
        collection: &str,
        path: &str,
        title: &str,
        body: &str,
        unverified: bool,
    ) -> Result<Document> {
        // Validate content size
        if body.len() > MAX_CONTENT_SIZE {
            return Err(EngramError::ContentTooLarge {
                size: body.len(),
                max: MAX_CONTENT_SIZE,
            });
        }

        let content_hash = hash_content(body);
        let docid = get_docid(&content_hash);
        let now = Utc::now().to_rfc3339();

        // Store content blob (deduplicated by hash)
        self.kv.put_content(&content_hash, body.as_bytes())?;

        let doc = Document {
            docid: docid.clone(),
            collection: collection.to_string(),
            path: path.to_string(),
            title: title.to_string(),
            body: Some(body.to_string()),
            summary: None,
            abstract_content: None,
            overview_content: None,
            content_hash,
            created_at: now.clone(),
            updated_at: now,
            unverified,
            utility_score: 0.0,
        };

        // Serialize and store document metadata
        let doc_key = format!("{}:{}", collection, path);
        let data =
            bincode::serialize(&doc).map_err(|e| EngramError::Serialization(e.to_string()))?;
        self.kv.put_document(&doc_key, &data)?;

        // Index in FTS engine
        let fts = FtsEngine::new(self.kv.clone());
        fts.index_document(&doc_key, body)?;

        debug!("Stored document: {} (docid: {})", path, docid);
        Ok(doc)
    }

    /// Get document by virtual path
    pub fn get_by_path(&self, collection: &str, path: &str) -> Result<Option<Document>> {
        let doc_key = format!("{}:{}", collection, path);
        match self.kv.get_document(&doc_key)? {
            Some(data) => {
                let doc: Document = bincode::deserialize(&data)
                    .map_err(|e| EngramError::Serialization(e.to_string()))?;
                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    /// Get document by docid (short hash)
    pub fn get_by_docid(&self, docid: &str) -> Result<Option<Document>> {
        let all_docs = self.kv.iter_documents()?;
        for (_key, data) in all_docs {
            let doc: Document = bincode::deserialize(&data)
                .map_err(|e| EngramError::Serialization(e.to_string()))?;
            if doc.docid == docid {
                return Ok(Some(doc));
            }
        }
        Ok(None)
    }

    /// BM25 full-text search
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let fts = FtsEngine::new(self.kv.clone());
        let total_docs = self.kv.document_count()?;
        let fts_results = fts.search(query, total_docs, limit)?;

        let mut results = Vec::new();
        for res in fts_results {
            if let Some(doc) = self.get_by_doc_key(&res.doc_key)? {
                results.push(SearchResult {
                    document: doc,
                    score: res.score,
                    snippet: None, // Snippet generation can be added later
                });
            }
        }

        Ok(results)
    }

    /// Helper to get document by internal doc_key
    fn get_by_doc_key(&self, doc_key: &str) -> Result<Option<Document>> {
        match self.kv.get_document(doc_key)? {
            Some(data) => {
                let doc: Document = bincode::deserialize(&data)
                    .map_err(|e| EngramError::Serialization(e.to_string()))?;
                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    /// Search within a specific collection
    pub fn search_fts_in_collection(
        &self,
        query: &str,
        collection: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let mut results = self.search_fts(query, limit * 2)?;
        results.retain(|r| r.document.collection == collection);
        results.truncate(limit);
        Ok(results)
    }

    /// Search with path prefix filter
    pub fn search_fts_with_path(
        &self,
        query: &str,
        path_prefix: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let mut results = self.search_fts(query, limit * 2)?;
        results.retain(|r| r.document.path.starts_with(path_prefix));
        results.truncate(limit);
        Ok(results)
    }

    /// Create a collection
    pub fn create_collection(&self, collection: Collection) -> Result<()> {
        let data = bincode::serialize(&collection)
            .map_err(|e| EngramError::Serialization(e.to_string()))?;
        self.kv.put_collection(&collection.name, &data)?;
        info!("Created collection: {}", collection.name);
        Ok(())
    }

    /// List all collections
    pub fn list_collections(&self) -> Result<Vec<Collection>> {
        let raw = self.kv.list_collections()?;
        let mut collections = Vec::new();
        for (_name, data) in raw {
            let col: Collection = bincode::deserialize(&data)
                .map_err(|e| EngramError::Serialization(e.to_string()))?;
            collections.push(col);
        }
        Ok(collections)
    }

    /// Get index statistics
    pub fn get_stats(&self) -> Result<StoreStats> {
        Ok(StoreStats {
            total_documents: self.kv.document_count()?,
            total_content_blobs: self.kv.content_count()?,
            total_collections: self.kv.list_collections()?.len(),
        })
    }

    /// Compact database (reclaim space)
    pub fn vacuum(&self) -> Result<()> {
        self.kv.compact()
    }

    /// Update the summary for a document
    pub fn update_summary(&self, collection: &str, path: &str, summary: &str) -> Result<()> {
        if let Some(mut doc) = self.get_by_path(collection, path)? {
            doc.summary = Some(summary.to_string());
            doc.updated_at = Utc::now().to_rfc3339();
            let doc_key = format!("{}:{}", collection, path);
            let data =
                bincode::serialize(&doc).map_err(|e| EngramError::Serialization(e.to_string()))?;
            self.kv.put_document(&doc_key, &data)?;
        }
        Ok(())
    }

    /// Update tiered context for a document
    pub fn update_tiered_context(
        &self,
        collection: &str,
        path: &str,
        abstract_content: &str,
        overview_content: &str,
    ) -> Result<()> {
        if let Some(mut doc) = self.get_by_path(collection, path)? {
            doc.abstract_content = Some(abstract_content.to_string());
            doc.overview_content = Some(overview_content.to_string());
            doc.updated_at = Utc::now().to_rfc3339();
            let doc_key = format!("{}:{}", collection, path);
            let data =
                bincode::serialize(&doc).map_err(|e| EngramError::Serialization(e.to_string()))?;
            self.kv.put_document(&doc_key, &data)?;
        }
        Ok(())
    }

    // ============ Session Persistence ============

    /// Store a session
    pub fn store_session(&self, id: &str, data: &str) -> Result<()> {
        self.kv.put_session(id, data)
    }

    /// Retrieve a session
    pub fn get_session(&self, id: &str) -> Result<Option<String>> {
        self.kv.get_session(id)
    }

    /// Delete stale sessions (stub - sessions don't have timestamps in KV yet)
    pub fn delete_stale_sessions(&self, _max_age_days: u32) -> Result<usize> {
        // TODO: Implement session TTL tracking
        Ok(0)
    }

    /// Vacuum content (stub for compatibility)
    pub fn vacuum_content(&self) -> Result<usize> {
        // In Engram-KV, orphaned content is handled by compaction
        Ok(0)
    }

    // ============ Unverified Document Management (Phase 12-B) ============

    /// List all unverified documents across all collections
    pub fn list_unverified(&self, limit: usize) -> Result<Vec<Document>> {
        let mut results = Vec::new();
        let all_docs = self.kv.iter_documents()?;
        for (_key, data) in all_docs {
            let doc: Document = bincode::deserialize(&data)
                .map_err(|e| EngramError::Serialization(e.to_string()))?;
            if doc.unverified {
                results.push(doc);
                if results.len() >= limit {
                    break;
                }
            }
        }
        Ok(results)
    }

    /// Mark a document as verified
    pub fn mark_verified(&self, collection: &str, path: &str) -> Result<()> {
        if let Some(mut doc) = self.get_by_path(collection, path)? {
            doc.unverified = false;
            doc.updated_at = Utc::now().to_rfc3339();
            let doc_key = format!("{}:{}", collection, path);
            let data =
                bincode::serialize(&doc).map_err(|e| EngramError::Serialization(e.to_string()))?;
            self.kv.put_document(&doc_key, &data)?;
        }
        Ok(())
    }

    /// Mark (delete) a document as pruned
    pub fn delete_document(&self, collection: &str, path: &str) -> Result<()> {
        let doc_key = format!("{}:{}", collection, path);
        self.kv.delete_document(&doc_key)?;
        // FTS cleanup
        let fts = FtsEngine::new(self.kv.clone());
        fts.delete_document(&doc_key)?;
        Ok(())
    }

    /// Update the utility score of a document
    pub fn update_utility(&self, docid: &str, increment: f32) -> Result<()> {
        let data = self
            .kv
            .get_document(docid)?
            .ok_or_else(|| EngramError::Internal(format!("Document not found: {}", docid)))?;

        let mut doc: Document =
            bincode::deserialize(&data).map_err(|e| EngramError::Serialization(e.to_string()))?;

        doc.utility_score += increment;
        doc.updated_at = Utc::now().to_rfc3339();

        let updated_data =
            bincode::serialize(&doc).map_err(|e| EngramError::Serialization(e.to_string()))?;
        self.kv.put_document(&doc.docid, &updated_data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_retrieve_document() {
        let dir = tempfile::tempdir().unwrap();
        let store = EngramStore::new(dir.path().join("test.aimaxxing_engram")).unwrap();

        let doc = store
            .store_document(
                "trading",
                "sol.md",
                "SOL Strategy",
                "Buy low sell high",
                false,
            )
            .unwrap();
        assert!(!doc.docid.is_empty());

        let retrieved = store.get_by_path("trading", "sol.md").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "SOL Strategy");
    }

    #[test]
    fn test_search_fts() {
        let dir = tempfile::tempdir().unwrap();
        let store = EngramStore::new(dir.path().join("test.aimaxxing_engram")).unwrap();

        store
            .store_document(
                "trading",
                "sol.md",
                "SOL Strategy",
                "Buy SOL when RSI < 30",
                false,
            )
            .unwrap();
        store
            .store_document(
                "trading",
                "btc.md",
                "BTC Analysis",
                "Bitcoin dominance rising",
                false,
            )
            .unwrap();

        let results = store.search_fts("SOL RSI", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].document.title, "SOL Strategy");
    }

    #[test]
    fn test_collections() {
        let dir = tempfile::tempdir().unwrap();
        let store = EngramStore::new(dir.path().join("test.aimaxxing_engram")).unwrap();

        store
            .create_collection(Collection {
                name: "notes".to_string(),
                description: Some("Personal notes".to_string()),
                glob_pattern: "**/*.md".to_string(),
                root_path: None,
            })
            .unwrap();

        let cols = store.list_collections().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, "notes");
    }
}
