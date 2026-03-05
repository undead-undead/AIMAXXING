use aimaxxing_core::memory::ShortTermMemory;
use aimaxxing_core::memory::LongTermMemory;
use aimaxxing_core::memory::Memory;
use aimaxxing_core::message::Message;
use std::time::Duration;
use std::path::PathBuf;

#[tokio::test]
async fn verify_short_term_memory_pruning() {
    let mem = ShortTermMemory::new(10);
    
    // 1. Store item
    mem.store("user1", None, Message::user("hello")).await.unwrap();
    
    // 2. Wait
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 3. Prune with small timeout (e.g. 10ms)
    // The previous item is 100ms old, so it SHOULD be pruned.
    mem.prune_inactive(Duration::from_millis(10));
    
    // 4. Verify empty
    let history = mem.retrieve("user1", None, 100).await;
    assert!(history.is_empty(), "ShortTermMemory should have been pruned! (History len: {})", history.len());
}

#[tokio::test]
async fn verify_long_term_memory_pruning_optimization() {
    let path = PathBuf::from("test_ltm_prune_opt.jsonl");
    if path.exists() { 
        std::fs::remove_file(&path).ok(); 
        std::fs::remove_file(path.with_extension("index")).ok(); 
        std::fs::remove_file(path.with_extension("index_v2")).ok();
    }
    
    let mem = LongTermMemory::new(100, path.clone()).await.expect("Failed to create LTM");
    
    // 1. Store 10 items
    for i in 0..10 {
        mem.store("user1", None, Message::user(format!("msg {}", i))).await.unwrap();
    }
    
    // 2. Prune to keep 5
    mem.prune(5, "user1".to_string(), None).await;
    
    // Allow background task to finish pruning
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    
    // 3. Retrieve
    let recent = mem.retrieve_recent("user1", None, 100).await;
    assert_eq!(recent.len(), 5, "Should have pruned to 5 items");
    
    // Cleanup
    if path.exists() { 
        std::fs::remove_file(&path).ok(); 
        std::fs::remove_file(path.with_extension("index")).ok(); 
        std::fs::remove_file(path.with_extension("index_v2")).ok();
    }
}
