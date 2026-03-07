//! Connectors module for external messaging platforms.
//!
//! This module provides the `Connector` trait and implementations for
//! bi-directional communication channels like Telegram, Discord, and others.

use brain::bus::{MessageBus, OutboundMessage};
use brain::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Describes a configuration field required by a Channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelField {
    pub key: String,         // e.g. "telegram_bot_token"
    pub label: String,       // e.g. "Bot Token"
    pub field_type: String,  // e.g. "password", "text"
    pub description: String,
    pub required: bool,
}

/// Metadata describing a supported channel and its configuration schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMetadata {
    pub id: String,          // e.g. "telegram"
    pub name: String,        // e.g. "Telegram"
    pub description: String,
    pub icon: String,        // Emoji or icon name
    pub fields: Vec<ChannelField>,
}

/// A Connector bridges an external platform (Telegram, Discord) to the internal MessageBus.
#[async_trait]
pub trait Connector: Send + Sync {
    /// Start the connector loop (listening for messages)
    async fn start(&self, bus: Arc<MessageBus>) -> Result<()>;

    /// Send a message back to the platform
    async fn send(&self, message: OutboundMessage) -> Result<()>;

    /// Get the unique name of this connector (e.g., "telegram")
    fn name(&self) -> &str;

    /// Return the metadata schema for configuring this connector via the Panel
    fn metadata() -> ChannelMetadata where Self: Sized;
}

pub mod discord;
pub mod im;
pub mod telegram;
pub mod feishu;
pub mod dingtalk;
pub mod slack;
pub mod email;

pub use telegram::TelegramConnector;
pub use discord::DiscordConnector;
pub use feishu::FeishuConnector;
pub use dingtalk::DingTalkConnector;
pub use slack::SlackConnector;
pub use email::EmailConnector;
pub use im::BarkConnector;
