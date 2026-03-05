/// Example: Agent with Tracing
/// 
/// This example demonstrates how to enable tracing for AIMAXXING agents.
/// Tracing provides:
/// - Execution flow visibility
/// - Performance metrics
/// - Debugging information
/// 
/// Run with different log levels:
/// ```bash
/// RUST_LOG=info cargo run --example tracing_agent
/// RUST_LOG=debug cargo run --example tracing_agent
/// ```

use brain::prelude::*;
use providers::gemini::{Gemini, GEMINI_2_0_FLASH};
use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber
    // This will output logs to stdout
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)  // Don't show module paths
        .pretty()            // Pretty formatting for development
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    info!("🚀 AIMAXXING Agent with Tracing");
    info!("========================\n");

    // Create provider
    let provider = Gemini::from_env()?;

    // Build agent (tracing is automatically enabled)
    let agent = Agent::builder(provider)
        .model(GEMINI_2_0_FLASH)
        .preamble("You are a helpful AI assistant.")
        .build()?;

    // Example 1: Simple prompt
    info!("=== Example 1: Simple Prompt ===");
    let response = agent.prompt("What is Rust?", None).await?;
    println!("Response: {}\n", response);

    // Example 2: Multi-turn conversation
    info!("=== Example 2: Multi-turn Conversation ===");
    let messages = vec![
        Message::user("I'm learning Rust"),
        Message::assistant("That's great! Rust is a powerful systems programming language."),
        Message::user("What makes it special?"),
    ];
    let response = agent.chat(messages, None).await?;
    println!("Response: {}\n", response);

    info!("✅ All examples completed");

    Ok(())
}
