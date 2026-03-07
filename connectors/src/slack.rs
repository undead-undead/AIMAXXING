use brain::bus::{InboundMessage, MessageBus, OutboundMessage};
use brain::config::SlackConfig;
use brain::error::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{error, info};

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
            description: "Bi-directional Slack communication via Events API".to_string(),
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
                    key: "SLACK_VERIFICATION_TOKEN".to_string(),
                    label: "Verification Token".to_string(),
                    field_type: "password".to_string(),
                    description: "Deprecated but useful for simple verification".to_string(),
                    required: false,
                }
            ],
        }
    }

    async fn start(&self, bus: Arc<MessageBus>) -> Result<()> {
        info!("Slack Connector started. Listening for webhook events...");
        
        let mut rx = bus.subscribe_webhook_event();
        let bus = bus.clone();

        while let Ok(event) = rx.recv().await {
            if event.connector_id != "slack" {
                continue;
            }

            let payload = event.payload;

            // Slack Events API format
            // https://api.slack.com/events/message.im
            let event_type = payload["event"]["type"].as_str().unwrap_or_default();
            
            if event_type == "message" {
                let text = payload["event"]["text"].as_str().unwrap_or("");
                let user_id = payload["event"]["user"].as_str().unwrap_or("unknown");
                let channel_id = payload["event"]["channel"].as_str().unwrap_or_default();
                let bot_id = payload["event"]["bot_id"].as_str();

                // Ignore messages from bots (including ourselves)
                if bot_id.is_some() {
                    continue;
                }

                if !text.is_empty() && !channel_id.is_empty() {
                    info!("Slack connector received message from {}: {}", user_id, text);
                    
                    let inbound = brain::bus::InboundMessage::new(
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
        
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        let url = "https://slack.com/api/chat.postMessage";

        let res = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .json(&json!({
                "channel": message.chat_id,
                "text": message.content
            }))
            .send()
            .await?;

        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            error!("Slack send failed: {}", body);
        }

        Ok(())
    }
}
