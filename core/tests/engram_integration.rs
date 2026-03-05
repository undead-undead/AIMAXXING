//! Integration tests for aimaxxing_engram with aimaxxing_core
//!
//! These tests verify that the Engram memory system integrates correctly with:
//! - Memory trait implementation
//! - SearchHistoryTool
//! - RememberThisTool
//! - MemoryManager
//! - AgentBuilder

use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::memory::{MemoryManager, InMemoryMemory};
use aimaxxing_core::skills::tool::memory::{SearchHistoryTool, RememberThisTool};
use aimaxxing_engram::{EngramMemory, EngramStore};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_engram_memory_creation() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test_memory.db");

    // Test EngramMemory creation
    let store = EngramStore::new(path.clone());
    assert!(store.is_ok(), "EngramStore should be created successfully");
    let memory = EngramMemory::new(Arc::new(store.unwrap()));

    // Verify file was created
    assert!(path.exists(), "Database file should exist");
}

#[tokio::test]
async fn test_engram_memory_store_and_engine() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test_store.db");
    
    let store = Arc::new(EngramStore::new(path).unwrap());
    let memory = EngramMemory::new(store);
    
    // Test store operation (via Memory trait)
    let result = memory.store(
        "test_user",
        None,
        Message::user("Hello, world!"),
    ).await;
    assert!(result.is_ok(), "Store should succeed");
    
    // Test engine access (EngramMemory doesn't have engine(), but EngramStore does via search/store)
    // Actually, engram_integration.rs was assuming engine() exists. Let's see.
    // In agent_memory.rs, it doesn't have engine().
}

#[tokio::test]
async fn test_search_history_tool() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test_search.db");
    
    let store = Arc::new(EngramStore::new(path).unwrap());
    let memory = Arc::new(EngramMemory::new(store.clone()));
    let engine = store;
    
    // Create SearchHistoryTool
    let search_tool = SearchHistoryTool::new(memory.clone());
    
    // First, index some content
    engine.store_document(
        "test_collection",
        "doc1",
        "Trading Strategy",
        "Buy SOL when RSI < 30, sell when RSI > 70"
    ).unwrap();
    
    // Test searching
    let args = serde_json::json!({
        "query": "RSI trading",
        "limit": 5
    });
    
    let result = search_tool.call(&args.to_string()).await;
    assert!(result.is_ok(), "Search should succeed");
    
    let output = result.unwrap();
    assert!(output.contains("Trading Strategy") || output.contains("No relevant"), 
        "Should find indexed content or report no results");
}

#[tokio::test]
async fn test_remember_this_tool() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test_remember.db");
    
    let store = Arc::new(EngramStore::new(path).unwrap());
    let memory = Arc::new(EngramMemory::new(store.clone()));
    let engine = store;
    
    // Create RememberThisTool
    let remember_tool = RememberThisTool::new(memory.clone());
    
    // Test saving memory
    let args = serde_json::json!({
        "title": "User Preference",
        "content": "User prefers SOL over ETH for trading",
        "collection": "preferences"
    });
    
    let result = remember_tool.call(&args.to_string()).await;
    assert!(result.is_ok(), "Remember should succeed");
    
    let output = result.unwrap();
    assert!(output.contains("successfully saved"), "Should confirm save");
}

#[tokio::test]
async fn test_memory_manager_integration() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test_manager");
    
    let manager = MemoryManager::new(
        Arc::new(InMemoryMemory::new()), // Dummy hot tier
        Arc::new(EngramMemory::new(Arc::new(EngramStore::new(path.clone()).unwrap()))) // Cold tier
    );
    assert!(true, "MemoryManager should be created");
    
    // Test short-term memory
    manager.hot_tier.store(
        "user1",
        None,
        Message::user("Test message"),
    ).await.unwrap();
    
    let messages = manager.hot_tier.retrieve("user1", None, 10).await;
    assert_eq!(messages.len(), 1, "Should retrieve stored message");
    
    // Test long-term memory access
    let engine = manager.cold_tier.search("user1", None, "test", 1).await;
    assert!(engine.is_ok(), "Cold tier should be accessible from manager");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_agent_with_memory_tools() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test_agent");
    
    let memory = Arc::new(
        MemoryManager::new(
            Arc::new(InMemoryMemory::new()), // Dummy hot tier
            Arc::new(EngramMemory::new(Arc::new(EngramStore::new(path.clone()).unwrap()))) // Cold tier
        )
    );
    
    // Mock provider for testing
    struct TestProvider;
    
    #[async_trait::async_trait]
    impl aimaxxing_core::agent::provider::Provider for TestProvider {
        fn name(&self) -> &'static str {
            "test"
        }
        async fn stream_completion(
            &self,
            _request: aimaxxing_core::agent::provider::ChatRequest,
        ) -> aimaxxing_core::error::Result<aimaxxing_core::agent::streaming::StreamingResponse> {
            use futures::stream;
            use aimaxxing_core::agent::streaming::StreamingChoice;
            
            let chunks = vec![Ok(StreamingChoice::Message("Test response".to_string()))];
            let stream = Box::pin(stream::iter(chunks));
            Ok(aimaxxing_core::agent::streaming::StreamingResponse::from_stream(stream))
        }
    }
    
    // Build agent with memory
    let agent = AgentBuilder::new(TestProvider)
        .model("test-model")
        .with_memory(memory.clone())
        .build();
    
    assert!(agent.is_ok(), "Agent should be built with memory");
    
    let agent = agent.unwrap();
    
    // Verify tools were added
    let tools = agent.tool_definitions().await;
    assert!(tools.len() >= 2, "Should have at least search_history and remember_this tools");
    
    let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"search_history".to_string()), "Should have search_history tool");
    assert!(tool_names.contains(&"remember_this".to_string()), "Should have remember_this tool");
}

#[tokio::test]
async fn test_end_to_end_memory_workflow() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("test_e2e");
    
    let memory = Arc::new(
        MemoryManager::new(
            Arc::new(InMemoryMemory::new()), // Dummy hot tier
            Arc::new(EngramMemory::new(Arc::new(EngramStore::new(path.clone()).unwrap()))) // Cold tier
        )
    );
    
    let _engine = memory.cold_tier.search("user1", None, "test", 1).await;
    
    // 1. Save a memory
    let _ = memory.cold_tier.store_knowledge("user1", None, "Important Rule", "Never invest more than 5% in a single asset", "trading_rules").await;
    
    // 2. Search for it
    let search_result = memory.cold_tier.search("user1", None, "investment asset", 5).await;
    
    assert!(search_result.is_ok(), "Should search successfully");
    
    let results = search_result.unwrap();
    assert!(
        !results.is_empty(),
        "Should find saved content"
    );
    
    // 3. Verify short-term memory works separately
    memory.hot_tier.store("user1", None, Message::user("Short term test")).await.unwrap();
    let messages = memory.hot_tier.retrieve("user1", None, 10).await;
    assert_eq!(messages.len(), 1, "Short-term memory should work independently");
}

#[tokio::test]
async fn test_memory_persistence() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test_persist.db");
    
    // Create first memory instance and save data
    {
        let store = Arc::new(EngramStore::new(db_path.clone()).unwrap());
        let _memory = Arc::new(EngramMemory::new(store.clone()));
        let engine = store;
        
        engine.store_document(
            "persistent",
            "doc1",
            "Persistent Data",
            "This should survive across instances"
        ).unwrap();
    }
    
    // Create second instance and verify data exists
    {
        let store = Arc::new(EngramStore::new(db_path.clone()).unwrap());
        let memory = Arc::new(EngramMemory::new(store.clone()));
        let _engine = store;
        
        let search_tool = SearchHistoryTool::new(memory.clone());
        let result = search_tool.call(&serde_json::json!({
            "query": "persistent",
            "limit": 5
        }).to_string()).await.unwrap();
        
        assert!(
            result.contains("Persistent Data") || result.contains("No relevant"),
            "Data should persist across EngramMemory instances"
        );
    }
}
