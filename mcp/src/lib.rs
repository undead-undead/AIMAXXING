//! MCP Client Module for AIMAXXING
//!
//! Allows AIMAXXING to consume external MCP servers as tool providers.
//! Supports both Stdio (local process) and SSE (remote HTTP) transports.

pub mod bridge;
pub mod client;
pub mod manager;
pub mod server;
pub mod transport;
pub mod types;

pub use bridge::McpToolBridge;
pub use client::{McpClient, McpClientState};
pub use manager::McpManager;
pub use server::McpServer;
pub use types::*;
