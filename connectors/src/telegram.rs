use brain::bus::{InboundMessage, MessageBus, OutboundMessage};
use brain::config::TelegramConfig;
use brain::error::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

pub struct TelegramConnector {
    config: TelegramConfig,
    client: Client,
}

impl TelegramConnector {
    pub fn try_new(config: TelegramConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                brain::error::Error::Internal(format!("Failed to build HTTP client: {}", e))
            })?;

        Ok(Self { config, client })
    }

    async fn get_updates(&self, offset: i64) -> Result<Vec<Value>> {
        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates",
            self.config.bot_token
        );

        let res = self
            .client
            .post(&url)
            .json(&json!({
                "offset": offset,
                "timeout": 25, // Long polling
                "allowed_updates": ["message"]
            }))
            .send()
            .await?;

        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(brain::error::Error::Internal(format!("Telegram API error: {}", body)));
        }

        let json: Value = res.json().await?;
        if let Some(updates) = json.get("result").and_then(|v| v.as_array()) {
            Ok(updates.clone())
        } else {
            Ok(Vec::new())
        }
    }
}

#[async_trait]
impl super::Connector for TelegramConnector {
    fn name(&self) -> &str {
        "telegram"
    }

    fn metadata() -> super::ChannelMetadata {
        super::ChannelMetadata {
            id: "telegram".to_string(),
            name: "Telegram".to_string(),
            description: "Bi-directional text and command interface via Telegram Bot API".to_string(),
            icon: "💬".to_string(),
            fields: vec![
                super::ChannelField {
                    key: "TELEGRAM_BOT_TOKEN".to_string(),
                    label: "Bot API Token".to_string(),
                    field_type: "password".to_string(),
                    description: "Get this from @BotFather".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "TELEGRAM_ALLOWED_CHAT_IDS".to_string(),
                    label: "Whitelisted Chat IDs".to_string(),
                    field_type: "text".to_string(),
                    description: "Comma-separated chat IDs (blank to allow all)".to_string(),
                    required: false,
                }
            ],
        }
    }

    async fn start(&self, bus: Arc<MessageBus>) -> Result<()> {
        info!("Telegram Connector started. Polling updates...");
        let mut offset = 0;
        let bus = bus.clone();
        let config = self.config.clone();

        loop {
            match self.get_updates(offset).await {
                Ok(updates) => {
                    for update in updates {
                        // Update offset
                        if let Some(update_id) = update.get("update_id").and_then(|v| v.as_i64()) {
                            offset = update_id + 1;
                        }

                        // Parse message
                        if let Some(msg) = update.get("message") {
                            let chat_id = msg
                                .get("chat")
                                .and_then(|c| c.get("id"))
                                .map(|id| id.to_string());
                            let text = msg.get("text").and_then(|t| t.as_str());

                            // Security check: Only allow configured chats
                            if let Some(cid) = chat_id.clone() {
                                if !config.allowed_chat_ids.is_empty()
                                    && !config.allowed_chat_ids.contains(&cid)
                                {
                                    warn!("Ignored message from unauthorized chat: {}", cid);
                                    continue;
                                }
                            }

                            if let (Some(chat_id), Some(text)) = (chat_id, text) {
                                let sender = msg
                                    .get("from")
                                    .and_then(|f| f.get("username").and_then(|u| u.as_str()))
                                    .unwrap_or("unknown");

                                info!("Received Telegram message from {}: {}", sender, text);

                                let inbound =
                                    InboundMessage::new("telegram", sender, chat_id, text);

                                if let Err(e) = bus.publish_inbound(inbound).await {
                                    error!("Failed to publish inbound message: {}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Telegram getUpdates failed: {}. Retrying in 5s...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
            // Small sleep to prevent busy loop if long polling fails immediately
            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let payload = json!({
            "chat_id": message.chat_id,
            "text": message.content,
            "parse_mode": "Markdown" // Or "HTML"
        });

        let _res = self.client.post(&url).json(&payload).send().await?;

        Ok(())
    }
}
