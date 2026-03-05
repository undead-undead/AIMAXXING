use crate::bus::{MessageBus, OutboundMessage};
use crate::config::FeishuConfig;
use crate::error::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct FeishuConnector {
    config: FeishuConfig,
    client: Client,
}

impl FeishuConnector {
    pub fn try_new(config: FeishuConfig) -> Result<Self> {
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
impl super::Connector for FeishuConnector {
    fn name(&self) -> &str {
        "feishu"
    }

    fn metadata() -> super::ChannelMetadata {
        super::ChannelMetadata {
            id: "feishu".to_string(),
            name: "Feishu / Lark".to_string(),
            description: "Enterprise messaging via Feishu Custom App (Lark)".to_string(),
            icon: "💼".to_string(),
            fields: vec![
                super::ChannelField {
                    key: "FEISHU_APP_ID".to_string(),
                    label: "App ID".to_string(),
                    field_type: "text".to_string(),
                    description: "Feishu Custom App ID".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "FEISHU_APP_SECRET".to_string(),
                    label: "App Secret".to_string(),
                    field_type: "password".to_string(),
                    description: "Feishu App Secret".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "FEISHU_VERIFICATION_TOKEN".to_string(),
                    label: "Verification Token".to_string(),
                    field_type: "password".to_string(),
                    description: "Token for event verification".to_string(),
                    required: true,
                },
            ],
        }
    }

    async fn start(&self, _bus: Arc<MessageBus>) -> Result<()> {
        info!("Feishu Connector started.");
        info!("Note: Feishu bi-directional stream requires establishing a WebSocket connection or setting up Event Webhooks.");
        // We stay alive to satisfy the interface.
        loop {
            sleep(Duration::from_secs(3600)).await;
        }
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        let app_id = &self.config.app_id;
        let app_secret = &self.config.app_secret;

        // 1. Get tenant_access_token
        let auth_url = "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
        let auth_resp = self.client.post(auth_url)
            .json(&json!({
                "app_id": app_id,
                "app_secret": app_secret
            }))
            .send()
            .await?;

        if !auth_resp.status().is_success() {
            return Err(crate::error::Error::Internal(format!("Feishu auth failed: {}", auth_resp.status())));
        }

        let auth_data: serde_json::Value = auth_resp.json().await?;
        let token = auth_data["tenant_access_token"].as_str()
            .ok_or_else(|| crate::error::Error::Internal("Failed to extract tenant_access_token".to_string()))?;

        // 2. Send message
        // We assume 'chat_id' in OutboundMessage refers to receive_id (could be open_id, chat_id, etc.)
        if message.chat_id.is_empty() {
             return Err(crate::error::Error::Internal("Feishu message chat_id (chat_id/open_id) is missing".to_string()));
        }
        let receive_id = message.chat_id;
        let receive_id_type = if receive_id.starts_with("oc_") { "chat_id" } else { "open_id" };

        let send_url = format!("https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type={}", receive_id_type);
        let send_resp = self.client.post(&send_url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({
                "receive_id": receive_id,
                "msg_type": "text",
                "content": json!({ "text": message.content }).to_string()
            }))
            .send()
            .await?;

        if !send_resp.status().is_success() {
            let error_body = send_resp.text().await.unwrap_or_default();
            error!("Feishu send failed: {}", error_body);
            return Err(crate::error::Error::Internal(format!("Feishu send failed: {}", error_body)));
        }

        info!("Feishu message sent successfully to {}", receive_id);
        Ok(())
    }
}
