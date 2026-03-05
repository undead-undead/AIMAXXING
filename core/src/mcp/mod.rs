//! MCP Client Module
//!
//! Allows AIMAXXING to consume external MCP servers as tool providers.
//! Supports both Stdio (local process) and SSE (remote HTTP) transports.
//!
//! This is completely decoupled — activating it adds zero overhead
//! to the rest of the system.

pub mod bridge;
pub mod client;
pub mod manager;
pub mod transport;
pub mod types;

pub use bridge::McpToolBridge;
pub use client::{McpClient, McpClientState};
pub use manager::McpManager;
