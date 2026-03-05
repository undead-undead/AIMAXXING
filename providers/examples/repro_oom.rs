use brain::store::file::{FileStore, FileStoreConfig};
use brain::rag::VectorStore;
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from("repro_oom.jsonl");
    // Ensure clean state
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("index")).ok();
    }

    let config = FileStoreConfig::new(path.clone());
    let store = FileStore::new(config).await?;

    println!("⚠️  Generating 10,000 dummy vectors (float32 x 1536)...");
    
    // Create a large number of entries to make memory usage visible
    // 10,000 * 1536 * 4 bytes = ~60 MB raw data
    // In Box<[f32]>, it's compact.
    // In search, it gets CLONED.
    
    let mut tasks = vec![];
    for i in 0..10_000 {
        let store = store.clone();
        tasks.push(tokio::spawn(async move {
            let mut metadata = std::collections::HashMap::new();
            metadata.insert("_embedding".to_string(), serde_json::to_string(&vec![0.1; 1536]).unwrap());
            store.store(&format!("doc {}", i), metadata).await.unwrap();
        }));
    }
    
    for t in tasks {
        t.await?;
    }
    
    println!("✅ Stored 10k documents. forcing snapshot...");
    // Force snapshot/load logic if needed, but it's in memory now.
    
    println!("🔍 Starting search bomb...");
    let start = Instant::now();
    
    // This call triggers the deep clone of ALL 10k vectors (60MB -> 120MB spike)
    // If we had 1M vectors, 6GB -> 12GB spike.
    let _results = store.search("test query", 5).await?;
    
    println!("❌ Search finished in {:?}. If you see this, we didn't crash (yet), but check memory usage!", start.elapsed());

    // Clean up
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("index")).ok();
    }
    
    Ok(())
}
