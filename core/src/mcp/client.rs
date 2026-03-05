//! MCP Client
//!
//! Manages the lifecycle of a single MCP server connection:
//! spawn → initialize handshake → tool discovery → tool calls → shutdown.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::transport::{McpTransport, StdioTransport};
use super::types::*;

/// State of an MCP client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpClientState {
    /// Transport spawned, not yet initialized.
    Connected,
    /// Handshake completed, ready for requests.
    Ready,
    /// Server exited or was shut down.
    Closed,
}

/// An MCP client connected to a single server.
pub struct McpClient {
    server_name: String,
    transport: Arc<dyn McpTransport>,
    state: McpClientState,
    server_info: Option<InitializeResult>,
    tools: Vec<McpToolDef>,
}

impl McpClient {
    /// Spawn a local MCP server and perform the protocol handshake.
    pub async fn connect(
        server_name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        info!(
            server = %server_name,
            command = %command,
            "Connecting to MCP server (stdio)"
        );

        let transport = StdioTransport::spawn(command, args, env)
            .await
            .with_context(|| format!("Failed to spawn MCP server '{}'", server_name))?;

        let mut client = Self {
            server_name: server_name.to_string(),
            transport,
            state: McpClientState::Connected,
            server_info: None,
            tools: Vec::new(),
        };

        client.initialize().await?;
        Ok(client)
    }

    /// Perform the MCP initialize handshake.
    async fn initialize(&mut self) -> Result<()> {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: "aimaxxing".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let resp = self
            .transport
            .request("initialize", Some(serde_json::to_value(&params)?))
            .await
            .context("MCP initialize request failed")?;

        let result: InitializeResult = serde_json::from_value(
            resp.result
                .context("MCP initialize returned no result")?,
        )
        .context("Failed to parse MCP initialize result")?;

        info!(
            server = %self.server_name,
            protocol = %result.protocol_version,
            server_name = %result.server_info.name,
            "MCP server initialized"
        );

        self.server_info = Some(result);

        // Send `initialized` notification to complete handshake
        self.transport
            .notify("notifications/initialized", None)
            .await?;

        self.state = McpClientState::Ready;
        Ok(())
    }

    /// Ensure the client is in the Ready state.
    fn ensure_ready(&self) -> Result<()> {
        if self.state != McpClientState::Ready {
            anyhow::bail!(
                "MCP client for '{}' is not ready (state: {:?})",
                self.server_name,
                self.state
            );
        }
        Ok(())
    }

    /// Get the server name.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Get the current connection state.
    pub fn state(&self) -> McpClientState {
        self.state
    }

    /// Get cached tools list.
    pub fn tools(&self) -> &[McpToolDef] {
        &self.tools
    }

    /// Fetch/refresh the list of tools from the server.
    pub async fn list_tools(&mut self) -> Result<&[McpToolDef]> {
        self.ensure_ready()?;

        let resp = self.transport.request("tools/list", None).await?;
        let result: ToolsListResult = serde_json::from_value(
            resp.result
                .context("tools/list returned no result")?,
        )?;

        debug!(
            server = %self.server_name,
            count = result.tools.len(),
            "Fetched MCP tools"
        );

        self.tools = result.tools;
        Ok(&self.tools)
    }

    /// Call a tool on the remote MCP server.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolsCallResult> {
        self.ensure_ready()?;

        let params = ToolsCallParams {
            name: name.to_string(),
            arguments,
        };

        let resp = self
            .transport
            .request("tools/call", Some(serde_json::to_value(&params)?))
            .await
            .with_context(|| format!("Failed to call MCP tool '{}'", name))?;

        let result: ToolsCallResult = serde_json::from_value(
            resp.result
                .context("tools/call returned no result")?,
        )?;

        if result.is_error {
            warn!(
                server = %self.server_name,
                tool = name,
                "MCP tool returned error: {}",
                result.text()
            );
        }

        Ok(result)
    }

    /// Check if the server process is still alive.
    pub async fn is_alive(&self) -> bool {
        self.transport.is_alive().await
    }

    /// Gracefully shut down the MCP server.
    pub async fn shutdown(&mut self) {
        if self.state == McpClientState::Closed {
            return;
        }
        self.state = McpClientState::Closed;
        self.transport.kill().await;
        info!(server = %self.server_name, "MCP server shut down");
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if self.state != McpClientState::Closed {
            warn!(
                server = %self.server_name,
                "MCP client dropped without explicit shutdown"
            );
        }
    }
}
