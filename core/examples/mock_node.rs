//! Mock Messaging Node Example
//!
//! Demonstrates how an external messaging bridge (e.g., Telegram Bot)
//! connects to the AIMAXXING Gateway to exchange messages.

use aimaxxing_core::bus::message_bus::{InboundMessage, OutboundMessage};
use aimaxxing_core::infra::aimaxxing_gateway::protocol::{ClientMessage, ServerMessage, ClientRole};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = "ws://127.0.0.1:18888/aimaxxing/ws";
    println!("Connecting to Gateway: {}", url);

    let (mut ws_stream, _) = connect_async(url).await?;
    println!("WebSocket connected!");

    // 1. Handshake
    let auth = ClientMessage::Connect {
        role: ClientRole::Node,
        token: "password123".to_string(),
    };
    ws_stream.send(Message::Text(serde_json::to_string(&auth)?.into())).await?;

    // 2. Wait for AuthSuccess
    if let Some(Ok(Message::Text(text))) = ws_stream.next().await {
        let resp: ServerMessage = serde_json::from_str(&text)?;
        match resp {
            ServerMessage::AuthSuccess { role } => {
                println!("Successfully authenticated as {:?}", role);
            }
            _ => {
                println!("Expected AuthSuccess, got: {:?}", resp);
                return Ok(());
            }
        }
    }

    // 3. Send an Inbound Message (Simulate user message from Telegram)
    let inbound = InboundMessage::new("telegram", "user_999", "chat_123", "Hello from Mock Telegram Node!");
    let msg = ClientMessage::Inbound { message: inbound };
    ws_stream.send(Message::Text(serde_json::to_string(&msg)?.into())).await?;
    println!("Sent inbound message to bus.");

    // 4. Listen for Outbound Messages (Bridge Bus -> Node)
    println!("Listening for messages from agents...");
    while let Some(Ok(msg)) = ws_stream.next().await {
        if let Message::Text(text) = msg {
            if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                match server_msg {
                    ServerMessage::Outbound { message } => {
                        println!(">>> RECEIVED OUTBOUND: [{}] {}: {}", 
                            message.channel, message.chat_id, message.content);
                    }
                    ServerMessage::Status(_) => {
                        // Ignore status for this demo
                    }
                    _ => println!("Received message: {:?}", server_msg),
                }
            }
        }
    }

    Ok(())
}
