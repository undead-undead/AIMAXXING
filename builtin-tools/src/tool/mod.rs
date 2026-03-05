//! Tool system for AI agents
//!
//! Provides the core abstraction for defining tools that AI agents can call.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use brain::error::Error;
use brain::skills::tool::{Tool, ToolDefinition};

#[cfg(feature = "browser")]
pub mod browser;
#[cfg(feature = "http")]
pub mod chart;
#[cfg(feature = "http")]
pub mod cipher;
#[cfg(feature = "cron")]
pub mod cron;
#[cfg(feature = "http")]
pub mod data_transform;
pub mod delegation;
pub mod filesystem;
pub mod forge;
#[cfg(feature = "http")]
pub mod git_ops;
pub mod handover;
#[cfg(feature = "http")]
pub mod mailer;
#[cfg(feature = "vector-db")]
pub mod memory;
#[cfg(feature = "http")]
pub mod notifier;
pub mod refine;
#[cfg(feature = "http")]
pub mod text_extract;
#[cfg(feature = "http")]
pub mod voice;
#[cfg(feature = "http")]
pub mod web_fetch;
#[cfg(feature = "http")]
pub mod web_search;

#[cfg(feature = "browser")]
pub use browser::BrowserTool;
#[cfg(feature = "cron")]
pub use cron::CronTool;
pub use delegation::DelegateTool;
pub use filesystem::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
pub use forge::ForgeSkill;
pub use handover::HandoverTool;
#[cfg(feature = "vector-db")]
pub use memory::{FetchDocumentTool, RememberThisTool, SearchHistoryTool, TieredSearchTool};
pub use refine::RefineSkill;
#[cfg(feature = "http")]
pub use web_fetch::WebFetchTool;
#[cfg(feature = "http")]
pub use web_search::WebSearchTool;

#[cfg(feature = "http")]
pub use git_ops::GitOpsTool;
#[cfg(feature = "http")]
pub use chart::ChartTool;
#[cfg(feature = "http")]
pub use mailer::MailerTool;
#[cfg(feature = "http")]
pub use data_transform::DataTransformTool;
#[cfg(feature = "http")]
pub use notifier::NotifierTool;
#[cfg(feature = "http")]
pub use cipher::CipherTool;
#[cfg(feature = "http")]
pub use text_extract::TextExtractTool;
#[cfg(feature = "http")]
pub use voice::{TranscribeTool, SpeakTool};

// Tool and ToolDefinition are now imported from brain::skills::tool

/// Helper for macros to generate JSON schema from a type
pub fn generate_schema<T: schemars::JsonSchema>() -> serde_json::Value {
    let gen = schemars::gen::SchemaSettings::openapi3().into_generator();
    let schema = gen.into_root_schema_for::<T>();
    serde_json::to_value(schema).unwrap_or(serde_json::json!({
        "type": "object",
        "properties": {},
        "required": []
    }))
}

#[derive(Clone)]
pub struct ToolSet {
    tools: Arc<parking_lot::RwLock<HashMap<String, Arc<dyn Tool>>>>,
    /// Cached definitions to avoid async calls during prompt generation
    cached_definitions: Arc<parking_lot::RwLock<HashMap<String, ToolDefinition>>>,
}

impl Default for ToolSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolSet {
    /// Create an empty toolset
    pub fn new() -> Self {
        Self {
            tools: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            cached_definitions: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Add a tool to the set
    pub fn add<T: Tool + 'static>(&self, tool: T) -> &Self {
        self.tools
            .write()
            .insert(tool.name().to_string(), Arc::new(tool));
        self
    }

    /// Add a shared tool to the set
    pub fn add_shared(&self, tool: Arc<dyn Tool>) -> &Self {
        self.tools.write().insert(tool.name().to_string(), tool);
        self
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().get(name).cloned()
    }

    /// Check if a tool exists
    pub fn contains(&self, name: &str) -> bool {
        self.tools.read().contains_key(name)
    }

    /// Get all tool definitions
    pub async fn definitions(&self) -> Vec<ToolDefinition> {
        self.definitions_filtered(None).await
    }

    /// Get tool definitions filtered by an enabled set
    pub async fn definitions_filtered(
        &self,
        enabled: Option<&std::collections::HashSet<String>>,
    ) -> Vec<ToolDefinition> {
        let mut defs = Vec::new();
        let tools_snapshot = self.iter();

        for (name, tool) in tools_snapshot {
            // If filter is provided, skip disabled tools
            if let Some(enabled_set) = enabled {
                if !enabled_set.contains(&name) {
                    continue;
                }
            }

            // Check cache in a small block to ensure guard is dropped
            let cached = { self.cached_definitions.read().get(&name).cloned() };

            if let Some(def) = cached {
                defs.push(def);
            } else {
                let def = tool.definition().await;
                self.cached_definitions.write().insert(name, def.clone());
                defs.push(def);
            }
        }
        defs
    }

    /// Call a tool by name
    pub async fn call(&self, name: &str, arguments: &str) -> anyhow::Result<String> {
        let tool = { self.tools.read().get(name).cloned() }
            .ok_or_else(|| Error::ToolNotFound(name.to_string()))?;

        tool.call(arguments).await
    }

    /// Get the number of tools
    pub fn len(&self) -> usize {
        self.tools.read().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.tools.read().is_empty()
    }

    /// Iterate over tools
    pub fn iter(&self) -> Vec<(String, Arc<dyn Tool>)> {
        self.tools
            .read()
            .iter()
            .map(|(k, v)| (k.clone(), Arc::clone(v)))
            .collect()
    }
}

#[async_trait::async_trait]
impl brain::agent::context::ContextInjector for ToolSet {
    async fn inject(
        &self,
        _history: &[brain::agent::message::Message],
    ) -> brain::error::Result<Vec<brain::agent::message::Message>> {
        if self.is_empty() {
            return Ok(Vec::new());
        }

        let mut content = String::from("## Available Tools (Index)\n\n");
        content.push_str(
            "You have access to the following tools. To save context, only descriptions are shown below. \
             Full TypeScript schemas and usage guidelines will be automatically injected into the conversation \
             the first time you use a specific tool.\n\n",
        );

        let mut sorted_tools: Vec<_> = self.iter();
        sorted_tools.sort_by_key(|(k, _)| k.clone());

        for (name, tool) in sorted_tools {
            let cached_def = { self.cached_definitions.read().get(&name).cloned() };

            let def = if let Some(d) = cached_def {
                d
            } else {
                let d = tool.definition().await;
                self.cached_definitions
                    .write()
                    .insert(name.clone(), d.clone());
                d
            };

            content.push_str(&format!("- **{}**: {}\n", name, def.description));
        }

        Ok(vec![brain::agent::message::Message::system(content)])
    }
}

/// Builder for creating a ToolSet
pub struct ToolSetBuilder {
    tools: Vec<Arc<dyn Tool>>,
}

impl Default for ToolSetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolSetBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Add a tool
    pub fn tool<T: Tool + 'static>(mut self, tool: T) -> Self {
        self.tools.push(Arc::new(tool));
        self
    }

    /// Add a shared tool
    pub fn shared_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    /// Build the ToolSet
    pub fn build(self) -> ToolSet {
        let toolset = ToolSet::new();
        for tool in self.tools {
            toolset.add_shared(tool);
        }
        toolset
    }
}

/// Helper macro for creating simple tools
///
/// # Example
/// ```ignore
/// simple_tool!(
///     name: "get_time",
///     description: "Get the current time",
///     handler: |_args| async {
///         Ok(chrono::Utc::now().to_rfc3339())
///     }
/// );
/// ```
#[macro_export]
macro_rules! simple_tool {
    (
        name: $name:expr,
        description: $desc:expr,
        parameters: $params:expr,
        handler: $handler:expr
    ) => {{
        struct SimpleTool;

        #[async_trait::async_trait]
        impl $crate::tool::Tool for SimpleTool {
            fn name(&self) -> String {
                $name.to_string()
            }

            async fn definition(&self) -> $crate::tool::ToolDefinition {
                $crate::tool::ToolDefinition {
                    name: $name.to_string(),
                    description: $desc.to_string(),
                    parameters: $params,
                    usage_guidelines: None,
                    is_binary: false,
                    is_verified: false,
                    parameters_ts: None,
                }
            }

            async fn call(&self, arguments: &str) -> anyhow::Result<String> {
                let handler = $handler;
                handler(arguments).await
            }
        }

        SimpleTool
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> String {
            "echo".to_string()
        }

        async fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "echo".to_string(),
                description: "Echo back the input".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to echo"
                        }
                    },
                    "required": ["message"]
                }),
                parameters_ts: None,
                is_binary: false,
                is_verified: true, // Internal tools are verified
                usage_guidelines: None,
            }
        }

        async fn call(&self, arguments: &str) -> anyhow::Result<String> {
            #[derive(Deserialize)]
            struct Args {
                message: String,
            }
            let args: Args = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
                tool_name: "echo".to_string(),
                message: e.to_string(),
            })?;
            Ok(args.message)
        }
    }

    #[tokio::test]
    async fn test_toolset() {
        let toolset = ToolSet::new();
        toolset.add(EchoTool);

        assert!(toolset.contains("echo"));
        assert_eq!(toolset.len(), 1);

        let result = toolset
            .call("echo", r#"{"message": "hello"}"#)
            .await
            .expect("call should succeed");
        assert_eq!(result, "hello");
    }
}
