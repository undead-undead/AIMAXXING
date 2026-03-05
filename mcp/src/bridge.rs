//! MCP Tool Bridge
//!
//! Bridges MCP tools into the AIMAXXING Tool system.
//! Each MCP tool becomes a native AIMAXXING Tool that the Agent can call seamlessly.

use async_trait::async_trait;
use std::sync::Arc;
use tracing::debug;

use brain::skills::tool::{Tool, ToolDefinition};
use super::manager::McpManager;
use super::types::McpToolDef;

/// A bridge that wraps an MCP tool as an AIMAXXING Tool.
///
/// This allows MCP tools to appear alongside native tools in the Agent's toolset.
pub struct McpToolBridge {
    /// The MCP tool definition
    tool_def: McpToolDef,
    /// Name of the MCP server that provides this tool
    server_name: String,
    /// Reference to the MCP manager for dispatching calls
    manager: Arc<McpManager>,
}

impl McpToolBridge {
    /// Create a new bridge for an MCP tool.
    pub fn new(
        tool_def: McpToolDef,
        server_name: String,
        manager: Arc<McpManager>,
    ) -> Self {
        Self {
            tool_def,
            server_name,
            manager,
        }
    }

    /// Create bridges for all tools from a connected MCP server.
    pub fn bridge_all(
        manager: Arc<McpManager>,
        tools: &[(String, McpToolDef)],
    ) -> Vec<Arc<dyn Tool>> {
        tools
            .iter()
            .map(|(server, tool)| {
                let bridge = McpToolBridge::new(
                    tool.clone(),
                    server.clone(),
                    Arc::clone(&manager),
                );
                Arc::new(bridge) as Arc<dyn Tool>
            })
            .collect()
    }
}

#[async_trait]
impl Tool for McpToolBridge {
    fn name(&self) -> String {
        // Prefix with server name to avoid conflicts
        format!("mcp:{}:{}", self.server_name, self.tool_def.name)
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: format!(
                "[MCP: {}] {}",
                self.server_name, self.tool_def.description
            ),
            parameters: self.tool_def.input_schema.clone(),
            parameters_ts: None,
            is_binary: false,
            is_verified: false, // MCP tools are external, thus unverified
            usage_guidelines: Some(format!(
                "This tool is provided by the external MCP server '{}'. It may have its own rate limits and capabilities.",
                self.server_name
            )),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| anyhow::anyhow!("Invalid JSON arguments: {}", e))?;

        debug!(
            tool = %self.tool_def.name,
            server = %self.server_name,
            "Calling MCP tool via bridge"
        );

        self.manager
            .call_tool(&self.tool_def.name, args)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_name_format() {
        let tool = McpToolDef {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };

        let manager = Arc::new(McpManager::new());
        let bridge = McpToolBridge::new(tool, "filesystem".to_string(), manager);

        assert_eq!(bridge.name(), "mcp:filesystem:read_file");
    }
}
