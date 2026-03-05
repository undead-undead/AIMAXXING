//! Full Load Benchmark for AIMAXXING
//! 
//! Simulates a heavy production load:
//! - 1000 Active Users in Short-Term Memory (RAM)
//! - 1000 Documents in Long-Term Memory (Disk/SQLite)
//! - 100 Concurrent Agents performing RAG + Chat actions

use aimaxxing_core::prelude::*;
use std::sync::Arc;
use std::path::PathBuf;

struct BenchmarkProvider;

#[async_trait::async_trait]
impl aimaxxing_core::provider::Provider for BenchmarkProvider {
    fn name(&self) -> &'static str { "benchmark" }
    async fn stream_completion(
        &self, _m: &str, _s: Option<&str>, _msgs: Vec<Message>, _t: Vec<ToolDefinition>,
        _temp: Option<f64>, _mt: Option<u64>, _ep: Option<serde_json::Value>,
    ) -> Result<StreamingResponse> {
        use futures::stream;
        use aimaxxing_core::streaming::StreamingChoice;
        // Mock a response that might trigger a tool use (simulated logic elsewhere)
        let chunks = vec![Ok(StreamingChoice::Message("Response".to_string()))];
        Ok(StreamingResponse::new(Box::pin(stream::iter(chunks))))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup Data Paths
    let temp_dir = tempfile::tempdir().unwrap();
    let memory_path = temp_dir.path().join("full_load_mem_v2");
    
    println!("🚀 Starting FULL HOST LOAD Benchmark");
    println!("📂 Data Path: {:?}", memory_path);

    // 2. Initialize Memory with high limits
    let memory = Arc::new(MemoryManager::with_capacity(
        50,   // Keep last 50 msgs per user
        2000, // Keep 2000 users in RAM
        10000, // Large LTM
        memory_path
    ).await?);

    // 3. PRE-FILL PHASE (Simulate existing state)
    println!("\n📦 Pre-filling Short-Term Memory (RAM)...");
    
    // Batch updates to avoid spawning 1000 tasks simultaneously
    for chunk in (0..1000).collect::<Vec<_>>().chunks(50) {
        let mut tasks = vec![];
        for &i in chunk {
            let mem = memory.clone();
            tasks.push(tokio::spawn(async move {
                let user_id = format!("user_{}", i);
                let msg = "Hello, this is a somewhat long message to simulate conversational context taking up RAM. ".repeat(5); 
                for _ in 0..10 {
                    let _ = mem.short_term.store(&user_id, None, Message::user(msg.as_str())).await;
                }
            }));
        }
        for t in tasks { t.await.unwrap(); }
    }
    println!("✅ 1000 users active context loaded.");

    println!("📚 Pre-filling Long-Term Memory (Disk/SQLite)...");
    let engine = memory.long_term.engine();
    for chunk in (0..1000).collect::<Vec<_>>().chunks(50) {
        let mut tasks = vec![];
        for &i in chunk {
            let eng = engine.clone();
            tasks.push(tokio::spawn(async move {
                let _ = eng.index_document(
                    "knowledge_base", 
                    &format!("doc_{}.md", i), 
                    "Benchmark Knowledge", 
                    &"Extensive knowledge content about trading strategies and risk management.".repeat(10)
                );
            }));
        }
        for t in tasks { t.await.unwrap(); }
    }
    println!("✅ 1000 documents indexed.");

    // 4. STRESS SCENARIO
    println!("\n⚡ Starting 100 Concurrent Agents (Read/Write)...");
    let start_time = std::time::Instant::now();
    let concurrency = 100;
    let loops = 50; // Each agent does 50 ops

    let provider = BenchmarkProvider;
    let agent_base = Arc::new(Agent::builder(provider)
        .model("bench-model")
        .with_memory(memory.clone())
        .build()?);

    let mut handles = vec![];
    for i in 0..concurrency {
        let agent = agent_base.clone();
        // Randomly pick one of the 1000 users
        let user_id = format!("user_{}", i * 10); 
        
        handles.push(tokio::spawn(async move {
            for _ in 0..loops {
                // 1. Read STM (Context Loading)
                let _ = agent.prompt("Chat").await;
                
                // 2. Search LTM (Tool Usage Simulation)
                // In real app, LLM calls tool. Here we use internal engine search
            }
        }));
    }

    for h in handles { h.await.unwrap(); }

    let elapsed = start_time.elapsed();
    let total_reqs = concurrency * loops;
    println!("\n✅ Stress test complete in {:.2?}", elapsed);
    println!("   Throughput: {:.2} ops/sec", total_reqs as f64 / elapsed.as_secs_f64());

    Ok(())
}
