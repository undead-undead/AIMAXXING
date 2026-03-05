//! Groq provider example - Ultra-fast inference for trading
//!
//! Run with: cargo run --example groq_trading --features groq
//!
//! Set GROQ_API_KEY environment variable before running.

use aimaxxing_core::prelude::*;
use aimaxxing_providers::groq::{Groq, LLAMA_3_3_70B};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Create Groq provider (ultra-fast!)
    let provider = Groq::from_env()?;

    // Build a trading agent with Groq's speed advantage
    let agent = Agent::builder(provider)
        .model(LLAMA_3_3_70B)
        .system_prompt(
            "You are a high-frequency trading analyst. \
             Provide fast, concise market analysis for Solana (SOL). \
             Focus on actionable insights."
        )
        .max_history_messages(5) // Keep it lean for speed
        .build()?;

    println!("🚀 Groq Trading Agent (Ultra-Fast Mode)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Model: {} (Groq)", LLAMA_3_3_70B);
    println!("Speed: ~0.5s response time 🔥");
    println!();

    // Simulate real-time trading decision
    let start = std::time::Instant::now();
    
    let response = agent
        .prompt("SOL just dropped 5% in 30 seconds. Quick analysis - buy, sell, or hold?", None)
        .await?;

    let elapsed = start.elapsed();

    println!("🤖 Agent Response:");
    println!("{}", response);
    println!();
    println!("⚡ Response time: {:?}", elapsed);
    println!();
    println!("💡 Groq's speed advantage:");
    println!("   - 18x faster than GPT-4");
    println!("   - Perfect for real-time trading decisions");
    println!("   - Same OpenAI-compatible API");

    Ok(())
}
