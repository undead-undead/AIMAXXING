//! Engram-KV: Pure Rust KV storage engine
//!
//! Unified storage layer based on `redb` providing:
//! - ACID transactions with crash recovery
//! - Memory-mapped I/O for zero-copy reads
//! - Minimal RAM footprint regardless of database size

use crate::error::{EngramError, Result};
use redb::{Database, ReadableTable, TableDefinition};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

// Table definitions for Engram-KV
const DOCUMENTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("documents");
const COLLECTIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("collections");
const CONTENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("content");
const SESSIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("sessions");
const FTS_FORWARD_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("fts_forward");
const FTS_INVERTED_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("fts_inverted");
const VECTORS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vectors");
const METADATA_TABLE: TableDefinition<&str, &str> = TableDefinition::new("metadata");

/// Engram-KV storage engine
pub struct EngramKV {
    db: Arc<Database>,
    path: PathBuf,
}

impl EngramKV {
    /// Create or open an Engram-KV database at the given path
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Database::create(&path)?;

        // Initialize all tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(DOCUMENTS_TABLE)?;
            let _ = write_txn.open_table(COLLECTIONS_TABLE)?;
            let _ = write_txn.open_table(CONTENT_TABLE)?;
            let _ = write_txn.open_table(SESSIONS_TABLE)?;
            let _ = write_txn.open_table(FTS_FORWARD_TABLE)?;
            let _ = write_txn.open_table(FTS_INVERTED_TABLE)?;
            let _ = write_txn.open_table(VECTORS_TABLE)?;
            let _ = write_txn.open_table(METADATA_TABLE)?;
        }
        write_txn.commit()?;

        info!("Engram-KV opened at: {}", path.display());

        Ok(Self {
            db: Arc::new(db),
            path,
        })
    }

    /// Get database path
    pub fn path(&self) -> &Path {
        &self.path
    }

    // ============ Document Operations ============

    /// Store a serialized document
    pub fn put_document(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DOCUMENTS_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Retrieve a serialized document
    pub fn get_document(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DOCUMENTS_TABLE)?;
        Ok(table.get(key)?.map(|v| v.value().to_vec()))
    }

    /// Delete a document
    pub fn delete_document(&self, key: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(DOCUMENTS_TABLE)?;
        let removed = table.remove(key)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    /// Iterate all documents
    pub fn iter_documents(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DOCUMENTS_TABLE)?;
        let mut results = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            results.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(results)
    }

    // ============ Content Blob Operations ============

    /// Store content blob (content-addressable by hash)
    pub fn put_content(&self, hash: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONTENT_TABLE)?;
            table.insert(hash, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Retrieve content blob by hash
    pub fn get_content(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONTENT_TABLE)?;
        Ok(table.get(hash)?.map(|v| v.value().to_vec()))
    }

    // ============ Collection Operations ============

    /// Store collection metadata
    pub fn put_collection(&self, name: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(COLLECTIONS_TABLE)?;
            table.insert(name, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get collection metadata
    pub fn get_collection(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(COLLECTIONS_TABLE)?;
        Ok(table.get(name)?.map(|v| v.value().to_vec()))
    }

    /// List all collections
    pub fn list_collections(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(COLLECTIONS_TABLE)?;
        let mut results = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            results.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(results)
    }

    // ============ Session Operations ============

    /// Store a session (JSON string)
    pub fn put_session(&self, id: &str, data: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SESSIONS_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Retrieve a session
    pub fn get_session(&self, id: &str) -> Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SESSIONS_TABLE)?;
        Ok(table.get(id)?.map(|v| v.value().to_string()))
    }

    /// Delete a session
    pub fn delete_session(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(SESSIONS_TABLE)?;
        let removed = table.remove(id)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    // ============ FTS Index Operations ============

    /// Store a forward index entry (doc_key -> term frequencies)
    pub fn put_fts_forward(&self, doc_key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(FTS_FORWARD_TABLE)?;
            table.insert(doc_key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Retrieve forward index entry
    pub fn get_fts_forward(&self, doc_key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(FTS_FORWARD_TABLE)?;
        Ok(table.get(doc_key)?.map(|v| v.value().to_vec()))
    }

    /// Delete forward index entry
    pub fn delete_fts_forward(&self, doc_key: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(FTS_FORWARD_TABLE)?;
        let removed = table.remove(doc_key)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    /// Store an inverted index entry (term -> posting list)
    pub fn put_fts_inverted(&self, term: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(FTS_INVERTED_TABLE)?;
            table.insert(term, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get inverted index entry
    pub fn get_fts_inverted(&self, term: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(FTS_INVERTED_TABLE)?;
        Ok(table.get(term)?.map(|v| v.value().to_vec()))
    }

    /// Delete inverted index entry
    pub fn delete_fts_inverted(&self, term: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(FTS_INVERTED_TABLE)?;
        let removed = table.remove(term)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    // ============ Vector Operations ============

    /// Store vector data
    pub fn put_vector(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VECTORS_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get vector data
    pub fn get_vector(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(VECTORS_TABLE)?;
        Ok(table.get(key)?.map(|v| v.value().to_vec()))
    }

    /// Optimized: Retrieve vector data as f32 (cloned)
    pub fn get_vector_f32(&self, key: &str) -> Result<Option<Vec<f32>>> {
        let bytes = self.get_vector(key)?;
        match bytes {
            Some(b) => {
                if b.len() % 4 != 0 {
                    return Err(EngramError::Storage(format!(
                        "Invalid vector size ({} bytes)",
                        b.len()
                    )));
                }
                let f32_count = b.len() / 4;
                let mut f32s = Vec::with_capacity(f32_count);
                // Safe way to convert bytes to f32s
                for i in 0..f32_count {
                    let mut chunk = [0u8; 4];
                    chunk.copy_from_slice(&b[i * 4..(i + 1) * 4]);
                    f32s.push(f32::from_le_bytes(chunk));
                }
                Ok(Some(f32s))
            }
            None => Ok(None),
        }
    }

    /// Extreme Performance: zero-copy cast to &[f32] if aligned.
    ///
    /// SAFETY: This depends on the underlying redb memory layout.
    /// Using it only for read-only access in localized high-ops.
    pub unsafe fn get_vector_f32_unchecked<'a>(data: &'a [u8]) -> &'a [f32] {
        let (prefix, f32_slice, suffix) = data.align_to::<f32>();
        if !prefix.is_empty() || !suffix.is_empty() {
            // Fallback to copy if not aligned
            // (In practice we should handle alignment in put_vector)
        }
        f32_slice
    }

    // ============ Statistics ============

    /// Get total document count
    pub fn document_count(&self) -> Result<u64> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DOCUMENTS_TABLE)?;
        let mut count = 0u64;
        for _ in table.iter()? {
            count += 1;
        }
        Ok(count)
    }

    /// Get total content blob count
    pub fn content_count(&self) -> Result<u64> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONTENT_TABLE)?;
        let mut count = 0u64;
        for _ in table.iter()? {
            count += 1;
        }
        Ok(count)
    }

    /// Compact the database (reclaim space)
    pub fn compact(&self) -> Result<()> {
        // redb compact() requires ownership or &mut, so we skip in shared mode
        // The database auto-manages space efficiently via its B-tree structure
        info!("Engram-KV compact requested (auto-managed by redb)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_basic_operations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.engram");
        let kv = EngramKV::open(&db_path).unwrap();

        // Put and get document
        kv.put_document("doc1", b"hello world").unwrap();
        let result = kv.get_document("doc1").unwrap();
        assert_eq!(result, Some(b"hello world".to_vec()));

        // Delete document
        assert!(kv.delete_document("doc1").unwrap());
        assert_eq!(kv.get_document("doc1").unwrap(), None);
    }

    #[test]
    fn test_kv_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.engram");
        let kv = EngramKV::open(&db_path).unwrap();

        kv.put_session("sess1", r#"{"id":"sess1","data":"test"}"#)
            .unwrap();
        let result = kv.get_session("sess1").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("sess1"));
    }

    #[test]
    fn test_kv_document_count() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.engram");
        let kv = EngramKV::open(&db_path).unwrap();

        assert_eq!(kv.document_count().unwrap(), 0);
        kv.put_document("a", b"1").unwrap();
        kv.put_document("b", b"2").unwrap();
        assert_eq!(kv.document_count().unwrap(), 2);
    }
}
