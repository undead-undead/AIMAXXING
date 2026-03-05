
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use brain::store::file::{FileStore, FileStoreConfig};
    use brain::rag::VectorStore;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    let path = PathBuf::from("bug_repro.jsonl");
    if path.exists() {
        tokio::fs::remove_file(&path).await?;
    }
    if path.with_extension("index").exists() {
        tokio::fs::remove_file(path.with_extension("index")).await?;
    }

    let config = FileStoreConfig::new(path.clone());
    let store = FileStore::new(config).await?;

    // 1. Store some data
    let mut map = HashMap::new();
    map.insert("key".to_string(), "value".to_string());
    let id1 = store.store("First document", map.clone()).await?;
    let id2 = store.store("Second document", map.clone()).await?;

    println!("Stored documents: {}, {}", id1, id2);

    // 2. Read back to verify
    let docs = store.get_all().await;
    assert_eq!(docs.len(), 2, "Should have 2 docs");
    println!("Initial read successful");

    // 3. Delete one to trigger need for compaction (logic-wise, though compaction is manual)
    store.delete(&id1).await?;
    
    // 4. Trigger compaction
    println!("Compacting...");
    store.compact().await?;
    println!("Compaction done");

    // 5. Try to read the remaining document
    // This should fail if the reader is stale
    println!("Attempting to read after compaction...");
    let docs_after = store.get_all().await;
    
    // If the bug exists, this might be empty, contain garbage, or panic depending on how the OS handles reading from unlinked file with new offsets.
    // The new offest of "Second document" will be 0 (start of file).
    // The OLD file (unlinked) still exists and has "First document" at offset 0.
    // So we might actually read "First document" again but think it's "Second document" if we don't check ID!
    // Or if the old file was smaller/larger... 
    
    println!("found {} docs", docs_after.len());
    for doc in docs_after {
        println!("Found doc: {} content: {}", doc.id, doc.content);
        if doc.id == id2 && doc.content == "Second document" {
            println!("SUCCESS: Read correct document");
        } else {
            println!("FAILURE: Read wrong content or ID. Expected {}/Second document", id2);
            // If we read the first document (which was at offset 0 in old file), id will be id1.
            if doc.id == id1 {
                println!("Confirmed BUG: Read deleted document from stale file descriptor!");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
