//! Gateway Shared State Management
//!
//! Manages connected WebSocket clients and broadcast channels.

use brain::bus::message_bus::{MessageBus, OutboundMessage};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

use super::protocol::{ClientRole, ServerMessage};

/// Shared gateway state
#[derive(Clone)]
pub struct GatewayState {
    /// Message bus for inter-component communication
    pub bus: Arc<MessageBus>,
    /// Connected WebSocket clients
    pub clients: Arc<DashMap<Uuid, ClientInfo>>,
    /// Broadcast channel for server -> all clients
    pub broadcast_tx: broadcast::Sender<ServerMessage>,
    /// Gateway start time
    pub start_time: Instant,
    /// Optional authentication token (protected by lock for rotation)
    pub auth_token: Arc<parking_lot::RwLock<Option<String>>>,
    /// Registry for synchronous response correlation (request_id -> sender)
    pub pending_responses: Arc<DashMap<String, oneshot::Sender<OutboundMessage>>>,
}

/// Connected client information
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Client ID
    pub id: Uuid,
    /// Client role
    pub role: ClientRole,
    /// Whether the client has successfully authenticated
    pub authenticated: bool,
    /// Subscribed topics
    pub topics: Vec<String>,
    /// Connection time
    pub connected_at: Instant,
    /// Client IP address
    pub addr: Option<String>,
}

impl GatewayState {
    /// Create new gateway state
    pub fn new(bus: MessageBus) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            bus: Arc::new(bus),
            clients: Arc::new(DashMap::new()),
            broadcast_tx,
            start_time: Instant::now(),
            auth_token: Arc::new(parking_lot::RwLock::new(None)),
            pending_responses: Arc::new(DashMap::new()),
        }
    }

    /// Create gateway state with authentication token
    pub fn with_auth(self, token: impl Into<String>) -> Self {
        *self.auth_token.write() = Some(token.into());
        self
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get current auth token
    pub fn get_auth_token(&self) -> Option<String> {
        self.auth_token.read().clone()
    }

    /// Update auth token
    pub fn update_auth_token(&self, new_token: String) {
        *self.auth_token.write() = Some(new_token);
    }

    /// Register a pending response
    pub fn register_response(&self, request_id: String) -> oneshot::Receiver<OutboundMessage> {
        let (tx, rx) = oneshot::channel();
        self.pending_responses.insert(request_id, tx);
        rx
    }

    /// Fulfill a pending response
    pub fn fulfill_response(&self, request_id: &str, msg: OutboundMessage) -> bool {
        if let Some((_, tx)) = self.pending_responses.remove(request_id) {
            let _ = tx.send(msg);
            true
        } else {
            false
        }
    }

    /// Get number of connected clients
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Register a new client
    pub fn register_client(
        &self,
        id: Uuid,
        addr: Option<String>,
    ) -> broadcast::Receiver<ServerMessage> {
        self.clients.insert(
            id,
            ClientInfo {
                id,
                role: ClientRole::Guest,
                authenticated: false,
                topics: vec!["status".to_string()], // Default subscription
                connected_at: Instant::now(),
                addr,
            },
        );
        self.broadcast_tx.subscribe()
    }

    /// Remove a client
    pub fn remove_client(&self, id: &Uuid) {
        self.clients.remove(id);
    }

    /// Update client role and auth status
    pub fn authenticate_client(&self, id: &Uuid, role: ClientRole) {
        if let Some(mut client) = self.clients.get_mut(id) {
            client.role = role;
            client.authenticated = true;
        }
    }

    /// Update client subscriptions
    pub fn update_subscriptions(&self, id: &Uuid, topics: Vec<String>) {
        if let Some(mut client) = self.clients.get_mut(id) {
            client.topics = topics;
        }
    }

    /// Broadcast message to all clients
    pub fn broadcast(&self, msg: ServerMessage) {
        // Ignore send errors (no subscribers)
        let _ = self.broadcast_tx.send(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_creation() {
        let bus = MessageBus::new(10);
        let state = GatewayState::new(bus);
        assert_eq!(state.client_count(), 0);
        assert!(state.uptime_secs() < 1);
    }

    #[test]
    fn test_client_registration() {
        let bus = MessageBus::new(10);
        let state = GatewayState::new(bus);

        let id = Uuid::new_v4();
        let _rx = state.register_client(id, Some("127.0.0.1".to_string()));

        assert_eq!(state.client_count(), 1);
        let client = state.clients.get(&id).unwrap();
        assert_eq!(client.role, ClientRole::Guest);
        assert!(!client.authenticated);

        state.remove_client(&id);
        assert_eq!(state.client_count(), 0);
    }

    #[test]
    fn test_client_authentication() {
        let bus = MessageBus::new(10);
        let state = GatewayState::new(bus);

        let id = Uuid::new_v4();
        state.register_client(id, None);

        state.authenticate_client(&id, ClientRole::Master);
        let client = state.clients.get(&id).unwrap();
        assert_eq!(client.role, ClientRole::Master);
        assert!(client.authenticated);
    }
}
