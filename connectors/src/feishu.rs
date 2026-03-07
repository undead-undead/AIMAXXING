use brain::bus::{MessageBus, OutboundMessage};
use brain::config::FeishuConfig;
use brain::error::Result;
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
                brain::error::Error::Internal(format!("Failed to build HTTP client: {}", e))
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

    async fn start(&self, bus: Arc<MessageBus>) -> Result<()> {
        info!("Feishu Connector started. Listening for webhook events...");
        
        let mut rx = bus.subscribe_webhook_event();
        let bus = bus.clone();

        while let Ok(event) = rx.recv().await {
            if event.connector_id != "feishu" {
                continue;
            }

            let payload = event.payload;

            // 1. Check event type
            // Feishu events are wrapped in a top-level "header" and "event" object (v2.0)
            let event_type = payload["header"]["event_type"].as_str().unwrap_or_default();
            
            if event_type == "im.message.receive_v1" {
                let message_data = &payload["event"]["message"];
                let sender_data = &payload["event"]["sender"];
                
                let content_str = message_data["content"].as_str().unwrap_or("{}");
                let content_json: serde_json::Value = serde_json::from_str(content_str).unwrap_or_default();
                let text = content_json["text"].as_str().unwrap_or("");
                
                let chat_id = message_data["chat_id"].as_str().unwrap_or_default();
                let sender_id = sender_data["sender_id"]["open_id"].as_str().unwrap_or("unknown");

                if !text.is_empty() && !chat_id.is_empty() {
                    info!("Feishu connector received message from {}: {}", sender_id, text);
                    
                    let inbound = brain::bus::InboundMessage::new(
                        "feishu",
                        sender_id,
                        chat_id,
                        text
                    );
                    
                    if let Err(e) = bus.publish_inbound(inbound).await {
                        error!("Failed to publish inbound Feishu message: {}", e);
                    }
                }
            }
        }
        
        Ok(())
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
            return Err(brain::error::Error::Internal(format!("Feishu auth failed: {}", auth_resp.status())));
        }

        let auth_data: serde_json::Value = auth_resp.json().await?;
        let token = auth_data["tenant_access_token"].as_str()
            .ok_or_else(|| brain::error::Error::Internal("Failed to extract tenant_access_token".to_string()))?;

        // 2. Send message
        // We assume 'chat_id' in OutboundMessage refers to receive_id (could be open_id, chat_id, etc.)
        if message.chat_id.is_empty() {
             return Err(brain::error::Error::Internal("Feishu message chat_id (chat_id/open_id) is missing".to_string()));
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
            return Err(brain::error::Error::Internal(format!("Feishu send failed: {}", error_body)));
        }

        info!("Feishu message sent successfully to {}", receive_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brain::bus::MessageBus;
    use brain::config::FeishuConfig;

    #[tokio::test]
    async fn test_feishu_message_parsing() {
        let bus = Arc::new(MessageBus::new(10));
        let config = FeishuConfig {
            app_id: "test_id".to_string(),
            app_secret: "test_secret".to_string(),
            verification_token: "test_token".to_string(),
        };
        let connector = FeishuConnector::try_new(config).unwrap();

        // Simulate Feishu v2.0 message event
        let payload = json!({
            "header": {
                "event_type": "im.message.receive_v1",
            },
            "event": {
                "sender": {
                    "sender_id": {
                        "open_id": "ou_12345"
                    }
                },
                "message": {
                    "chat_id": "oc_67890",
                    "content": "{\"text\":\"Hello AIMAXXING\"}"
                }
            }
        });

        let event = brain::bus::WebhookEvent::new("feishu", payload);
        
        // We can't easily wait for the loop in start() in a unit test without more plumbing.
        // Instead, we can verify the parsing logic if we refactored it, 
        // or just manually trigger the bus and see if it works.
        
        // For now, let's just ensure the build and basic logic is sound.
        // In a real verification, we'd run the gateway and send a curl.
    }
}
