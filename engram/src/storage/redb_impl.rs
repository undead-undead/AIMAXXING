//! redb implementation of the Storage trait

use crate::error::{EngramError, Result};
use crate::storage::Storage;
use bytes::Bytes;
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
const EMBEDDING_CACHE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("embedding_cache");

/// Engram-KV storage engine using redb
pub struct EngramKV {
    db: Arc<Database>,
    path: PathBuf,
}

impl EngramKV {
    /// Create or open an Engram-KV database at the given path
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

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
            let _ = write_txn.open_table(EMBEDDING_CACHE_TABLE)?;
        }
        write_txn.commit()?;

        info!("Engram-KV opened at: {}", path.display());

        Ok(Self {
            db: Arc::new(db),
            path,
        })
    }
}

impl Storage for EngramKV {
    fn path(&self) -> &Path {
        &self.path
    }

    // ============ Document Operations ============

    fn put_document(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DOCUMENTS_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_document(&self, key: &str) -> Result<Option<Bytes>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DOCUMENTS_TABLE)?;
        Ok(table.get(key)?.map(|v| Bytes::copy_from_slice(v.value())))
    }

    fn delete_document(&self, key: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(DOCUMENTS_TABLE)?;
        let removed = table.remove(key)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    fn iter_documents(&self) -> Result<Vec<(String, Bytes)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DOCUMENTS_TABLE)?;
        let mut results = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            results.push((
                key.value().to_string(),
                Bytes::copy_from_slice(value.value()),
            ));
        }
        Ok(results)
    }

    // ============ Content Blob Operations ============

    fn put_content(&self, hash: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CONTENT_TABLE)?;
            table.insert(hash, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_content(&self, hash: &str) -> Result<Option<Bytes>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONTENT_TABLE)?;
        Ok(table.get(hash)?.map(|v| Bytes::copy_from_slice(v.value())))
    }

    // ============ Collection Operations ============

    fn put_collection(&self, name: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(COLLECTIONS_TABLE)?;
            table.insert(name, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_collection(&self, name: &str) -> Result<Option<Bytes>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(COLLECTIONS_TABLE)?;
        Ok(table.get(name)?.map(|v| Bytes::copy_from_slice(v.value())))
    }

    fn list_collections(&self) -> Result<Vec<(String, Bytes)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(COLLECTIONS_TABLE)?;
        let mut results = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            results.push((
                key.value().to_string(),
                Bytes::copy_from_slice(value.value()),
            ));
        }
        Ok(results)
    }

    // ============ Session Operations ============

    fn put_session(&self, id: &str, data: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SESSIONS_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_session(&self, id: &str) -> Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SESSIONS_TABLE)?;
        Ok(table.get(id)?.map(|v| v.value().to_string()))
    }

    fn delete_session(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(SESSIONS_TABLE)?;
        let removed = table.remove(id)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    // ============ FTS Index Operations ============

    fn put_fts_forward(&self, doc_key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(FTS_FORWARD_TABLE)?;
            table.insert(doc_key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_fts_forward(&self, doc_key: &str) -> Result<Option<Bytes>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(FTS_FORWARD_TABLE)?;
        Ok(table
            .get(doc_key)?
            .map(|v| Bytes::copy_from_slice(v.value())))
    }

    fn delete_fts_forward(&self, doc_key: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(FTS_FORWARD_TABLE)?;
        let removed = table.remove(doc_key)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    fn put_fts_inverted(&self, term: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(FTS_INVERTED_TABLE)?;
            table.insert(term, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_fts_inverted(&self, term: &str) -> Result<Option<Bytes>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(FTS_INVERTED_TABLE)?;
        Ok(table.get(term)?.map(|v| Bytes::copy_from_slice(v.value())))
    }

    fn delete_fts_inverted(&self, term: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(FTS_INVERTED_TABLE)?;
        let removed = table.remove(term)?.is_some();
        drop(table);
        write_txn.commit()?;
        Ok(removed)
    }

    // ============ Vector Operations ============

    fn put_vector(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VECTORS_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_vector(&self, key: &str) -> Result<Option<Bytes>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(VECTORS_TABLE)?;
        Ok(table.get(key)?.map(|v| Bytes::copy_from_slice(v.value())))
    }

    // ============ Embedding Cache Operations ============

    fn put_embedding_cache(&self, hash: &str, vector: &[f32]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(EMBEDDING_CACHE_TABLE)?;
            let bytes: Vec<u8> = vector.iter().flat_map(|&f| f.to_le_bytes()).collect();
            table.insert(hash, bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_embedding_cache(&self, hash: &str) -> Result<Option<Vec<f32>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(EMBEDDING_CACHE_TABLE)?;
        let data = table.get(hash)?;
        match data {
            Some(v) => {
                let bytes = v.value();
                if bytes.len() % 4 != 0 {
                    return Err(EngramError::Storage("Invalid embedding size".to_string()));
                }
                let mut vector = Vec::with_capacity(bytes.len() / 4);
                for i in 0..(bytes.len() / 4) {
                    let mut chunk = [0u8; 4];
                    chunk.copy_from_slice(&bytes[i * 4..(i + 1) * 4]);
                    vector.push(f32::from_le_bytes(chunk));
                }
                Ok(Some(vector))
            }
            None => Ok(None),
        }
    }

    // ============ Statistics ============

    fn document_count(&self) -> Result<u64> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DOCUMENTS_TABLE)?;
        let mut count = 0u64;
        for _ in table.iter()? {
            count += 1;
        }
        Ok(count)
    }

    fn content_count(&self) -> Result<u64> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CONTENT_TABLE)?;
        let mut count = 0u64;
        for _ in table.iter()? {
            count += 1;
        }
        Ok(count)
    }

    fn compact(&self) -> Result<()> {
        info!("Engram-KV compact requested (auto-managed by redb)");
        Ok(())
    }
}
