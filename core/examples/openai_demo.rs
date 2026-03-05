//! OpenAI Compatibility Demo
//!
//! This example starts the AIMAXXING Gateway and tests the OpenAI-compatible API.

use aimaxxing_core::infra::aimaxxing_gateway::Gateway;
use aimaxxing_core::bus::message_bus::MessageBus;
use tokio::time::{sleep, Duration};
use reqwest::Client;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting OpenAI Compatibility Demo...");

    let bus = MessageBus::new(100);
    let gateway = Gateway::builder()
        .port(18889) // Use a different port to avoid conflicts
        .auth_token("secret_token_123")
        .with_bus(bus.clone())
        .build();

    // Spawn gateway in background
    tokio::spawn(async move {
        if let Err(e) = gateway.run().await {
            eprintln!("Gateway error: {}", e);
        }
    });

    sleep(Duration::from_secs(2)).await;

    // Simulate an agent listening on the bus
    let agent_bus = bus.clone();
    tokio::spawn(async move {
        println!("🤖 Mock Agent waiting for messages...");
        loop {
            if let Ok(inbound) = agent_bus.consume_inbound().await {
                println!("🤖 Agent received: {}", inbound.content);
                
                // Simulate processing time
                sleep(Duration::from_millis(500)).await;
                
                // Prepare response
                let mut outbound = aimaxxing_core::bus::message_bus::OutboundMessage::new(
                    &inbound.channel,
                    &inbound.chat_id,
                    format!("Response from agent {}: I received your message '{}'", inbound.chat_id, inbound.content)
                );
                
                // CRITICAL: Echo the request_id for correlation
                if let Some(req_id) = inbound.request_id {
                    outbound = outbound.with_request_id(req_id);
                }
                
                if let Err(e) = agent_bus.publish_outbound(outbound).await {
                    eprintln!("Failed to publish outbound: {}", e);
                }
            }
        }
    });

    // Test 1: List Models
    println!("\nTest 1: GET /v1/models");
    let client = Client::new();
    let resp = client.get("http://127.0.0.1:18889/v1/models")
        .header("Authorization", "Bearer secret_token_123")
        .send()
        .await?;
    
    println!("Status: {}", resp.status());
    println!("Response: {}", resp.text().await?);

    // Test 2: Chat Completion
    println!("\nTest 2: POST /v1/chat/completions");
    let start = std::time::Instant::now();
    let resp = client.post("http://127.0.0.1:18889/v1/chat/completions")
        .header("Authorization", "Bearer secret_token_123")
        .json(&json!({
            "model": "helpful-agent",
            "messages": [
                {"role": "user", "content": "Ping AIMAXXING Gateway"}
            ]
        }))
        .send()
        .await?;

    println!("Status: {}", resp.status());
    let elapsed = start.elapsed();
    let body = resp.text().await?;
    println!("Latency: {:?}", elapsed);
    println!("Response: {}", body);

    // Test 3: Unauthorized
    println!("\nTest 3: POST /v1/chat/completions (Unauthorized)");
    let resp = client.post("http://127.0.0.1:18889/v1/chat/completions")
        .header("Authorization", "Bearer wrong_token")
        .json(&json!({
            "model": "helpful-agent",
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .send()
        .await?;
    println!("Status: {} (Expected 401)", resp.status());

    println!("\n✅ OpenAI Compatibility Demo Finished Successfully!");
    
    Ok(())
}
