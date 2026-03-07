use brain::bus::{InboundMessage, MessageBus, OutboundMessage};
use brain::config::SlackConfig;
use brain::error::{Error, Result};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};

pub struct SlackConnector {
    config: SlackConfig,
    client: Client,
}

impl SlackConnector {
    pub fn try_new(config: SlackConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                brain::error::Error::Internal(format!("Failed to build HTTP client: {}", e))
            })?;

        Ok(Self { config, client })
    }

    async fn get_socket_mode_url(&self) -> Result<String> {
        let app_token = self.config.app_token.as_ref().ok_or_else(|| {
            Error::Internal("Slack App Token is required for Socket Mode".to_string())
        })?;

        let res = self.client
            .post("https://slack.com/api/apps.connections.open")
            .header("Authorization", format!("Bearer {}", app_token))
            .header("Content-type", "application/x-www-form-urlencoded")
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Failed to open Slack connection: {}", e)))?;

        let body: Value = res.json().await
            .map_err(|e| Error::Internal(format!("Failed to parse Slack response: {}", e)))?;

        if body["ok"].as_bool().unwrap_or(false) {
            Ok(body["url"].as_str().unwrap_or_default().to_string())
        } else {
            Err(Error::Internal(format!("Slack API error: {}", body["error"])))
        }
    }

    async fn run_socket_mode(&self, bus: Arc<MessageBus>) -> Result<()> {
        loop {
            let url = match self.get_socket_mode_url().await {
                Ok(u) => u,
                Err(e) => {
                    error!("Failed to get Slack Socket Mode URL: {}. Retrying in 10s...", e);
                    sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };

            info!("Connecting to Slack Socket Mode: {}", url);
            let (mut ws_stream, _) = match connect_async(&url).await {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to connect to Slack WebSocket: {}. Retrying...", e);
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };

            info!("Slack Socket Mode connected.");

            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        let v: Value = serde_json::from_str(&text).unwrap_or_default();
                        let msg_type = v["type"].as_str().unwrap_or_default();

                        match msg_type {
                            "hello" => info!("Slack greeted us: hello"),
                            "events_api" => {
                                // Acknowledge the event
                                let envelope_id = v["envelope_id"].as_str().unwrap_or_default();
                                let ack = json!({ "envelope_id": envelope_id });
                                let _ = ws_stream.send(Message::Text(ack.to_string().into())).await;

                                // Process the event
                                self.handle_payload(v["payload"].clone(), &bus).await;
                            }
                            "disconnect" => {
                                warn!("Slack requested disconnect. Reconnecting...");
                                break;
                            }
                            _ => {}
                        }
                    }
                    Ok(Message::Close(_)) => {
                        warn!("Slack WebSocket closed. Reconnecting...");
                        break;
                    }
                    Err(e) => {
                        error!("Slack WebSocket error: {}. Reconnecting...", e);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    async fn handle_payload(&self, payload: Value, bus: &MessageBus) {
        let event_type = payload["event"]["type"].as_str().unwrap_or_default();
        
        if event_type == "message" {
            let text = payload["event"]["text"].as_str().unwrap_or("");
            let user_id = payload["event"]["user"].as_str().unwrap_or("unknown");
            let channel_id = payload["event"]["channel"].as_str().unwrap_or_default();
            let bot_id = payload["event"]["bot_id"].as_str();

            // Ignore messages from bots (including ourselves)
            if bot_id.is_some() {
                return;
            }

            if !text.is_empty() && !channel_id.is_empty() {
                info!("Slack received message from {}: {}", user_id, text);
                
                let inbound = InboundMessage::new(
                    "slack",
                    user_id,
                    channel_id,
                    text
                );
                
                if let Err(e) = bus.publish_inbound(inbound).await {
                    error!("Failed to publish inbound Slack message: {}", e);
                }
            }
        }
    }
}

#[async_trait]
impl super::Connector for SlackConnector {
    fn name(&self) -> &str {
        "slack"
    }

    fn metadata() -> super::ChannelMetadata {
        super::ChannelMetadata {
            id: "slack".to_string(),
            name: "Slack".to_string(),
            description: "Bi-directional Slack communication via Socket Mode or Events API".to_string(),
            icon: "💬".to_string(),
            fields: vec![
                super::ChannelField {
                    key: "SLACK_BOT_TOKEN".to_string(),
                    label: "Bot Token".to_string(),
                    field_type: "password".to_string(),
                    description: "xoxb-... token from Slack App settings".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "SLACK_APP_TOKEN".to_string(),
                    label: "App Level Token".to_string(),
                    field_type: "password".to_string(),
                    description: "xapp-... token (required for Socket Mode)".to_string(),
                    required: false,
                },
                super::ChannelField {
                    key: "SLACK_VERIFICATION_TOKEN".to_string(),
                    label: "Verification Token".to_string(),
                    field_type: "password".to_string(),
                    description: "For Webhook/Events API mode".to_string(),
                    required: false,
                }
            ],
        }
    }

    async fn start(&self, bus: Arc<MessageBus>) -> Result<()> {
        let bus_clone = bus.clone();

        // Handle outbound messages
        let mut outbound_rx = bus.subscribe_outbound();
        let this = Arc::new(Self {
            config: self.config.clone(),
            client: self.client.clone(),
        });
        
        let outbound_this = this.clone();
        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel == "slack" || msg.channel == "broadcast" {
                    if let Err(e) = outbound_this.send(msg).await {
                        error!("Slack send error: {}", e);
                    }
                }
            }
        });

        if self.config.app_token.is_some() {
            info!("Slack Connector starting in Socket Mode...");
            self.run_socket_mode(bus_clone).await?;
        } else {
            info!("Slack Connector starting in Webhook Mode...");
            let mut webhook_rx = bus.subscribe_webhook_event();
            while let Ok(event) = webhook_rx.recv().await {
                if event.connector_id == "slack" {
                    self.handle_payload(event.payload, &bus).await;
                }
            }
        }
        
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        let url = "https://slack.com/api/chat.postMessage";

        // Logic for broadcast: we might need a default channel or a way to discover active channels
        // For now, if chat_id is "all", we skip unless we have a specific broadcast target.
        // Actually, broadcast is usually used for pre-configured channels.
        let target_channel = if message.chat_id == "all" {
             // In Slack, we don't have an "all" channel, so we might want to skip or use a default
             return Ok(());
        } else {
            &message.chat_id
        };

        let res = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .json(&json!({
                "channel": target_channel,
                "text": message.content
            }))
            .send()
            .await
            .map_err(|e| Error::Internal(format!("Slack HTTP error: {}", e)))?;

        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            error!("Slack send failed: {}", body);
            return Err(Error::Internal(format!("Slack send failed: {}", body)));
        }

        Ok(())
    }
}
