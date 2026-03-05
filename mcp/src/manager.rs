//! MCP Server Manager
//!
//! Manages multiple MCP server connections concurrently.
//! Handles connection lifecycle, health checks, and auto-reconnection.

use anyhow::Result;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use super::client::McpClient;
use super::types::McpToolDef;

/// Configuration for a single MCP server.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Unique name for this server.
    pub name: String,
    /// Command to spawn the server.
    pub command: String,
    /// Arguments for the command.
    pub args: Vec<String>,
    /// Environment variables to set.
    pub env: HashMap<String, String>,
    /// Whether to auto-reconnect on failure.
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts.
    pub max_reconnect_attempts: u32,
}

/// Manages multiple MCP servers and their tool registrations.
pub struct McpManager {
    /// Active clients, keyed by server name.
    clients: DashMap<String, Arc<Mutex<McpClient>>>,
    /// Server configs for reconnection.
    configs: DashMap<String, McpServerConfig>,
    /// Aggregated tool → server mapping.
    tool_map: DashMap<String, String>,
}

impl McpManager {
    /// Create a new empty manager.
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            configs: DashMap::new(),
            tool_map: DashMap::new(),
        }
    }

    /// Add and connect to an MCP server.
    pub async fn add_server(&self, config: McpServerConfig) -> Result<()> {
        let name = config.name.clone();
        info!(server = %name, command = %config.command, "Adding MCP server");

        let client = McpClient::connect(
            &config.name,
            &config.command,
            &config.args,
            &config.env,
        )
        .await?;

        let client = Arc::new(Mutex::new(client));

        // Discover tools
        {
            let mut c = client.lock().await;
            let tools = c.list_tools().await?;
            for tool in tools {
                info!(
                    server = %name,
                    tool = %tool.name,
                    "Registered MCP tool"
                );
                self.tool_map.insert(tool.name.clone(), name.clone());
            }
        }

        self.configs.insert(name.clone(), config);
        self.clients.insert(name, client);

        Ok(())
    }

    /// Remove and shut down an MCP server.
    pub async fn remove_server(&self, name: &str) {
        // Remove tool mappings
        self.tool_map.retain(|_, server| server != name);

        // Shut down client
        if let Some((_, client)) = self.clients.remove(name) {
            let mut c = client.lock().await;
            c.shutdown().await;
        }

        self.configs.remove(name);
        info!(server = %name, "MCP server removed");
    }

    /// Get all available tools from all connected servers.
    pub fn all_tools(&self) -> Vec<(String, McpToolDef)> {
        let mut result = Vec::new();
        for client in self.clients.iter() {
            let name = client.key().clone();
            // We need to access cached tools without locking
            // This returns the last fetched tool list
            if let Some(tools_ref) = self.tool_map.iter().next() {
                let _ = tools_ref; // tool_map is server -> name mapping
            }
            // For simplicity, collect from tool_map
            for entry in self.tool_map.iter() {
                if entry.value() == &name {
                    result.push((
                        name.clone(),
                        McpToolDef {
                            name: entry.key().clone(),
                            description: String::new(),
                            input_schema: serde_json::Value::Object(serde_json::Map::new()),
                        },
                    ));
                }
            }
        }
        result
    }

    /// Call a tool by name (automatically routes to the right server).
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String> {
        // Find which server owns this tool
        let server_name = self
            .tool_map
            .get(tool_name)
            .map(|v| v.value().clone())
            .ok_or_else(|| {
                anyhow::anyhow!("MCP tool '{}' not found in any connected server", tool_name)
            })?;

        // Get the client
        let client = self
            .clients
            .get(&server_name)
            .ok_or_else(|| {
                anyhow::anyhow!("MCP server '{}' not connected", server_name)
            })?;

        let client_arc = Arc::clone(client.value());
        drop(client); // Release DashMap reference

        let c = client_arc.lock().await;

        // Check health
        if !c.is_alive().await {
            warn!(server = %server_name, "MCP server is not alive");

            // Attempt reconnection if configured
            if let Some(config) = self.configs.get(&server_name) {
                if config.auto_reconnect {
                    drop(c);
                    drop(client_arc);
                    return self.reconnect_and_call(&server_name, tool_name, arguments).await;
                }
            }

            anyhow::bail!("MCP server '{}' is not alive", server_name);
        }

        let result = c.call_tool(tool_name, arguments).await?;
        Ok(result.text())
    }

    /// Reconnect to a server and retry the tool call.
    async fn reconnect_and_call(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String> {
        let config = self
            .configs
            .get(server_name)
            .map(|c| c.value().clone())
            .ok_or_else(|| anyhow::anyhow!("No config for server '{}'", server_name))?;

        let mut last_error = None;
        for attempt in 0..config.max_reconnect_attempts {
            info!(
                server = %server_name,
                attempt = attempt + 1,
                "Attempting MCP server reconnection"
            );

            match McpClient::connect(
                &config.name,
                &config.command,
                &config.args,
                &config.env,
            )
            .await
            {
                Ok(mut new_client) => {
                    // Re-discover tools
                    if let Ok(tools) = new_client.list_tools().await {
                        for tool in tools {
                            self.tool_map.insert(tool.name.clone(), server_name.to_string());
                        }
                    }

                    // Retry the tool call
                    let result = new_client.call_tool(tool_name, arguments).await?;

                    // Replace the old client
                    self.clients.insert(
                        server_name.to_string(),
                        Arc::new(Mutex::new(new_client)),
                    );

                    return Ok(result.text());
                }
                Err(e) => {
                    warn!(
                        server = %server_name,
                        attempt = attempt + 1,
                        error = %e,
                        "Reconnection failed"
                    );
                    last_error = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("All reconnection attempts failed for '{}'", server_name)
        }))
    }

    /// Health check all servers, reconnecting dead ones if configured.
    pub async fn health_check(&self) {
        let server_names: Vec<String> = self.clients.iter().map(|e| e.key().clone()).collect();

        for name in server_names {
            if let Some(client) = self.clients.get(&name) {
                let c = client.value().lock().await;
                if !c.is_alive().await {
                    warn!(server = %name, "MCP server health check failed");
                    drop(c);

                    if let Some(config) = self.configs.get(&name) {
                        if config.auto_reconnect {
                            drop(client);
                            let config_clone = config.value().clone();
                            drop(config);
                            let _ = self.add_server(config_clone).await;
                        }
                    }
                }
            }
        }
    }

    /// Shut down all servers gracefully.
    pub async fn shutdown_all(&self) {
        let names: Vec<String> = self.clients.iter().map(|e| e.key().clone()).collect();
        for name in names {
            self.remove_server(&name).await;
        }
    }

    /// Get the number of connected servers.
    pub fn server_count(&self) -> usize {
        self.clients.len()
    }

    /// Get the total number of registered tools.
    pub fn tool_count(&self) -> usize {
        self.tool_map.len()
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
