//! WebSocket Gateway Protocol Definitions
//!
//! Defines the message types for client-server communication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use brain::bus::message_bus::{InboundMessage, OutboundMessage};

/// Roles for connected clients
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClientRole {
    /// Full administrative control
    Master,
    /// External messaging provider (e.g., Telegram node)
    Node,
    /// Read-only or restricted UI client
    Client,
    /// Unauthenticated connection
    Guest,
}

/// Messages sent from client to gateway
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Initial handshake to authenticate
    Connect { role: ClientRole, token: String },
    /// Inbound message from an external node (e.g., Telegram)
    Inbound { message: InboundMessage },
    /// Subscribe to event topics
    Subscribe { topics: Vec<String> },
    /// Unsubscribe from topics
    Unsubscribe { topics: Vec<String> },
    /// Send a command (e.g., emergency stop)
    Command {
        action: String,
        params: Option<Value>,
    },
    /// Ping for keepalive
    Ping,
    /// Administrative commands (Master only)
    Admin { command: AdminCommand },
}

/// Administrative commands
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AdminCommand {
    /// Rotate the authentication token
    RotateToken { new_token: String },
    /// Evict a client by ID
    EvictClient { client_id: uuid::Uuid },
    /// Request a security audit report
    SecurityAudit,
}

// ============================================================================
// Server -> Client Messages
// ============================================================================

/// Messages sent from gateway to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Authentication successful
    AuthSuccess { role: ClientRole },
    /// Outbound message to be sent to external channels
    Outbound { message: OutboundMessage },
    /// Gateway status update
    Status(GatewayStatus),
    /// Risk alert notification
    RiskAlert(RiskAlert),
    /// Trade event
    Trade(TradeEvent),
    /// Message log entry
    Log(LogEntry),
    /// Pong response
    Pong,
    /// Error message
    Error { code: String, message: String },
}

/// Gateway status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStatus {
    /// Number of connected clients
    pub clients: usize,
    /// Gateway uptime in seconds
    pub uptime_secs: u64,
    /// Agent statuses
    pub agents: Vec<AgentStatus>,
    /// Connected clients (Master only see this usually, but shared for now)
    pub connected_clients: Vec<ClientSnapshot>,
    /// System resource stats
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemStats>,
    /// Current timestamp
    pub timestamp: DateTime<Utc>,
}

/// System resource statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// CPU usage percentage (0-100)
    pub cpu: f32,
    /// Memory usage
    pub memory: MemoryStats,
    /// Disk usage
    pub disk: DiskStats,
    /// System load average (1m, 5m, 15m)
    pub load: [f64; 3],
    /// System uptime in seconds
    pub sys_uptime: u64,
}

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Used memory in bytes
    pub used: u64,
    /// Total memory in bytes
    pub total: u64,
    /// Usage percentage
    pub percent: f32,
}

/// Disk statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskStats {
    /// Used disk space in bytes
    pub used: u64,
    /// Total disk space in bytes
    pub total: u64,
    /// Usage percentage
    pub percent: f32,
}

/// Snapshot of a connected client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSnapshot {
    pub id: uuid::Uuid,
    pub role: ClientRole,
    pub addr: Option<String>,
    pub uptime_secs: u64,
}

/// Snapshot of a connected client

/// Individual agent status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Agent identifier
    pub id: String,
    /// Agent name
    pub name: String,
    /// Current state (idle, running, error)
    pub state: AgentState,
    /// Last active timestamp
    pub last_active: Option<DateTime<Utc>>,
}

/// Agent state enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Idle,
    Running,
    Error,
    Stopped,
}

/// Risk alert from risk manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAlert {
    /// Alert level (info, warning, critical)
    pub level: AlertLevel,
    /// Alert message
    pub message: String,
    /// Related trade ID if applicable
    pub trade_id: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// Trade event notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    /// Trade ID
    pub id: String,
    /// Trade action (buy, sell, swap)
    pub action: String,
    /// From token
    pub from_token: String,
    /// To token
    pub to_token: String,
    /// Amount in USD
    pub amount_usd: f64,
    /// Status (pending, completed, failed)
    pub status: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Log entry for message bus activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Source channel
    pub channel: String,
    /// Log level
    pub level: String,
    /// Log message
    pub message: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_serialization() {
        let msg = ClientMessage::Subscribe {
            topics: vec!["status".to_string(), "trades".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("subscribe"));

        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            ClientMessage::Subscribe { topics } => {
                assert_eq!(topics.len(), 2);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_server_message_serialization() {
        let msg = ServerMessage::Status(GatewayStatus {
            clients: 5,
            uptime_secs: 3600,
            agents: vec![],
            connected_clients: vec![], // Added this line as per instruction
            system: None,
            timestamp: Utc::now(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("status"));
        assert!(json.contains("clients"));
    }

    #[test]
    fn test_risk_alert_levels() {
        let alert = RiskAlert {
            level: AlertLevel::Critical,
            message: "Daily limit exceeded".to_string(),
            trade_id: Some("trade_123".to_string()),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&alert).unwrap();
        assert!(json.contains("critical"));
    }
}
