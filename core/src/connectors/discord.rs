use crate::bus::{MessageBus, OutboundMessage};
use crate::config::DiscordConfig;
use crate::error::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct DiscordConnector {
    config: DiscordConfig,
    client: Client,
}

impl DiscordConnector {
    pub fn try_new(config: DiscordConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                crate::error::Error::Internal(format!("Failed to build HTTP client: {}", e))
            })?;

        Ok(Self { config, client })
    }
}

#[async_trait]
impl super::Connector for DiscordConnector {
    fn name(&self) -> &str {
        "discord"
    }

    fn metadata() -> super::ChannelMetadata {
        super::ChannelMetadata {
            id: "discord".to_string(),
            name: "Discord".to_string(),
            description: "Push notifications to Discord channels via Webhooks".to_string(),
            icon: "🎮".to_string(),
            fields: vec![
                super::ChannelField {
                    key: "DISCORD_BOT_TOKEN".to_string(),
                    label: "Bot Token".to_string(),
                    field_type: "password".to_string(),
                    description: "Discord Bot Token from Developer Portal".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "DISCORD_CHANNEL_ID".to_string(),
                    label: "Channel ID".to_string(),
                    field_type: "text".to_string(),
                    description: "Target Discord Channel ID (Numeric)".to_string(),
                    required: true,
                }
            ],
        }
    }

    async fn start(&self, _bus: Arc<MessageBus>) -> Result<()> {
        info!("Discord Connector (Webhook mode) started.");
        info!("Note: Discord currently only supports outbound notifications via webhooks in this version.");
        // Discord bidirectional requires Bot User + Gateway (Websocket).
        // For now, we just stay alive to satisfy the interface.
        loop {
            sleep(Duration::from_secs(3600)).await;
        }
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        // We use the bot token to send via API (not just webhooks) if we have it
        // But the user might be using a Webhook URL.
        // Let's assume the config has a bot token as implemented in config/mod.rs.

        for channel_id in &self.config.channel_ids {
            let url = format!(
                "https://discord.com/api/v10/channels/{}/messages",
                channel_id
            );

            let res = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.config.bot_token))
                .json(&json!({
                    "content": message.content
                }))
                .send()
                .await?;

            if !res.status().is_success() {
                let body = res.text().await.unwrap_or_default();
                error!("Discord send failed for channel {}: {}", channel_id, body);
            }
        }

        Ok(())
    }
}
