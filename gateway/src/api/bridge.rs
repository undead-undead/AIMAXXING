use brain::agent::multi_agent::Coordinator;
use brain::bus::{InboundMessage, OutboundMessage, MessageBus};
use brain::session::store::SessionStore;

use std::sync::Arc;
use tracing::{info, error, debug, warn};
use brain::agent::message::Message;
use async_trait::async_trait;
use moka::future::Cache;
use std::time::Duration;

use engram::EngramStore;

/// Implementation of SessionStore using Engram-KV backend.
pub struct EngramSessionStore {
    store: Arc<EngramStore>,
}

impl EngramSessionStore {
    pub fn new(store: Arc<EngramStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SessionStore for EngramSessionStore {
    async fn save(&self, id: &str, messages: &[Message]) -> brain::error::Result<()> {
        let data = serde_json::to_string(messages)
            .map_err(|e| brain::error::Error::Internal(format!("Failed to serialize session: {}", e)))?;
            
        self.store.store_session(id, &data)
            .map_err(|e| brain::error::Error::Internal(format!("Failed to store session: {}", e)))?;
            
        Ok(())
    }

    async fn load(&self, id: &str) -> brain::error::Result<Option<Vec<Message>>> {
        let data = self.store.get_session(id)
            .map_err(|e| brain::error::Error::Internal(format!("Failed to load session: {}", e)))?;
            
        if let Some(s) = data {
            let messages = serde_json::from_str(&s)
                .map_err(|e| brain::error::Error::Internal(format!("Failed to deserialize session: {}", e)))?;
            Ok(Some(messages))
        } else {
            Ok(None)
        }
    }

    async fn delete_stale(&self, max_age_days: u32) -> brain::error::Result<usize> {
        self.store.delete_stale_sessions(max_age_days)
            .map_err(|e| brain::error::Error::Internal(format!("Failed to cleanup sessions: {}", e)))
    }
}

/// Bridges the MessageBus with the Coordinator (Swarm).
/// 
/// It listens for InboundMessages, routes them through the Swarm,
/// maintains session history (with LRU cache and persistent KV storage),
/// and publishes responses as OutboundMessages.
pub struct AgentBridge {
    coordinator: Arc<Coordinator>,
    bus: Arc<MessageBus>,
    /// Persistent storage for sessions
    store: Arc<dyn SessionStore>,
    /// In-memory LRU cache with TTL to prevent OOM
    cache: Cache<String, Vec<Message>>,
}

impl AgentBridge {
    pub fn new(
        coordinator: Arc<Coordinator>, 
        bus: Arc<MessageBus>,
        store: Arc<dyn SessionStore>,
    ) -> Self {
        // Cache: Max 1000 active sessions, 1 hour idle TTL
        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_idle(Duration::from_secs(3600))
            .build();

        Self {
            coordinator,
            bus,
            store,
            cache,
        }
    }

    pub async fn start(self: Arc<Self>) {
        info!("Agent Bridge started. Listening for inbound messages...");
        
        // Phase 5: Global Risk Notification Relay
        // We spawn a task for each known agent role to listen for ApprovalPending events.
        for role in self.coordinator.roles() {
            if let Some(agent) = self.coordinator.get(&role) {
                let bus = self.bus.clone();
                let mut events_rx = agent.events();
                tokio::spawn(async move {
                    while let Ok(event) = events_rx.recv().await {
                        if let brain::agent::core::AgentEventData::ApprovalPending { tool, input } = event.data {
                            info!("Relaying ApprovalPending for tool '{}' to external channels", tool);
                            // Relay to all configured outbound connectors
                            // In a real system, we'd map the session_id to specific channel/chat_id.
                            // For now, we broadcast to the "assistant" channel if known, or generic.
                            let msg = format!(
                                "⚠️ *Approval Required* (Agent: {:?})\nTool: `{}`\nInput: `{}`\n\nPlease check the dashboard to approve or reject.",
                                role, tool, input
                            );
                            
                            // Broadcast to Telegram/Discord via the bus
                            // Note: This is an unmapped broadcast; connectors should be configured to handle these.
                            let outbound_tg = OutboundMessage::new("telegram", "broadcast", msg.clone());
                            let outbound_ds = OutboundMessage::new("discord", "broadcast", msg);
                            
                            let _ = bus.publish_outbound(outbound_tg).await;
                            let _ = bus.publish_outbound(outbound_ds).await;
                        }
                    }
                });
            }
        }

        let bus = self.bus.clone();
        loop {
            match bus.consume_inbound().await {
                Ok(msg) => {
                    let self_clone = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = self_clone.handle_message(msg).await {
                            error!("Error handling message: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Bus error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn handle_message(&self, msg: InboundMessage) -> anyhow::Result<()> {
        debug!("Processing message from {}: {}", msg.channel, msg.content);
        
        let session_key = msg.session_key.clone();
        
        // 1. Load History (Cache -> Persistent Store -> New)
        let mut history = if let Some(h) = self.cache.get(&session_key).await {
            debug!("Session cache hit: {}", session_key);
            h
        } else if let Some(h) = self.store.load(&session_key).await? {
            debug!("Session retrieved from persistence: {}", session_key);
            h
        } else {
            debug!("New session started: {}", session_key);
            Vec::new()
        };

        // 2. Append User Message
        history.push(Message::user(msg.content));

        // 3. Prompt Swarm
        let mut full_response = match self.coordinator.chat_session(&session_key, history.clone()).await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Swarm chat error: {}", e);
                format!("I encountered an error: {}", e)
            }
        };

        if full_response.is_empty() {
             full_response = "I'm sorry, I couldn't generate a response.".to_string();
        }

        // 4. Update History
        history.push(Message::assistant(full_response.clone()));
        
        // 5. Save (Cache + Persistence)
        self.cache.insert(session_key.clone(), history.clone()).await;
        if let Err(e) = self.store.save(&session_key, &history).await {
            warn!("Failed to persist session {}: {}", session_key, e);
        }

        // 6. Send Response
        let outbound = OutboundMessage::new(
            msg.channel,
            msg.chat_id,
            full_response
        );
        
        self.bus.publish_outbound(outbound).await?;

        Ok(())
    }
    
    /// Cleanup stale sessions from persistent storage
    pub async fn cleanup_sessions(&self, max_age_days: u32) -> anyhow::Result<usize> {
        self.store.delete_stale(max_age_days).await.map_err(|e| anyhow::anyhow!(e))
    }
}
