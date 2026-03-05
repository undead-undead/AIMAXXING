use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::memory::{ShortTermMemory, Memory};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup Memory
    let memory = ShortTermMemory::new(10, 100, "test_isolation.json").await;
    let user_id = "user_verify_isolation";

    println!("--- Verifying Memory Isolation ---");

    // 2. Agent A stores a message
    println!("> Agent A storing message...");
    memory.store(user_id, Some("AgentA"), Message::user("Secret for Agent A")).await.unwrap();

    // 3. Agent B stores a message
    println!("> Agent B storing message...");
    memory.store(user_id, Some("AgentB"), Message::user("Secret for Agent B")).await.unwrap();

    // 4. Verify Retrieval
    
    // Agent A should ONLY see "Secret for Agent A"
    let history_a = memory.retrieve(user_id, Some("AgentA"), 10).await;
    if history_a.len() == 1 && history_a[0].text() == "Secret for Agent A" {
        println!("✅ Agent A isolation confirmed.");
    } else {
        println!("❌ Agent A isolation FAILED. Got: {:?}", history_a);
        std::process::exit(1);
    }

    // Agent B should ONLY see "Secret for Agent B"
    let history_b = memory.retrieve(user_id, Some("AgentB"), 10).await;
    if history_b.len() == 1 && history_b[0].text() == "Secret for Agent B" {
        println!("✅ Agent B isolation confirmed.");
    } else {
        println!("❌ Agent B isolation FAILED. Got: {:?}", history_b);
        std::process::exit(1);
    }
    
    // Global context (None) - should be empty or distinct (depending on design choice, I chose distinct)
    let history_global = memory.retrieve(user_id, None, 10).await;
    if history_global.is_empty() {
        println!("✅ Global context isolation confirmed (empty).");
    } else {
         println!("❌ Global context isolation FAILED. Got: {:?}", history_global);
         std::process::exit(1);
    }

    println!("--- Verification Success ---");
    Ok(())
}
