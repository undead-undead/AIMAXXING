//! RAG (Retrieval-Augmented Generation) Interfaces
//!
//! This module defines the standard interface for vector stores.
//! Implementations (like Qdrant, Pinecone, Postgres) should be handled
//! in the application layer (e.g. `listen-memory`), not here.

use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// A document retrieved from the vector store
#[derive(Debug, Clone)]
pub struct Document {
    /// Unique identifier
    pub id: String,
    /// The title or mnemonic for the document
    pub title: String,
    /// The full text content
    pub content: String,
    /// A shorter summary of the content (Tiered RAG)
    pub summary: Option<String>,
    /// The collection it belongs to
    pub collection: Option<String>,
    /// The virtual path/source
    pub path: Option<String>,
    /// Metadata associated with the document
    pub metadata: HashMap<String, String>,
    /// Similarity score (0.0 to 1.0)
    pub score: f32,
}

/// Interface for vector stores
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Store a text with metadata
    /// Returns the ID of the stored document
    async fn store(&self, content: &str, metadata: HashMap<String, String>) -> Result<String>;
    
    /// Search for similar documents
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Document>>;
    
    /// Delete a document by ID
    async fn delete(&self, id: &str) -> Result<()>;
}

/// Interface for embeddings providers
/// (Optional: Application might handle embeddings manually)
#[async_trait]
pub trait Embeddings: Send + Sync {
    /// Generate embedding vector for text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}
