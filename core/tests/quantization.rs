use brain::store::file::{FileStore, FileStoreConfig};
use brain::rag::VectorStore;
use std::collections::HashMap;
use tempfile::tempdir;

#[tokio::test]
async fn test_quantized_search() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("quant_test.jsonl");
    let store = FileStore::new(FileStoreConfig::new(db_path)).await.unwrap();

    // 1. Create two orthogonal vectors and one close to the first
    // V1: [1.0, 0.0, ...] -> Quantized: [127, 0, ...]
    // V2: [0.0, 1.0, ...] -> Quantized: [0, 127, ...]
    
    let mut v1 = vec![0.0; 1536]; v1[0] = 1.0;
    let mut v2 = vec![0.0; 1536]; v2[1] = 1.0;
    
    // Store Doc 1 (Target)
    let mut meta1 = HashMap::new();
    meta1.insert("_embedding".to_string(), serde_json::to_string(&v1).unwrap());
    store.store("Doc 1", meta1).await.unwrap();

    // Store Doc 2 (Distractor)
    let mut meta2 = HashMap::new();
    meta2.insert("_embedding".to_string(), serde_json::to_string(&v2).unwrap());
    store.store("Doc 2", meta2).await.unwrap();

    // 2. Search with vector close to V1
    // Query: [0.9, 0.1, ...]
    // Should match Doc 1 with high score, Doc 2 with low score
    
    // Note: Since we don't expose raw search with vector in trait, we rely on the internal logic.
    // But `FileStore` calculates embedding from query string via embedder usually.
    // However, `FileStore` implementation of `search` calls `embedder.embed(query)`.
    // Since we didn't provide an embedder, it returns zero vector!
    // Wait, the implementation says: 
    // `if let Some(embedder) = ... else { vec![0.0; 1536] }`
    
    // This makes testing `search` hard without an embedder mock.
    // BUT! I can inject a Mock Embedder.
    
    // Let's create a Mock Embedder
    use async_trait::async_trait;
    use brain::rag::Embeddings;
    use brain::error::Result;
    use std::sync::Arc;

    struct MockEmbedder;
    #[async_trait]
    impl Embeddings for MockEmbedder {
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            let mut v = vec![0.0; 1536];
            if text == "query_v1" {
                v[0] = 1.0;
            } else if text == "query_v2" {
                v[1] = 1.0;
            }
            Ok(v)
        }
    }

    let store_with_embedder = store.with_embedder(Arc::new(MockEmbedder));

    // Search for V1
    let results = store_with_embedder.search("query_v1", 2).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].content, "Doc 1");
    // Cosine of [127, 0...] and [127, 0...] is 1.0
    println!("Score 1: {}", results[0].score);
    assert!(results[0].score > 0.95);

    // Search for V2
    let results = store_with_embedder.search("query_v2", 2).await.unwrap();
    assert_eq!(results[0].content, "Doc 2");
    println!("Score 2: {}", results[0].score);
    assert!(results[0].score > 0.95);
}
