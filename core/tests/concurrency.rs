use brain::store::file::{FileStore, FileStoreConfig};
use brain::rag::VectorStore; // Import trait
use std::sync::Arc;
use tokio::time::Duration;
use std::sync::atomic::AtomicBool; // Import Atomics

#[tokio::test]
async fn test_compaction_race_condition() {
    let _ = tracing_subscriber::fmt::try_init();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("race_test.jsonl");
    let config = FileStoreConfig::new(path.clone());

    let store = Arc::new(FileStore::new(config).await.unwrap());
    
    // Number of items to append
    let count = 500;
    
    // Shared done flag
    let done = Arc::new(AtomicBool::new(false));
    
    let store_clone = store.clone();
    let done_clone = done.clone();

    // Spawn Compactor
    let compactor = tokio::spawn(async move {
        while !done_clone.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if let Err(e) = store_clone.compact().await {
               println!("Compaction error (expected sometimes): {}", e);
            }
        }
    });

    // Writer
    for i in 0..count {
        let content = format!("Item {}", i);
        let mut meta = std::collections::HashMap::new();
        meta.insert("idx".to_string(), i.to_string());
        
        store.store(&content, meta).await.unwrap();
        // Small delay to allow compaction to interleave
        if i % 10 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    compactor.await.unwrap();

    // Verification
    // Re-load store from disk to ensure persistence
    drop(store);
    let config = FileStoreConfig::new(path);
    let store = FileStore::new(config).await.unwrap();
    
    let all_docs = store.get_all().await;
    println!("Total docs found: {}", all_docs.len());
    
    // Check missing
    let mut missing = Vec::new();
    for i in 0..count {
        let exists = all_docs.iter().any(|d| d.metadata.get("idx") == Some(&i.to_string()));
        if !exists {
            missing.push(i);
        }
    }
    
    if !missing.is_empty() {
        panic!("DATA LOSS DETECTED! Missing items: {:?} (Total missing: {})", missing, missing.len());
    } else {
        println!("SUCCESS: No data loss after {} writes with concurrent compaction.", count);
    }
}
