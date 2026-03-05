use crate::bus::{MessageBus, OutboundMessage};
use crate::config::BarkConfig;
use crate::error::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

/// Bark Connector for iOS iMessage-style push notifications.
///
/// Note: Bark is primarily a one-way notification system.
/// Bidirectional iMessage support on Linux is typically handled via third-party bridges
/// which are out of scope for the core, so Bark provides the best "iOS Native" experience.
pub struct BarkConnector {
    config: BarkConfig,
    client: Client,
}

impl BarkConnector {
    pub fn try_new(config: BarkConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                crate::error::Error::Internal(format!("Failed to build HTTP client: {}", e))
            })?;

        Ok(Self { config, client })
    }
}

#[async_trait]
impl super::Connector for BarkConnector {
    fn name(&self) -> &str {
        "im"
    }

    fn metadata() -> super::ChannelMetadata {
        super::ChannelMetadata {
            id: "im".to_string(),
            name: "Bark (iOS)".to_string(),
            description: "Send push notifications to iOS devices via Bark".to_string(),
            icon: "📱".to_string(),
            fields: vec![
                super::ChannelField {
                    key: "BARK_SERVER_URL".to_string(),
                    label: "Server URL".to_string(),
                    field_type: "text".to_string(),
                    description: "e.g., https://api.day.app".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "BARK_DEVICE_KEY".to_string(),
                    label: "Device Key".to_string(),
                    field_type: "password".to_string(),
                    description: "Your unique device key".to_string(),
                    required: true,
                }
            ],
        }
    }

    async fn start(&self, _bus: Arc<MessageBus>) -> Result<()> {
        info!("Bark (iMessage iOS) Connector started.");
        // Bark is one-way, but we stay alive to satisfy the interface.
        loop {
            sleep(Duration::from_secs(3600)).await;
        }
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        let url = format!(
            "{}/{}",
            self.config.server_url.trim_end_matches('/'),
            self.config.device_key
        );

        // Bark API supports simple GET/POST
        // https://day.app/2018/06/bark-server-api/
        let payload = json!({
            "title": "AIMAXXING Agent",
            "body": message.content,
            "group": "AIMAXXING",
            "icon": "https://img.icons8.com/color/512/bot.png", // Optional: Default bot icon
            "sound": "calypso"
        });

        let res = self.client.post(&url).json(&payload).send().await?;

        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            error!("Bark push failed: {}", body);
        } else {
            info!("Bark notification sent to iOS device.");
        }

        Ok(())
    }
}
