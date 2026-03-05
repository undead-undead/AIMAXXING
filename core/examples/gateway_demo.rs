//! Gateway Demo Example
//!
//! Demonstrates how to start the AIMAXXING Gateway server.
//!
//! # Run
//! ```bash
//! cargo run --example gateway_demo
//! ```
//!
//! Then open http://127.0.0.1:18888/aimaxxing/ in your browser.

use brain::bus::message_bus::{MessageBus, OutboundMessage};
use brain::infra::aimaxxing_gateway::Gateway;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("brain=info".parse()?))
        .init();

    println!("╔════════════════════════════════════════╗");
    println!("║       AIMAXXING Gateway Demo                ║");
    println!("╠════════════════════════════════════════╣");
    println!("║  Dashboard: http://127.0.0.1:18888/aimaxxing/   ║");
    println!("║  WebSocket: ws://127.0.0.1:18888/aimaxxing/ws  ║");
    println!("╚════════════════════════════════════════╝");

    // Create message bus
    let bus = MessageBus::new(100);

    // Build and run gateway
    let gateway = Gateway::builder()
        .port(18888)
        .host("127.0.0.1")
        .auth_token("password123")
        .log_to_canvas(true)
        .with_bus(bus.clone())
        .build();

    // Spawn mock agent that sends messages to the bus
    let agent_bus = bus.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let outbound = OutboundMessage::new(
                "telegram",
                "chat_123",
                "Hello! This is a proactive message from AIMAXXING Agent."
            );
            if let Err(e) = agent_bus.publish_outbound(outbound).await {
                eprintln!("Failed to publish outbound: {}", e);
            }
        }
    });

    // Run server (blocks until shutdown)
    gateway.run().await?;

    Ok(())
}
