/// Basic example of creating and using an AIMAXXING agent
/// 
/// This example demonstrates:
/// - Creating an OpenAI provider (standard for AIMAXXING)
/// - Building an agent with custom configuration using the builder pattern
/// - Simple prompt-response interaction
///
/// Prerequisite:
/// - Set `OPENAI_API_KEY` in your environment or .env file

use aimaxxing_core::prelude::*;
use aimaxxing_providers::openai::{OpenAI, GPT_4O};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for logs
    tracing_subscriber::fmt::init();

    // 1. Create provider (requires OPENAI_API_KEY env var)
    let provider = OpenAI::from_env()?;

    // 2. Build agent with configuration
    let agent = Agent::builder(provider)
        .model(GPT_4O)
        .preamble("You are a helpful AI assistant.")
        .temperature(0.7)
        .max_tokens(1000)
        .build()?;

    // 3. Simple interaction
    println!("Agent: Hello! I'm ready to help.");
    
    let response = agent.prompt("What is Rust and why is it good for AI agents?", None).await?;
    println!("\nAgent: {}", response);

    Ok(())
}
