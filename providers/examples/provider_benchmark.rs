//! Speed comparison: Groq vs OpenAI vs Ollama
//!
//! Run with: cargo run --example provider_benchmark --features full
//!
//! Required environment variables:
//! - OPENAI_API_KEY
//! - GROQ_API_KEY
//! - Ollama server running locally

use aimaxxing_core::prelude::*;
use aimaxxing_providers::{
    openai::{OpenAI, GPT_4O_MINI},
    groq::{Groq, LLAMA_3_1_8B as GROQ_LLAMA},
    ollama::{Ollama, LLAMA_3_1_8B as OLLAMA_LLAMA},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();

    println!("🏎️  AIMAXXING Provider Speed Benchmark");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    let prompt = "What's the current trend for Solana? One sentence only.";

    // Test 1: OpenAI (Baseline)
    println!("1️⃣  Testing OpenAI (GPT-4o-mini)...");
    match test_provider("OpenAI", OpenAI::from_env()?, GPT_4O_MINI, prompt).await {
        Ok(time) => println!("   ✅ Response time: {:.2}s\n", time),
        Err(e) => println!("   ❌ Error: {}\n", e),
    }

    // Test 2: Groq (Speed King)
    println!("2️⃣  Testing Groq (Llama 3.1 8B)...");
    match test_provider("Groq", Groq::from_env()?, GROQ_LLAMA, prompt).await {
        Ok(time) => println!("   ✅ Response time: {:.2}s 🚀\n", time),
        Err(e) => println!("   ❌ Error: {} (Check GROQ_API_KEY)\n", e),
    }

    // Test 3: Ollama (Privacy King)
    println!("3️⃣  Testing Ollama (Local Llama 3.1 8B)...");
    match test_provider("Ollama", Ollama::from_env()?, OLLAMA_LLAMA, prompt).await {
        Ok(time) => println!("   ✅ Response time: {:.2}s 🔐\n", time),
        Err(e) => println!("   ❌ Error: {} (Is Ollama running?)\n", e),
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 Summary:");
    println!();
    println!("🥇 Groq     - Fastest (0.3-0.5s) | Cloud | Usage-based pricing");
    println!("🥈 OpenAI   - Reliable (1-3s)    | Cloud | Token-based pricing");
    println!("🥉 Ollama   - Private (varies)   | Local | Free, no data leak");
    println!();
    println!("💡 Recommendation:");
    println!("   • Real-time trading  → Groq (speed)");
    println!("   • Sensitive strategies → Ollama (privacy)");
    println!("   • Production stable → OpenAI (reliability)");
    println!("   • Hybrid approach → Use all three!");

    Ok(())
}

async fn test_provider<P: Provider>(
    name: &str,
    provider: P,
    model: &str,
    prompt: &str,
) -> Result<f64> {
    let agent = Agent::builder(provider)
        .model(model)
        .system_prompt("You are a concise trading analyst.")
        .build()?;

    let start = std::time::Instant::now();
    let _response = agent.prompt(prompt, None).await?;
    let elapsed = start.elapsed();

    Ok(elapsed.as_secs_f64())
}
