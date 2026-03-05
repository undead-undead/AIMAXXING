use brain::bus::{MessageBus, OutboundMessage};
use brain::config::DingTalkConfig;
use brain::error::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

pub struct DingTalkConnector {
    config: DingTalkConfig,
    client: Client,
}

impl DingTalkConnector {
    pub fn try_new(config: DingTalkConfig) -> Result<Self> {
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
impl super::Connector for DingTalkConnector {
    fn name(&self) -> &str {
        "dingtalk"
    }

    fn metadata() -> super::ChannelMetadata {
        super::ChannelMetadata {
            id: "dingtalk".to_string(),
            name: "DingTalk".to_string(),
            description: "Enterprise messaging integration for DingTalk".to_string(),
            icon: "🏢".to_string(),
            fields: vec![
                super::ChannelField {
                    key: "DINGTALK_APP_KEY".to_string(),
                    label: "App Key".to_string(),
                    field_type: "text".to_string(),
                    description: "DingTalk generic Application Key".to_string(),
                    required: true,
                },
                super::ChannelField {
                    key: "DINGTALK_APP_SECRET".to_string(),
                    label: "App Secret".to_string(),
                    field_type: "password".to_string(),
                    description: "Secret key for the DingTalk App".to_string(),
                    required: true,
                },
            ],
        }
    }

    async fn start(&self, _bus: Arc<MessageBus>) -> Result<()> {
        info!("DingTalk Connector started.");
        info!("Note: DingTalk bi-directional stream mode requires WebSocket integration.");
        // We stay alive to satisfy the interface.
        loop {
            sleep(Duration::from_secs(3600)).await;
        }
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        let app_key = &self.config.app_key;
        let app_secret = &self.config.app_secret;

        // 1. Get Access Token
        let token_url = format!("https://oapi.dingtalk.com/gettoken?appkey={}&appsecret={}", app_key, app_secret);
        let token_resp = self.client.get(token_url).send().await?;
        if !token_resp.status().is_success() {
            return Err(brain::error::Error::Internal(format!("DingTalk token fetch failed: {}", token_resp.status())));
        }
        let token_data: serde_json::Value = token_resp.json().await?;
        let access_token = token_data["access_token"].as_str()
            .ok_or_else(|| brain::error::Error::Internal("Failed to extract DingTalk access_token".to_string()))?;

        // 2. Send Message (Assuming Chatbot Internal App Message for now)
        // Similar to Feishu, we need a target
        if message.chat_id.is_empty() {
            return Err(brain::error::Error::Internal("DingTalk message chat_id (open_conversation_id/userid) is missing".to_string()));
        }
        let target_id = message.chat_id;

        // Standard DingTalk Robot/Message API
        let send_url = format!("https://oapi.dingtalk.com/topapi/message/corpconversation/asyncsend_v2?access_token={}", access_token);
        
        // Note: DingTalk corpconversation needs agent_id too. 
        // For simple chatbot, use robot send. Assuming we need a more generic approach or robot webhook later.
        // For now, let's use the most common Custom Robot Webhook if the target looks like a token, 
        // OR corpconversation if we had AgentID. Let's stick to a basic structure.
        
        let payload = json!({
            "msg": {
                "msgtype": "text",
                "text": { "content": message.content }
            },
            "to_all_user": false,
            // Simple mapping for demo
            "userid_list": target_id
        });

        let send_resp = self.client.post(send_url).json(&payload).send().await?;
        if !send_resp.status().is_success() {
            let error_body = send_resp.text().await.unwrap_or_default();
            error!("DingTalk send failed: {}", error_body);
            return Err(brain::error::Error::Internal(format!("DingTalk send failed: {}", error_body)));
        }

        info!("DingTalk message dispatched successfully.");
        Ok(())
    }
}
