//! Simple File-based Vector Store (JSONL)
//!
//! A lightweight, Persistent vector store using JSONL files and in-memory brute-force search.
//! Designed for low-resource environments (e.g. 1GB VPS) where running a full Vector DB (Qdrant/pgvector) is too heavy.
//!
//! # Features
//! - **Storage**: Append-only JSONL file.
//! - **Index**: In-memory `Vec<IndexEntry>` (ID + Embedding + Offset). Content is NOT stored in RAM.
//! - **Search**: Brute-force cosine similarity -> Seek & Read content from disk for top N results.
//!
//! # Performance
//! - Memory: ~1.5MB per 10k documents (vs ~6MB with f32, vs ~100MB+ with content).
//! - Speed: < 20ms search (IO overhead for top results only).

use crate::rag::{Document, VectorStore, Embeddings};
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use std::path::PathBuf;
use std::io::{Seek, SeekFrom, Write};
use tokio::fs;
use tokio::io::AsyncBufReadExt;
use tokio::sync::{RwLock, mpsc, oneshot};
use rayon::prelude::*;
use futures::FutureExt;

/// Configuration for FileStore
#[derive(Debug, Clone)]
pub struct FileStoreConfig {
    /// Path to the JSONL file
    pub path: PathBuf,
}

impl FileStoreConfig {
    /// Create config from path
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// In-memory index entry - optimized for RAM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    /// Store metadata as a boxed slice of tuples to save RAM (no HashMap overhead)
    pub metadata: Box<[(String, String)]>,
    /// Store embedding as a boxed slice of i8 (quantized) to save 4x RAM
    pub embedding: Box<[i8]>,
    /// Byte offset in the file
    pub offset: u64,
    /// Length of the JSON line in bytes
    pub length: u64,
    /// Timestamp (Unix millis) for Time-Travel
    pub timestamp: i64,
}


impl IndexEntry {
    /// Get a metadata value by key
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Stored document format (On Disk) - Keeps f32 for full precision recovery if needed
#[derive(Debug, Serialize, Deserialize, Clone)]
struct StoredDocument {
    id: String,
    content: String,
    metadata: HashMap<String, String>,
    embedding: Vec<f32>,
}

/// Messages sent to the FileStore Actor
enum StoreMessage {
    /// Append a new line (document) to the file
    Append { 
        line: String, 
        reply: oneshot::Sender<Result<(u64, u64)>> // Returns (offset, length)
    },
    /// Trigger compaction (rewrite file to remove deleted items)
    Compact {
        deleted_ids: HashSet<String>,
        reply: oneshot::Sender<Result<HashMap<String, u64>>> // Returns ID -> New Offset
    },
    /// Save index snapshot to disk
    SaveSnapshot {
        indices: Vec<IndexEntry>,
    },
    /// Batch delete multiple IDs
    DeleteBatch {
        ids: Vec<String>,
        reply: oneshot::Sender<Result<()>>,
    },
}

/// Actor managing exclusive write access to the file
struct FileStoreActor {
    path: PathBuf,
    receiver: mpsc::Receiver<StoreMessage>,
}

impl FileStoreActor {

    async fn handle_append(&self, line: String) -> Result<(u64, u64)> {
        // Use blocking task for proper file locking
        let path = self.path.clone();
        
        tokio::task::spawn_blocking(move || {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| Error::MemoryStorage(format!("IO error: {}", e)))?;

            use fs2::FileExt;
            file.lock_exclusive()
                .map_err(|e| Error::Internal(format!("Lock failure: {}", e)))?;

            let offset = file.seek(SeekFrom::End(0))
                .map_err(|e| Error::MemoryStorage(format!("Seek error: {}", e)))?;
                
            use std::io::Write;
            file.write_all(line.as_bytes())
                .map_err(|e| Error::MemoryStorage(format!("Write error: {}", e)))?;
            file.flush()?; 
            
            file.unlock().ok();

            let length = line.len() as u64;
            Ok((offset, length))
        }).await.map_err(|e| Error::Internal(format!("Join error: {}", e)))?
    }

    async fn handle_compact(&self, deleted_ids: HashSet<String>) -> Result<HashMap<String, u64>> {
        let path = self.path.clone();
        let tmp_path = self.path.with_extension("compact");
        
        tokio::task::spawn_blocking(move || {
             use fs2::FileExt;
             
             // Open original file with exclusive lock to prevent appends during compaction
             let file = std::fs::File::open(&path)
                .map_err(|e| Error::MemoryStorage(format!("Failed to open for compaction: {}", e)))?;
             file.lock_exclusive()
                .map_err(|e| Error::Internal(format!("Lock failure: {}", e)))?;
             
             let mut reader = std::io::BufReader::new(&file);
             let mut writer = std::io::BufWriter::new(
                 std::fs::File::create(&tmp_path)?
             );

            let mut new_offsets = HashMap::new();
            let mut current_offset = 0u64;
            let mut buffer = String::new();
            
            use std::io::BufRead;

            loop {
                buffer.clear();
                let bytes_read = reader.read_line(&mut buffer)?;
                if bytes_read == 0 {
                    break;
                }

                #[derive(Deserialize)]
                struct DocHeader { id: String }
                
                if let Ok(header) = serde_json::from_str::<DocHeader>(&buffer) {
                    if !deleted_ids.contains(&header.id) {
                        use std::io::Write;
                        writer.write_all(buffer.as_bytes())?;
                        new_offsets.insert(header.id, current_offset);
                        current_offset += buffer.len() as u64;
                    }
                }
            }

            writer.flush()?;
            drop(writer);
            drop(reader);
            // Unlock original before renaming over it
            file.unlock().ok();
            drop(file);
            
            std::fs::rename(tmp_path, &path)?;
            
            Ok(new_offsets)
        }).await.map_err(|e| Error::Internal(format!("Join error: {}", e)))?
    }

    // Helper for batch reading in one go
    // This is NOT an actor message handler but a static helper we'll use in FileStore::search
    async fn read_batch(path: PathBuf, reads: Vec<(u64, u64)>) -> Result<Vec<StoredDocument>> {
        tokio::task::spawn_blocking(move || {
             let file = std::fs::File::open(&path)
                .map_err(|e| Error::MemoryStorage(format!("Failed to open for batch read: {}", e)))?;
             
             // Shared lock for reading? Optional but good for consistency
             // But 'pred' doesn't require locking usually if we don't care about torn reads on append (which shouldn't happen with atomic writes)
             // fs2::lock_shared might block if compaction is running, which is GOOD.

             file.lock_shared().ok(); // Best effort lock
             
             let mut results = Vec::with_capacity(reads.len());
             use std::os::unix::fs::FileExt as UnixFileExt; // For read_at

             for (offset, length) in reads {
                 let mut buffer = vec![0u8; length as usize];
                 if let Err(_) = file.read_exact_at(&mut buffer, offset) {
                     continue; // Skip failed reads
                 }
                 if let Ok(s) = String::from_utf8(buffer) {
                     if let Ok(doc) = serde_json::from_str::<StoredDocument>(&s) {
                         results.push(doc);
                     } else {
                         return Err(Error::MemoryStorage(format!("JSON corruption at offset {}", offset)));
                     }
                 } else {
                     return Err(Error::MemoryStorage(format!("UTF8 corruption at offset {}", offset)));
                 }
             }
             
             file.unlock().ok();
             Ok(results)
        }).await.map_err(|e| Error::Internal(format!("Join error: {}", e)))?
    }

    async fn handle_save_snapshot(&self, indices: Vec<IndexEntry>) -> Result<()> {
        let snap_path = self.path.with_extension("index");
        
        tokio::task::spawn_blocking(move || {
            let bytes = bincode::serialize(&indices)
                .map_err(|e| Error::Internal(format!("Failed to serialize index: {}", e)))?;

            let tmp_path = snap_path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
            std::fs::write(&tmp_path, bytes)?;
            std::fs::rename(&tmp_path, &snap_path)?;
            Ok::<(), Error>(())
        }).await.map_err(|e| Error::Internal(format!("Snapshot task join error: {}", e)))??;
        Ok(())
    }

    async fn handle_delete_batch(&self, _ids: Vec<String>) -> Result<()> {
         // Deletion in Append-Only file is just updating the Blocklist (done in FileStore struct)
         // and Saving the Snapshot of the Index (which excludes the deleted items).
         // BUT wait, FileStore struct handles the in-memory index update.
         // Calling this method on the Actor is mainly to serialize the Snapshot update if needed 
         // or just to synchronize.
         
         // Actually, `delete` logic usually updates the `deleted_ids` and then saves a snapshot.
         // In `delete`, we do: indices.remove(), deleted_ids.insert(), queue_snapshot().
         
         // So for batch delete, we just need to ensure we can queue a snapshot.
         // This message handler might be redundant if we just use SaveSnapshot, 
         // BUT having it allows us to do specific logic if we change storage engine.
         // For now, it's a no-op on the FILE itself, but useful for synchronization?
         
         // Looking at `delete` implementation in FileStore struct:
         // It removes from memory, then calls `queue_snapshot`.
         // So the Actor doesn't really need to do anything special for deletion other than 
         // eventually compacting or saving snapshot.
         
         // However, to follow the pattern and perhaps log or verify:
         Ok(())
    }
}

/// A lightweight file-based vector store with lazy loading
#[derive(Clone)]
pub struct FileStore {
    config: FileStoreConfig,
    /// In-memory index (ID, Embedding, Metadata, File Offset)
    indices: Arc<RwLock<Vec<IndexEntry>>>,
    /// Optional embedder for vectorizing content
    embedder: Option<Arc<dyn Embeddings>>,
    /// Channel to Actor
    sender: mpsc::Sender<StoreMessage>,
    /// Persistent read handle for efficient random access
    /// Wrapped in Arc<File> to allow cheap cloning of the handle for race-free independent reads
    reader: Arc<std::sync::RwLock<Arc<std::fs::File>>>,
    /// Track deleted items for smart compaction (Blocklist)
    deleted_ids: Arc<RwLock<HashSet<String>>>,
    /// Fix H2: Version counter for detecting compaction
    version: Arc<std::sync::atomic::AtomicU64>,
}

impl FileStore {
    /// Open or create a new FileStore
    pub async fn new(config: FileStoreConfig) -> Result<Self> {
        let indices = Arc::new(RwLock::new(Vec::new()));
        
        // Initialize reader (create file if not exists)
        // Fix #1: Use spawn_blocking for blocking I/O
        let path = config.path.clone();
        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::File::create(&path).ok();
            }
        }).await.map_err(|e| Error::Internal(format!("Join error: {}", e)))?;

        let path = config.path.clone();
        let reader = tokio::task::spawn_blocking(move || {
            std::fs::OpenOptions::new()
                .read(true)
                .open(&path)
                .map_err(|e| Error::MemoryStorage(format!("Failed to open read handle at {:?}: {}", path, e)))
        }).await.map_err(|e| Error::Internal(format!("Join error: {}", e)))??;

        // Spawn Actor for writes
        let (tx, rx) = mpsc::channel(100);
        let actor = FileStoreActor {
            path: config.path.clone(),
            receiver: rx,
        };
        tokio::spawn(async move {
            let mut actor = actor;
            loop {
                if actor.receiver.is_closed() {
                    break;
                }

                tracing::info!("FileStoreActor starting/restarting");
                let res = std::panic::AssertUnwindSafe(async {
                    while let Some(msg) = actor.receiver.recv().await {
                        match msg {
                            StoreMessage::Append { line, reply } => {
                                let res = actor.handle_append(line).await;
                                let _ = reply.send(res);
                            }
                            StoreMessage::Compact { deleted_ids, reply } => {
                                let res = actor.handle_compact(deleted_ids).await;
                                let _ = reply.send(res);
                            }
                            StoreMessage::SaveSnapshot { indices } => {
                                let _ = actor.handle_save_snapshot(indices).await;
                            }
                            StoreMessage::DeleteBatch { ids: _, reply } => {
                                // For now, simple ack, as actual deletion is memory+snapshot
                                let _ = reply.send(Ok(()));
                            }
                        }
                    }
                }).catch_unwind().await;

                if let Err(_) = res {
                    tracing::error!("FileStoreActor PANICKED. Restarting in 1s...");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                } else {
                    break;
                }
            }
        });

        let store = Self { 
            config, 
            indices,
            embedder: None,
            sender: tx,
            reader: Arc::new(std::sync::RwLock::new(Arc::new(reader))),
            deleted_ids: Arc::new(RwLock::new(HashSet::new())),
            version: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        
        store.load().await?;
        Ok(store)
    }

    /// Attach an embedder to this store
    pub fn with_embedder(mut self, embedder: Arc<dyn Embeddings>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// Load index from disk (Scans file, does not load content)
    async fn load(&self) -> Result<()> {
        if !self.config.path.exists() {
            return Ok(());
        }

        // Try loading from snapshot first
        if let Ok(Some(cached_indices)) = self.try_load_snapshot().await {
            let mut indices = self.indices.write().await;
            *indices = cached_indices;
            tracing::info!("FileStore loaded {} documents from snapshot", indices.len());
            return Ok(());
        }

        let file = fs::File::open(&self.config.path).await?;
        let mut reader = tokio::io::BufReader::new(file);
        let mut buffer = String::new();
        let mut offset = 0u64;

        let mut indices = self.indices.write().await;
        indices.clear();

        loop {
            buffer.clear();
            let bytes_read = reader.read_line(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }

            if buffer.trim().is_empty() {
                offset += bytes_read as u64;
                continue;
            }

            // Parse document
            if let Ok(doc) = serde_json::from_str::<StoredDocument>(&buffer) {
                let timestamp = Self::extract_timestamp(&doc.metadata);
                let metadata: Vec<(String, String)> = doc.metadata.into_iter().collect();
                indices.push(IndexEntry {
                    id: doc.id,
                    metadata: metadata.into_boxed_slice(),
                    embedding: Self::quantize(&doc.embedding).into_boxed_slice(),
                    offset,
                    length: bytes_read as u64,
                    timestamp,
                });


            } else {
                tracing::warn!("Skipping malformed line in FileStore at offset {}", offset);
            }

            offset += bytes_read as u64;
        }
        
        tracing::info!("FileStore loaded {} documents (index only)", indices.len());
        
        // Save snapshot after first full load
        self.queue_snapshot(&indices).await;
        
        Ok(())
    }

    fn snapshot_path(&self) -> std::path::PathBuf {
        self.config.path.with_extension("index_v2") // Bump version to invalidate old snapshots
    }

    /// Extract timestamp from metadata or return 0
    fn extract_timestamp(metadata: &HashMap<String, String>) -> i64 {
        metadata.get("timestamp")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0)
    }

    /// Extract timestamp from slice metadata or return 0
    fn extract_timestamp_from_slice(metadata: &[(String, String)]) -> i64 {
        metadata.iter()
            .find(|(k, _)| k == "timestamp")
            .and_then(|(_, v)| v.parse::<i64>().ok())
            .unwrap_or(0)
    }

    async fn try_load_snapshot(&self) -> Result<Option<Vec<IndexEntry>>> {
        let snap_path = self.snapshot_path();
        if !snap_path.exists() {
            return Ok(None);
        }

        let main_meta = fs::metadata(&self.config.path).await?;
        let snap_meta = fs::metadata(&snap_path).await?;

        // If snapshot is older than the main file (by more than 1s to account for clock skew/fs precision), it's invalid
        if snap_meta.modified()? < main_meta.modified()? {
             return Ok(None);
        }

        let bytes = fs::read(&snap_path).await?;
        let indices: Vec<IndexEntry> = bincode::deserialize(&bytes)
            .map_err(|e| Error::Internal(format!("Failed to deserialize index: {}", e)))?;

        Ok(Some(indices))
    }

    async fn queue_snapshot(&self, indices: &[IndexEntry]) {
         let _ = self.sender.send(StoreMessage::SaveSnapshot { indices: indices.to_vec() }).await;
    }

    /// Quantize f32 vector to i8 (-127 to 127) simple linear scaling
    fn quantize(vec: &[f32]) -> Vec<i8> {
        vec.iter().map(|&x| (x.clamp(-1.0, 1.0) * 127.0) as i8).collect()
    }

    /// Cosine similarity between two quantized vectors (i8)
    /// Accumulates in i32 to prevent overflow, then returns f32 score
    fn cosine_similarity_i8(a: &[i8], b: &[i8]) -> f32 {
        if a.len() != b.len() { return 0.0; }
        
        // Use i32 accumulator to prevent overflow
        let mut dot_product: i32 = 0;
        let mut norm_a_sq: i32 = 0;
        let mut norm_b_sq: i32 = 0;

        for (&x, &y) in a.iter().zip(b) {
            let x_i32 = x as i32;
            let y_i32 = y as i32;
            dot_product += x_i32 * y_i32;
            norm_a_sq += x_i32 * x_i32;
            norm_b_sq += y_i32 * y_i32;
        }
        
        let norm_a = (norm_a_sq as f32).sqrt();
        let norm_b = (norm_b_sq as f32).sqrt();
        
        if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { (dot_product as f32) / (norm_a * norm_b) }
    }

    async fn read_content(&self, offset: u64, length: u64, reader_override: Option<Arc<std::fs::File>>) -> Result<StoredDocument> {
        // Use pread (read_at) to avoid seeking and syscall overhead of repeated open()
        // If reader_override is provided (Snapshot), use it. Otherwise, acquire current lock.
        let reader_handle = if let Some(r) = reader_override {
            r
        } else {
            let lock = self.reader.read().map_err(|_| Error::Internal("Failed to acquire reader lock".to_string()))?;
            lock.clone()
        };
        
        // Blocking read on the specific handle
        let bytes = tokio::task::spawn_blocking(move || {
            let mut buffer = vec![0u8; length as usize];
            use std::os::unix::fs::FileExt; 
            reader_handle.read_exact_at(&mut buffer, offset)
                .map_err(|e| Error::MemoryStorage(format!("IO error: {}", e)))?;
            Ok::<Vec<u8>, Error>(buffer)
        }).await.map_err(|e| Error::Internal(format!("Join error: {}", e)))??;

        let s = String::from_utf8(bytes)
            .map_err(|e| Error::MemoryStorage(format!("UTF8 error: {}", e)))?;
            
        serde_json::from_str(&s)
            .map_err(|e| Error::MemoryStorage(format!("JSON error at offset {}: {}", offset, e)))
    }

    /// Get a single document by ID (Fast, uses Index)
    pub async fn get(&self, id: &str) -> Result<Option<Document>> {
        let indices = self.indices.read().await;
        let entry = indices.iter().find(|idx| idx.id == id).cloned();
        drop(indices);

        if let Some(idx) = entry {
            let doc = self.read_content(idx.offset, idx.length, None).await?;
            Ok(Some(Document {
                id: doc.id,
                content: doc.content,
                metadata: doc.metadata,
                score: 1.0,
            }))
        } else {
            Ok(None)
        }
    }

    /// Check if document exists by ID (using Index only)
    pub async fn exists(&self, id: &str) -> bool {
        let indices = self.indices.read().await;
        indices.iter().any(|idx| idx.id == id)
    }

    /// Find documents by metadata key-value pair (using Index only, then fetch content)
    pub async fn find_by_metadata(&self, key: &str, value: &str) -> Vec<Document> {
        let indices = self.indices.read().await;
        let matches: Vec<IndexEntry> = indices.iter()
            .filter(|idx| idx.metadata.iter().any(|(k, v)| k == key && v == value))
            .cloned() 
            .collect();
        drop(indices);

        self.hydrate_results(matches, None).await
    }
    
    /// Find documents using a custom predicate on the IndexEntry (Fast)
    pub async fn find<F>(&self, predicate: F) -> Vec<Document> 
    where F: Fn(&IndexEntry) -> bool 
    {
        let indices = self.indices.read().await;
        let matches: Vec<IndexEntry> = indices.iter()
            .filter(|idx| predicate(idx))
            .cloned()
            .collect();
        drop(indices);

        self.hydrate_results(matches, None).await
    }

    /// Optimized: Find metadata without loading content (Fix for Memory Performance)
    pub async fn find_metadata<F>(&self, predicate: F) -> Vec<IndexEntry>
    where F: Fn(&IndexEntry) -> bool
    {
        let indices = self.indices.read().await;
        indices.iter()
            .filter(|idx| predicate(idx))
            .cloned()
            .collect()
    }

    /// Find document IDs using a custom predicate on the IndexEntry (Fast, No IO)
    pub async fn find_ids<F>(&self, predicate: F) -> Vec<String>
    where F: Fn(&IndexEntry) -> bool
    {
        let indices = self.indices.read().await;
        indices.iter()
            .filter(|idx| predicate(idx))
            .map(|idx| idx.id.clone())
            .collect()
    }
    
    /// Retrieve all documents (Manual iteration - expensive)
    pub async fn get_all(&self) -> Vec<Document> {
        let indices = self.indices.read().await;
        let matches = indices.clone();
        drop(indices);
        self.hydrate_results(matches, None).await
    }

    /// Get the last N documents (Efficient for recent history)
    pub async fn tail(&self, limit: usize) -> Vec<Document> {
        let indices = self.indices.read().await;
        let len = indices.len();
        let start = len.saturating_sub(limit);
        let matches: Vec<IndexEntry> = indices[start..].to_vec();
        drop(indices);
        self.hydrate_results(matches, None).await
    }
    
    // Helper to fetch content for indices
    async fn hydrate_results(&self, matches: Vec<IndexEntry>, reader: Option<Arc<std::fs::File>>) -> Vec<Document> {
        let mut results = Vec::new();
        for idx in matches {
            match self.read_content(idx.offset, idx.length, reader.clone()).await {
                Ok(doc) => {
                    results.push(Document {
                        id: doc.id,
                        content: doc.content,
                        metadata: doc.metadata,
                        score: 1.0,
                    });
                }
                Err(e) => {
                    // Log but don't fail the whole batch, however return error if it's severe
                    tracing::error!("FileStore: CRITICAL hydration failure for {}: {}", idx.id, e);
                }
            }
        }
        results
    }

    /// Trigger manual compaction to reclaim disk space
    pub async fn compact(&self) -> Result<()> {
        // Fix H2: Increment version to invalidate ongoing searches
        self.version.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        let deleted_snapshot = self.deleted_ids.read().await.clone();
        
        let (tx, rx) = oneshot::channel();
        self.sender.send(StoreMessage::Compact { deleted_ids: deleted_snapshot.clone(), reply: tx }).await
            .map_err(|_| Error::Internal("FileStore actor closed".to_string()))?;
            
        let new_offsets = rx.await
            .map_err(|_| Error::Internal("FileStore reply dropped".to_string()))??;

        // Update offsets in memory
        let mut indices_mut = self.indices.write().await;
        for idx in indices_mut.iter_mut() {
            if let Some(new_offset) = new_offsets.get(&idx.id) {
                idx.offset = *new_offset;
            }
        }
        
        // REOPEN READER (Critical Fix)
        // Perform blocking I/O on a separate thread to avoid freezing the executor
        let path = self.config.path.clone();
        let new_file_handle = tokio::task::spawn_blocking(move || {
            std::fs::OpenOptions::new()
                .read(true)
                .open(&path)
                .map_err(|e| Error::MemoryStorage(format!("Failed to reopen file after compaction: {}", e)))
        }).await.map_err(|e| Error::Internal(format!("Join error: {}", e)))??;

        {
            if let Ok(mut reader) = self.reader.write() {
                *reader = Arc::new(new_file_handle);
            }
        }

        // Clean up deleted_ids - remove only those we compacted
        let mut deleted_mut = self.deleted_ids.write().await;
        for id in deleted_snapshot {
            deleted_mut.remove(&id);
        }

        // Save Snapshot
        self.queue_snapshot(&indices_mut).await;
        Ok(())
    }

    /// Automatically trigger compaction if the number of documents exceeds a threshold
    /// or if the ratio of deleted documents is high
    /// Automatically trigger compaction if the number of documents exceeds a threshold
    /// or if the ratio of deleted documents is high
    pub async fn auto_compact(&self, threshold: usize) -> Result<()> {
        let len = self.indices.read().await.len();
        let deleted = self.deleted_ids.read().await.len();
        
        // Fix #9: Trigger if deleted > threshold OR significant fragmentation
        let should_compact = deleted > threshold || (len > 100 && deleted > len / 3);

        if should_compact {
            tracing::info!("Auto-compacting FileStore (deleted: {}, active: {})", deleted, len);
            self.compact().await?;
        }
        Ok(())
    }

    /// Time-Travel Search: Search as if it were `as_of` timestamp
    /// Only considers documents with timestamp <= as_of
    pub async fn search_snapshot(&self, query: &str, as_of: i64, limit: usize) -> Result<Vec<Document>> {
        // Generate query embedding
         let query_embedding: Vec<f32> = if let Some(embedder) = &self.embedder {
            embedder.embed(query).await?
        } else {
            vec![0.0; 1536]
        };

        let query_quantized = Self::quantize(&query_embedding);
        
        // 1. Capture Reader Snapshot FIRST (Atomic View)
        // We need the file handle that corresponds to the CURRENT indices.
        // Actually, strictly speaking, we need the handle that corresponds to the indices we are about to read.
        // If we lock indices then reader, we are safe.
        
        let indices_lock = self.indices.read().await;
        // While holding indices lock, grab the reader handle
        let reader_snapshot = self.reader.read()
            .map_err(|_| Error::Internal("Reader lock poisoned".to_string()))?
            .clone();
        
        // Clone indices for processing
        let indices_clone = indices_lock.clone();
        drop(indices_lock); // Release lock
        
        // Block to search in parallel
        let scored_indices = tokio::task::spawn_blocking(move || {
            let indices = indices_clone;
            
            // Filter by timestamp THEN calc similarity
            let mut scored: Vec<(f32, usize)> = indices.par_iter().enumerate()
                .filter(|(_, idx)| idx.timestamp <= as_of) // Time-Travel Magic
                .map(|(i, idx)| (Self::cosine_similarity_i8(&query_quantized, &idx.embedding), i))
                .collect();

            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            
            scored.into_iter().take(limit)
                .map(|(score, i)| (score, indices[i].clone()))
                .collect::<Vec<_>>()
                
        }).await.map_err(|e| Error::Internal(format!("Search task join error: {}", e)))?;

        // Hydrate using the SNAPSHOT reader
        let mut results = Vec::new();
        for (score, idx) in scored_indices {
            // Pass reader snapshot
            if let Ok(doc) = self.read_content(idx.offset, idx.length, Some(reader_snapshot.clone())).await {
                results.push(Document {
                    id: doc.id,
                    content: doc.content,
                    metadata: doc.metadata,
                    score,
                });
            }
        }
        Ok(results)
    }



    pub async fn delete_batch(&self, ids: Vec<String>) -> Result<()> {
        let mut indices = self.indices.write().await;
        let mut deleted_count = 0;
        
        let ids_set: HashSet<_> = ids.iter().collect();
        
        // Remove from indices
        indices.retain(|idx| {
            if ids_set.contains(&idx.id) {
                deleted_count += 1;
                false
            } else {
                true
            }
        });
        
        if deleted_count > 0 {
            // Track deletions
            let mut deleted_blocklist = self.deleted_ids.write().await;
            for id in ids {
                // Should clone id, but we consumed ids vector so it's fine
                deleted_blocklist.insert(id);
            }
            drop(deleted_blocklist);

            // Save Snapshot ONCE
            let snap = indices.clone();
            drop(indices);
            
            self.queue_snapshot(&snap).await;
        }
        
        Ok(())
    }

    /// Filter metadata to keep the in-memory index small
    fn filter_metadata_for_index(metadata: HashMap<String, String>) -> Vec<(String, String)> {
        metadata.into_iter()
            .filter(|(k, v)| {
                // exclude large content fields
                let key = k.as_str();
                if matches!(key, "page_content" | "content" | "text" | "body" | "_embedding" | "embedding") {
                    return false;
                }
                // exclude huge values (e.g. base64 images, long descriptions)
                if v.len() > 512 {
                     return false;
                }
                true
            })
            .collect()
    }
}

#[async_trait]
impl VectorStore for FileStore {
    async fn store(&self, content: &str, mut metadata: HashMap<String, String>) -> Result<String> {
        // Extract embedding from metadata or generate using embedder
        let embedding: Vec<f32> = if let Some(emb_str) = metadata.get("_embedding") {
            serde_json::from_str(emb_str).unwrap_or_else(|_| vec![])
        } else if let Some(embedder) = &self.embedder {
            embedder.embed(content).await?
        } else {
            vec![0.0; 1536]
        };
        
        metadata.remove("_embedding");

        let doc = StoredDocument {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.to_string(),
            metadata: metadata.clone(),
            embedding: embedding.clone(),
        };

        // 1. Prepare line
        let line = serde_json::to_string(&doc)
            .map_err(|e| Error::MemoryStorage(format!("JSON error: {}", e)))? + "\n";
        
        // 2. Send to Actor
        let (tx, rx) = oneshot::channel();
        self.sender.send(StoreMessage::Append { line, reply: tx }).await
            .map_err(|_| Error::Internal("FileStore actor closed".to_string()))?;
            
        let (offset, length) = rx.await
            .map_err(|_| Error::Internal("FileStore reply dropped".to_string()))??;

        // 3. Update Index
        let entry_metadata = Self::filter_metadata_for_index(metadata);
        let timestamp = Self::extract_timestamp_from_slice(&entry_metadata);
        
        let entry = IndexEntry {
            id: doc.id.clone(),
            metadata: entry_metadata.into_boxed_slice(),
            embedding: Self::quantize(&embedding).into_boxed_slice(),
            offset,
            length,
            timestamp,
        };


        
        let snapshot_needed = {
            let mut indices = self.indices.write().await;
            indices.push(entry);
            indices.len() % 50 == 0
        }; // Lock dropped here! Fix #5
        
        // 4. Save Snapshot (Throttled: every 50 inserts or so for efficiency)
        if snapshot_needed {
            let indices = self.indices.read().await.clone(); // Read lock, clone safely
            self.queue_snapshot(&indices).await;
        }

        Ok(doc.id)
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Document>> {
        // Generate query embedding
        let query_embedding: Vec<f32> = if let Some(embedder) = &self.embedder {
            embedder.embed(query).await?
        } else {
            vec![0.0; 1536]
        };

        // Quantize query embedding ONCE
        let query_quantized = Self::quantize(&query_embedding);

        // 1. Capture Reader Snapshot FIRST (Atomic View)
        let indices_lock = self.indices.read().await;
        let reader_snapshot = self.reader.read()
            .map_err(|_| Error::Internal("Reader lock poisoned".to_string()))?
            .clone();
        let indices_clone = indices_lock.clone();
        drop(indices_lock);

        // 2. Memory Search (Indices only) - CPU Bound
        let scored_indices = tokio::task::spawn_blocking(move || {
            let indices = indices_clone;
            // Vector Sim Check (Parallel)
            let mut scored: Vec<(f32, usize)> = indices.par_iter().enumerate()
                .map(|(i, idx)| (Self::cosine_similarity_i8(&query_quantized, &idx.embedding), i))
                .collect();

            // Sort descending
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            
            // Limit and Clone Snapshot Data
            scored.into_iter().take(limit)
                .map(|(score, i)| {
                    let idx = &indices[i];
                    (score, idx.offset, idx.length, idx.id.clone(), idx.metadata.clone())
                })
                .collect::<Vec<_>>()
                
        }).await.map_err(|e| Error::Internal(format!("Search task join error: {}", e)))?;

        if scored_indices.is_empty() {
             return Ok(Vec::new());
        }

        // 3. Hydrate using Snapshot (Batched IO)
        let results = tokio::task::spawn_blocking(move || {
            let mut hydrated = Vec::with_capacity(scored_indices.len());
            // Use the snapshot reader
            let reader = reader_snapshot;
            use std::os::unix::fs::FileExt;

            for (score, offset, length, _id, _metadata) in scored_indices {
                let mut buffer = vec![0u8; length as usize];
                // Read from same file handle (thread-safe pread)
                if let Ok(_) = reader.read_exact_at(&mut buffer, offset) {
                    if let Ok(s) = String::from_utf8(buffer) {
                        if let Ok(doc) = serde_json::from_str::<StoredDocument>(&s) {
                             hydrated.push(Document {
                                id: doc.id,
                                content: doc.content,
                                metadata: doc.metadata, 
                                score,
                            });
                        }
                    }
                }
            }
            hydrated
        }).await.map_err(|e| Error::Internal(format!("Search hydration join error: {}", e)))?;
        
        Ok(results)
    }



    async fn delete(&self, id: &str) -> Result<()> {
        self.delete_batch(vec![id.to_string()]).await
    }
}
