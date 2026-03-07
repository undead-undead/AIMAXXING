use crate::error::{EngramError, Result};
use bytes::Bytes;
use std::path::Path;

pub mod redb_impl;

/// Storage Trait: The abstract backend for Engram
///
/// This trait decouples Engram from hardcoded redb bindings, allowing for:
/// - In-memory test stores
/// - Sled/RocksDB alternative backends
/// - Remote/Cloud KV proxies
pub trait Storage: Send + Sync {
    /// Database path (if applicable)
    fn path(&self) -> &Path;

    // ============ Document Operations ============
    fn put_document(&self, key: &str, data: &[u8]) -> Result<()>;
    fn get_document(&self, key: &str) -> Result<Option<Bytes>>;
    fn delete_document(&self, key: &str) -> Result<bool>;
    fn iter_documents(&self) -> Result<Vec<(String, Bytes)>>;
    fn document_count(&self) -> Result<u64>;

    // ============ Content Blob Operations ============
    fn put_content(&self, hash: &str, data: &[u8]) -> Result<()>;
    fn get_content(&self, hash: &str) -> Result<Option<Bytes>>;
    fn content_count(&self) -> Result<u64>;

    // ============ Collection Operations ============
    fn put_collection(&self, name: &str, data: &[u8]) -> Result<()>;
    fn get_collection(&self, name: &str) -> Result<Option<Bytes>>;
    fn list_collections(&self) -> Result<Vec<(String, Bytes)>>;

    // ============ Session Operations ============
    fn put_session(&self, id: &str, data: &str) -> Result<()>;
    fn get_session(&self, id: &str) -> Result<Option<String>>;
    fn delete_session(&self, id: &str) -> Result<bool>;

    // ============ FTS Index Operations ============
    fn put_fts_forward(&self, doc_key: &str, data: &[u8]) -> Result<()>;
    fn get_fts_forward(&self, doc_key: &str) -> Result<Option<Bytes>>;
    fn delete_fts_forward(&self, doc_key: &str) -> Result<bool>;

    fn put_fts_inverted(&self, term: &str, data: &[u8]) -> Result<()>;
    fn get_fts_inverted(&self, term: &str) -> Result<Option<Bytes>>;
    fn delete_fts_inverted(&self, term: &str) -> Result<bool>;

    // ============ Vector Operations ============
    fn put_vector(&self, key: &str, data: &[u8]) -> Result<()>;
    fn get_vector(&self, key: &str) -> Result<Option<Bytes>>;

    // ============ Embedding Cache Operations ============
    fn put_embedding_cache(&self, hash: &str, vector: &[f32]) -> Result<()>;
    fn get_embedding_cache(&self, hash: &str) -> Result<Option<Vec<f32>>>;

    /// Optimized: Retrieve vector data as f32
    fn get_vector_f32(&self, key: &str) -> Result<Option<Vec<f32>>> {
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

    // ============ Maintenance ============
    fn compact(&self) -> Result<()>;
}
