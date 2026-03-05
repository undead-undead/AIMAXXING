//! Resource Benchmark for AIMAXXING
//! 
//! This example simulates 100 concurrent agents to measure memory and CPU overhead.
//! Run with: /usr/bin/time -v cargo run --release --example resource_benchmark

use aimaxxing_core::prelude::*;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use std::path::PathBuf;

/// Minimal Mock Provider to isolate framework overhead from network overhead
struct BenchmarkProvider;

#[async_trait::async_trait]
impl aimaxxing_core::provider::Provider for BenchmarkProvider {
    fn name(&self) -> &'static str { "benchmark" }
    
    async fn stream_completion(
        &self,
        _model: &str,
        _system: Option<&str>,
        _messages: Vec<Message>,
        _tools: Vec<ToolDefinition>,
        _temperature: Option<f64>,
        _max_tokens: Option<u64>,
        _extra_params: Option<serde_json::Value>,
    ) -> Result<StreamingResponse> {
        // Simulate minor compute delay (inference latency)
        // sleep(Duration::from_millis(10)).await;
        
        use futures::stream;
        use aimaxxing_core::streaming::StreamingChoice;
        let chunks = vec![Ok(StreamingChoice::Message("Benchmark response".to_string()))];
        Ok(StreamingResponse::new(Box::pin(stream::iter(chunks))))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let start_time = std::time::Instant::now();
    println!("🚀 Starting AIMAXXING Resource Benchmark\n");

    // 1. Initialize Memory Manager (Disk I/O + RAM Baseline)
    let temp_dir = tempfile::tempdir().unwrap();
    let memory_path = temp_dir.path().join("bench_mem");
    
    // Simulate realistic capacity: 1000 active users in RAM cache
    let memory = Arc::new(MemoryManager::with_capacity(
        50,   // 50 msgs per user (STM context)
        1000, // 1000 concurrent users cache
        1000, // 1000 entries per doc (LTM)
        memory_path
    ).await?);

    println!("✅ Memory Initialized (1000 user capacity)");

    // 2. Spawn 100 Concurrent Agents
    // AIMAXXING is lightweight, so 100 agents usually share same memory/provider references
    // This tests the overhead of the Agent struct and Tokio tasks.
    let concurrency = 100;
    println!("⚡ Spawning {} concurrent agents...", concurrency);

    let provider = BenchmarkProvider;
    let agent_base = Agent::builder(provider)
        .model("bench-model")
        .with_memory(memory.clone())
        .build()?;
    
    // Using Arc<Agent> is typical pattern for efficient sharing
    let agent = Arc::new(agent_base);
    
    let mut handles = vec![];
    
    for i in 0..concurrency {
        let agent_ref = agent.clone();
        let user_id = format!("user_{}", i);
        
        handles.push(tokio::spawn(async move {
            // Simulate conversation flow: Write STM -> Process -> Read STM -> Write STM
            for _ in 0..10 {
                let _ = agent_ref.prompt("Benchmark request").await;
            }
        }));
    }

    // Wait for all tasks
    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = start_time.elapsed();
    println!("\n✅ Benchmark execution complete in {:.2?}", elapsed);
    println!("   Throughput: {:.2} req/sec", (concurrency * 10) as f64 / elapsed.as_secs_f64());
    
    // Keep alive for external monitoring if needed
    // sleep(Duration::from_secs(5)).await;

    Ok(())
}
