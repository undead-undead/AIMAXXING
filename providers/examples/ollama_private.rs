//! Ollama provider example - Private local trading agent
//!
//! Run with: cargo run --example ollama_private --features ollama
//!
//! Prerequisites:
//! 1. Install Ollama: https://ollama.ai
//! 2. Pull a model: ollama pull llama3.1:8b
//! 3. Start Ollama server (usually auto-starts)

use brain::prelude::*;
use providers::ollama::{Ollama, LLAMA_3_1_8B};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Create Ollama provider (local & private!)
    let provider = Ollama::from_env()?;

    // Build a private trading agent
    let agent = Agent::builder(provider)
        .model(LLAMA_3_1_8B)
        .system_prompt(
            "You are a private trading strategy analyst. \
             All conversations are confidential and never leave this machine. \
             Analyze trading strategies for Solana DeFi protocols."
        )
        .max_history_messages(10)
        .build()?;

    println!("🔐 Ollama Private Trading Agent");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Model: {} (Local)", LLAMA_3_1_8B);
    println!("Privacy: 100% - No data leaves your machine");
    println!("Cost: $0 - Unlimited usage");
    println!();

    // Example: Discuss proprietary trading strategy
    let response = agent
        .prompt(
            "I'm developing a MEV arbitrage strategy on Solana. \
             Should I focus on Jupiter swaps or Orca pools? \
             Consider slippage and gas costs.",
             None
        )
        .await?;

    println!("🤖 Agent Response:");
    println!("{}", response);
    println!();
    
    println!("💡 Ollama advantages:");
    println!("   ✅ Complete privacy - protect your alpha");
    println!("   ✅ Zero API costs - unlimited queries");
    println!("   ✅ No rate limits - query as much as needed");
    println!("   ✅ Works offline - no internet required");
    println!();
    
    println!("📊 Recommended models for trading:");
    println!("   • llama3.1:8b   - Fast, balanced");
    println!("   • llama3.1:70b  - Most capable (needs GPU)");
    println!("   • mistral:7b    - Good for analysis");
    println!("   • qwen2.5:7b    - Excellent reasoning");
    println!();
    
    println!("🛠️  Setup tips:");
    println!("   1. ollama pull llama3.1:8b");
    println!("   2. Set OLLAMA_BASE_URL if needed");
    println!("   3. Use GPU for faster inference");

    Ok(())
}
