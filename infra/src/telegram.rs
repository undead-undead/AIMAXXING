use reqwest::Client;
use serde_json::json;
use std::time::Duration;

/// Telegram Notifier - send one-way notifications to Telegram
/// 
/// # Example
/// 
/// ```ignore
/// let notifier = TelegramNotifier::new(
///     "1234567890:ABCdefGHI...",  // bot token
///     "123456789"                  // chat ID
/// );
/// 
/// notifier.notify("Order filled: BTC/USDT @ $43,200").await?;
/// ```
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: Client,
}

impl TelegramNotifier {
    /// Create a new Telegram notifier
    pub fn new(bot_token: impl Into<String>, chat_id: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            bot_token: bot_token.into(),
            chat_id: chat_id.into(),
            client,
        }
    }
    
    /// Send a notification message
    pub async fn notify(&self, message: &str) -> brain::error::Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );
        
        let payload = json!({
            "chat_id": self.chat_id,
            "text": message,
            "parse_mode": "Markdown"
        });
        
        let response = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| brain::error::Error::Internal(format!("Telegram API error: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(brain::error::Error::Internal(
                format!("Telegram API returned {}: {}", status, body)
            ));
        }
        
        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::observable::AgentObserver for TelegramNotifier {
    async fn on_event(&self, event: &brain::agent::core::AgentEvent) -> brain::error::Result<()> {
        let message = match &event.data {
            brain::agent::core::AgentEventData::Thinking { prompt } => {
                format!("─── *thinking* ───\n`{}`", prompt)
            }
            brain::agent::core::AgentEventData::Thought { content } => {
                format!("─── *reasoning* ───\n`{}`", content)
            }
            brain::agent::core::AgentEventData::ToolCall { tool, input } => {
                format!("─── *tool call* ───\n*target:* `{}`\n*input:* `{}`", tool, input)
            }
            brain::agent::core::AgentEventData::ToolResult { tool, output } => {
                let preview = if output.len() > 100 { format!("{}...", &output[..100]) } else { output.clone() };
                format!("─── *tool result* ───\n*target:* `{}`\n*output:* `{}`", tool, preview)
            }
            brain::agent::core::AgentEventData::ApprovalPending { tool, input } => {
                format!("─── *approval required* ───\n*target:* `{}`\n*input:* `{}`", tool, input)
            }
            brain::agent::core::AgentEventData::Response { content, .. } => {
                format!("─── *response* ───\n{}", content)
            }
            brain::agent::core::AgentEventData::Error { message } => {
                format!("─── *error* ───\n{}", message)
            }
            brain::agent::core::AgentEventData::Cancelled { reason } => {
                format!("─── *cancelled* ───\n{}", reason)
            }
            // Technical events are ignored for one-way Telegram notifications to avoid spam
            brain::agent::core::AgentEventData::StepStart { .. } | brain::agent::core::AgentEventData::ToolExecutionStart { .. } | brain::agent::core::AgentEventData::ToolExecutionEnd { .. } | brain::agent::core::AgentEventData::TokenUsage { .. } | brain::agent::core::AgentEventData::LatencyTTFT { .. } => {
                return Ok(());
            }
        };

        self.notify(&message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires real Telegram credentials
    async fn test_send_notification() {
        let notifier = TelegramNotifier::new("test_token", "test_chat_id");
        // Would need real credentials to test
    }
}
