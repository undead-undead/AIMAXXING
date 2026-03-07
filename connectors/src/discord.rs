use brain::bus::{MessageBus, OutboundMessage};
use brain::config::DiscordConfig;
use brain::error::Result;
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
                brain::error::Error::Internal(format!("Failed to build HTTP client: {}", e))
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

    async fn start(&self, bus: Arc<MessageBus>) -> Result<()> {
        info!("Discord Connector started. Listening for webhook interactions...");
        
        let mut rx = bus.subscribe_webhook_event();
        let bus = bus.clone();

        while let Ok(event) = rx.recv().await {
            if event.connector_id != "discord" {
                continue;
            }

            let payload = event.payload;

            // Discord Interactions (Webhook mode)
            // https://discord.com/developers/docs/interactions/receiving-and-responding#interaction-object
            let interaction_type = payload["type"].as_u64().unwrap_or(0);
            
            if interaction_type == 2 { // APPLICATION_COMMAND
                let data = &payload["data"];
                let command_name = data["name"].as_str().unwrap_or("unknown");
                let chat_id = payload["channel_id"].as_str().unwrap_or_default();
                let sender_id = payload["member"]["user"]["id"].as_str()
                    .or(payload["user"]["id"].as_str())
                    .unwrap_or("unknown");

                // Get command content (options)
                let options = data["options"].as_array();
                let text = if let Some(opts) = options {
                    opts.iter()
                        .filter_map(|o| o["value"].as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    command_name.to_string()
                };

                if !chat_id.is_empty() {
                    info!("Discord connector received interaction /{} from {}", command_name, sender_id);
                    
                    let inbound = brain::bus::InboundMessage::new(
                        "discord",
                        sender_id,
                        chat_id,
                        format!("/{} {}", command_name, text).trim().to_string()
                    );
                    
                    if let Err(e) = bus.publish_inbound(inbound).await {
                        error!("Failed to publish inbound Discord interaction: {}", e);
                    }
                }
            }
        }
        
        Ok(())
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
