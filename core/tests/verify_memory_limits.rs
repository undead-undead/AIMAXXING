use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::memory::ShortTermMemory;

#[tokio::test]
async fn verify_short_term_memory_limits() {
    let path = "test_limits.json";
    if std::path::Path::new(path).exists() {
        let _ = std::fs::remove_file(path);
    }

    // Create memory with limit of 3 users
    let memory = ShortTermMemory::new(10, 3, path).await;
    
    // Add 3 users
    memory.store("user1", None, Message::user("Hi")).await.unwrap();
    memory.store("user2", None, Message::user("Hi")).await.unwrap();
    memory.store("user3", None, Message::user("Hi")).await.unwrap();
    
    assert_eq!(memory.message_count("user1", None), 1);
    assert_eq!(memory.message_count("user2", None), 1);
    assert_eq!(memory.message_count("user3", None), 1);
    
    // Add 4th user - should evict one of the others
    // Sleep briefly to ensure timestamps are distinct if resolution is low
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    memory.store("user4", None, Message::user("Hi")).await.unwrap();
    
    // Check who was evicted.
    // Since we just added them sequentially, and `store` updates access time, 
    // user1 is likely the oldest access (or close to it).
    // The implementation iterates safely. We just verify SOMEONE is gone.
    
    let count1 = memory.message_count("user1", None);
    let count2 = memory.message_count("user2", None);
    let count3 = memory.message_count("user3", None);
    let count4 = memory.message_count("user4", None);
    
    let total_active_users = (if count1 > 0 { 1 } else { 0 }) +
                             (if count2 > 0 { 1 } else { 0 }) +
                             (if count3 > 0 { 1 } else { 0 }) +
                             (if count4 > 0 { 1 } else { 0 });
                             
    assert!(total_active_users <= 3, "Failed to enforce max user limit. Active users: {}", total_active_users);
    assert_eq!(count4, 1, "New user was not added");
}
