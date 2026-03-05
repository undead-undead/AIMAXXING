use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::error::Result;

/// Notification channel types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotifyChannel {
    /// Send an email
    Email,
    /// Send via Telegram
    Telegram,
    /// Send via Discord
    Discord,
    /// Send to a generic Webhook
    Webhook { url: String },
    /// Log to console/file
    Log,
}

/// Trait for sending notifications
/// 
/// Implement this trait to connect the Agent to external communication systems
/// like Telegram bots, Discord webhooks, or email servers.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Send a notification
    async fn notify(&self, channel: NotifyChannel, message: &str) -> Result<()>;
}

/// A no-op notifier that logs to tracing
pub struct LogNotifier;

#[async_trait]
impl Notifier for LogNotifier {
    async fn notify(&self, channel: NotifyChannel, message: &str) -> Result<()> {
        let channel_name = match channel {
            NotifyChannel::Email => "Email",
            NotifyChannel::Telegram => "Telegram",
            NotifyChannel::Discord => "Discord",
            NotifyChannel::Webhook { .. } => "Webhook",
            NotifyChannel::Log => "Log",
        };
        tracing::info!("[Notification via {}]: {}", channel_name, message);
        Ok(())
    }
}
