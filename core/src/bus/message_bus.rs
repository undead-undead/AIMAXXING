use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, broadcast};
use std::sync::Arc;

/// Inbound message from external channels (Telegram, CLI, Scheduler, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    /// Source channel (e.g., "telegram", "cli", "scheduler")
    pub channel: String,
    /// Sender identifier (user ID, phone number, etc.)
    pub sender_id: String,
    /// Chat/conversation identifier
    pub chat_id: String,
    /// Message content
    pub content: String,
    /// Optional: Media attachments (images, voice, etc.)
    pub media: Option<Vec<MediaAttachment>>,
    /// Message timestamp
    pub timestamp: DateTime<Utc>,
    /// Session key for conversation tracking
    pub session_key: String,
}

impl InboundMessage {
    /// Create a new inbound message
    pub fn new(
        channel: impl Into<String>,
        sender_id: impl Into<String>,
        chat_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let channel = channel.into();
        let chat_id = chat_id.into();
        let session_key = format!("{}:{}", channel, chat_id);
        
        Self {
            channel,
            sender_id: sender_id.into(),
            chat_id,
            content: content.into(),
            media: None,
            timestamp: Utc::now(),
            session_key,
        }
    }
    
    /// Add media attachment
    pub fn with_media(mut self, media: Vec<MediaAttachment>) -> Self {
        self.media = Some(media);
        self
    }
}

/// Outbound message to external channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    /// Target channel
    pub channel: String,
    /// Target chat ID
    pub chat_id: String,
    /// Message content
    pub content: String,
    /// Optional: Media attachments
    pub media: Option<Vec<MediaAttachment>>,
}

impl OutboundMessage {
    /// Create a new outbound message
    pub fn new(
        channel: impl Into<String>,
        chat_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            chat_id: chat_id.into(),
            content: content.into(),
            media: None,
        }
    }
}

/// Media attachment (images, voice, documents)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAttachment {
    /// Media type
    pub media_type: MediaType,
    /// File path or URL
    pub url: String,
    /// Optional caption
    pub caption: Option<String>,
}

/// Media types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Voice,
    Video,
    Document,
}

/// Message Bus - central routing for all messages
/// 
/// # Architecture
/// 
/// ```text
/// Telegram ──┐
/// Discord ───┼──▶ InboundQueue ──▶ Agent ──▶ OutboundQueue ──┐
/// CLI ───────┤                                               ├──▶ Channels
/// Scheduler ─┘                                               └──▶ Notifiers
/// ```
/// 
/// # Example
/// 
/// ```ignore
/// let bus = MessageBus::new(100);
/// 
/// // Channel publishes message
/// bus.publish_inbound(InboundMessage::new("telegram", "123", "456", "Hello")).await?;
/// 
/// // Agent consumes message
/// let msg = bus.consume_inbound().await?;
/// 
/// // Agent publishes response
/// bus.publish_outbound(OutboundMessage::new("telegram", "456", "Hi there!")).await?;
/// 
/// // Channel consumes response
/// let response = bus.consume_outbound().await?;
/// ```
pub struct MessageBus {
    inbound_tx: mpsc::Sender<InboundMessage>,
    inbound_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<InboundMessage>>>,
    outbound_tx: broadcast::Sender<OutboundMessage>,
    // outbound_rx removed as broadcast doesn't have a single shared receiver
}

impl MessageBus {
    /// Create a new message bus with specified buffer size
    pub fn new(buffer_size: usize) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(buffer_size);
        let (outbound_tx, _) = broadcast::channel(buffer_size);
        
        Self {
            inbound_tx,
            inbound_rx: Arc::new(tokio::sync::Mutex::new(inbound_rx)),
            outbound_tx,
        }
    }
    
    /// Publish an inbound message (from channel to agent)
    pub async fn publish_inbound(&self, message: InboundMessage) -> crate::error::Result<()> {
        self.inbound_tx.send(message).await
            .map_err(|e| crate::error::Error::Internal(format!("Failed to publish inbound message: {}", e)))
    }
    
    /// Consume an inbound message (agent reads from channels)
    pub async fn consume_inbound(&self) -> crate::error::Result<InboundMessage> {
        let mut rx = self.inbound_rx.lock().await;
        rx.recv().await
            .ok_or_else(|| crate::error::Error::Internal("Inbound channel closed".to_string()))
    }
    
    
    /// Publish an outbound message (from agent to channels)
    pub async fn publish_outbound(&self, message: OutboundMessage) -> crate::error::Result<()> {
        // Broadcast can fail if no receivers, but that's fine (or we log it)
        let _ = self.outbound_tx.send(message);
        Ok(())
    }
    
    /// Subscribe to outbound messages (channels receive agent responses)
    pub fn subscribe_outbound(&self) -> broadcast::Receiver<OutboundMessage> {
        self.outbound_tx.subscribe()
    }
    
    /// Get inbound sender (for cloning to multiple publishers)
    pub fn inbound_sender(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }
    
}

impl Clone for MessageBus {
    fn clone(&self) -> Self {
        Self {
            inbound_tx: self.inbound_tx.clone(),
            inbound_rx: Arc::clone(&self.inbound_rx),
            outbound_tx: self.outbound_tx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_bus_flow() {
        let bus = MessageBus::new(10);
        
        // Simulate channel publishing message
        let inbound = InboundMessage::new("telegram", "user123", "chat456", "Hello Agent");
        bus.publish_inbound(inbound.clone()).await.unwrap();
        
        // Simulate agent consuming message
        let received = bus.consume_inbound().await.unwrap();
        assert_eq!(received.channel, "telegram");
        assert_eq!(received.content, "Hello Agent");
        
        // Simulate channel consuming response
        let mut rx = bus.subscribe_outbound();
        
        // Simulate agent responding
        let outbound = OutboundMessage::new("telegram", "chat456", "Hello User");
        bus.publish_outbound(outbound).await.unwrap();

        let response = rx.recv().await.unwrap();
        assert_eq!(response.content, "Hello User");
    }

    #[tokio::test]
    async fn test_session_key_generation() {
        let msg = InboundMessage::new("telegram", "user123", "chat456", "test");
        assert_eq!(msg.session_key, "telegram:chat456");
    }
}
