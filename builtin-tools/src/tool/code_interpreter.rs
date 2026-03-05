//! Code Interpreter Tool

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::sync::Arc;
use tokio::sync::Mutex;

use brain::skills::tool::Tool;
use brain::error::Error;
use brain::skills::capabilities::Sidecar;

/// Arguments for the Code Interpreter tool
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CodeArgs {
    /// The Python code to execute
    pub code: String,
}

/// A tool that executes Python code in a stateful sidecar
pub struct CodeInterpreter {
    sidecar: Arc<Mutex<Sidecar>>,
}

impl CodeInterpreter {
    /// Create a new CodeInterpreter connected to the given sidecar
    pub fn new(sidecar: Arc<Mutex<Sidecar>>) -> Self {
        Self { sidecar }
    }
}

#[async_trait]
impl Tool for CodeInterpreter {
    fn name(&self) -> String {
        "code_interpreter".to_string()
    }

    async fn definition(&self) -> brain::skills::tool::ToolDefinition {
        brain::skills::tool::ToolDefinition {
            name: self.name(),
            description: "Executes Python code in a stateful shell. Use this for data analysis, math, and plotting.".to_string(),
            parameters: serde_json::json!({

                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python code to execute"
                    }
                },
                "required": ["code"]
            }),
            parameters_ts: Some("interface CodeArgs {\n  code: string; // Python code to execute\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this for data analysis, complex mathematical calculations, or file processing that cannot be done with simple search. Avoid using it for simple string manipulations.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: CodeArgs = serde_json::from_str(arguments)
            .map_err(|e| Error::ToolArguments {
                tool_name: self.name(),
                message: format!("Invalid JSON arguments: {}", e),
            })?;

        let mut sidecar = self.sidecar.lock().await;
        let result = sidecar.execute(args.code).await?;

        let mut output = result.stdout;
        if !result.stderr.is_empty() {
            output.push_str("\n--- Stderr ---\n");
            output.push_str(&result.stderr);
        }

        if !result.images.is_empty() {
            output.push_str(&format!("\n(Note: Generated {} image(s))", result.images.len()));
            // In a real scenario, we might want to save these to files or return them as part of multi-modal message
        }

        Ok(output)
    }
}
