use brain::store::file::{FileStore, FileStoreConfig};
use brain::rag::{VectorStore, Embeddings};
use brain::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;

struct MockEmbedder;
#[async_trait]
impl Embeddings for MockEmbedder {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        // Return unit vector so dot product is always 1.0 (if stored is unit)
        // or just return something consistent.
        let mut v = vec![0.0; 1536];
        v[0] = 1.0; 
        Ok(v)
    }
}

#[tokio::test]
async fn test_time_travel_search() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("time_travel.jsonl");
    let store = FileStore::new(FileStoreConfig::new(db_path)).await.unwrap()
        .with_embedder(Arc::new(MockEmbedder));

    let mut v = vec![0.0; 1536]; v[0] = 1.0;
    let embed_json = serde_json::to_string(&v).unwrap();

    // Helper to store doc with timestamp
    let store_doc = |id: &str, ts: i64| {
        let store = store.clone();
        let e_json = embed_json.clone();
        let id = id.to_string();
        async move {
            let mut meta = HashMap::new();
            meta.insert("_embedding".to_string(), e_json);
            meta.insert("timestamp".to_string(), ts.to_string());
            meta.insert("content".to_string(), id.clone()); // Store ID in metadata for easy check
            store.store(&id, meta).await.unwrap();
        }
    };

    // 1. Insert docs at different times
    store_doc("DocA", 100).await;
    store_doc("DocB", 200).await;
    store_doc("DocC", 300).await;

    // 2. Search snapshot at T=150 (Should find DocA only)
    let res = store.search_snapshot("query", 150, 10).await.unwrap();
    assert_eq!(res.len(), 1);
    assert!(res.iter().any(|d| d.content == "DocA"));

    // 3. Search snapshot at T=250 (Should find DocA and DocB)
    let res = store.search_snapshot("query", 250, 10).await.unwrap();
    assert_eq!(res.len(), 2);
    assert!(res.iter().any(|d| d.content == "DocA"));
    assert!(res.iter().any(|d| d.content == "DocB"));
    assert!(!res.iter().any(|d| d.content == "DocC"));

    // 4. Search snapshot at T=400 (Should find all)
    let res = store.search_snapshot("query", 400, 10).await.unwrap();
    assert_eq!(res.len(), 3);
}
