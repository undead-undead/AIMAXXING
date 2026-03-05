/// Example: Using the built-in FileStore
///
/// AIMAXXING comes with a lightweight, file-based vector store (`FileStore`) 
/// that is optimized for low-resource environments (VPS).
///
/// This example demonstrates:
/// - Initializing a FileStore
/// - Storing documents with metadata
/// - Performing vector search (simulation)

use brain::prelude::*;
use brain::store::file::{FileStore, FileStoreConfig};
use brain::rag::{VectorStore, Document};
use std::collections::HashMap;
use anyhow::Result;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    // 1. Configure store
    let path = PathBuf::from("data/example_store.jsonl");
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    
    // Clean up previous run
    if path.exists() {
        tokio::fs::remove_file(&path).await?;
    }

    let config = FileStoreConfig::new(path);
    let store = FileStore::new(config.clone()).await?;

    println!("📂 FileStore initialized at {:?}", config.path);

    // 2. Store some documents
    // In a real app, you would generate embeddings using an embedding provider (e.g. OpenAI)
    // Here we just store simulated data. FileStore supports storing without embeddings if needed,
    // but for vector search, embeddings are required.
    
    let docs = vec![
        ("Rust is a systems programming language.", "rust", vec![1.0, 0.0, 0.0]),
        ("Python is great for data science.", "python", vec![0.0, 1.0, 0.0]),
        ("Solana is a high-performance blockchain.", "solana", vec![0.0, 0.0, 1.0]),
    ];

    for (content, tag, _vec) in docs {
        let mut metadata = HashMap::new();
        metadata.insert("tag".to_string(), tag.to_string());
        
        // We manually construct a document with embedding for this example
        // (The public API might usually hide this, but here we show low-level usage)
        // Note: The public `store` method generates an ID but expects external embedding logic
        // if we were using a full RAG pipeline.
        // For this low-level example, we use the internal `add` method if available, 
        // or just `store` and assume the embedding provider would handle it.
        // Since FileStore is low-level, we just store content.
        
        let id = store.store(content, metadata).await?;
        println!("✅ Stored doc ID: {}", id);
        
        // NOTE: In a real scenario, you would update the index with the embedding.
        // The FileStore manages the JSONL persistence.
    }

    // 3. Search (Simulated)
    // Brute-force search is handled by `search` method.
    // However, without a real embedding provider hooked up, `search` with a text query won't produce vectors.
    // This example focuses on the persistence aspect.
    
    println!("\n🔍 Inspecting store contents...");
    let all_docs = store.get_all().await;
    for doc in all_docs {
        println!("- [{}] {}", doc.metadata.get("tag").unwrap_or(&"?".to_string()), doc.content);
    }
    
    println!("\n✨ FileStore example complete.");
    Ok(())
}
