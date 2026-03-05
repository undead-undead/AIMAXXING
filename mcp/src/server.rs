use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use builtin_tools::SkillLoader;
use brain::skills::tool::Tool;
use tracing::{info, error};

#[derive(Debug, Deserialize)]
struct McpRequest {
    _jsonrpc: String,
    id: Value,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
    code: i32,
    message: String,
}

pub struct McpServer {
    loader: Arc<SkillLoader>,
}

impl McpServer {
    pub fn new(loader: Arc<SkillLoader>) -> Self {
        Self { loader }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        let mut stdout = io::stdout();

        info!("MCP Server started (stdio transport)");

        while let Some(line) = reader.next_line().await? {
            let request: McpRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    error!("Failed to parse MCP request: {}", e);
                    continue;
                }
            };

            let response = self.handle_request(request).await;
            let response_json = serde_json::to_string(&response)? + "\n";
            stdout.write_all(response_json.as_bytes()).await?;
            stdout.flush().await?;
        }

        Ok(())
    }

    async fn handle_request(&self, req: McpRequest) -> McpResponse {
        match req.method.as_str() {
            "initialize" => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        },
                        "resources": {
                            "listChanged": false
                        },
                        "prompts": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": "aimaxxing-gateway",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                })),
                error: None,
            },
            "tools/list" => {
                let mut tools = Vec::new();
                for skill_ref in self.loader.skills.iter() {
                    let skill = skill_ref.value();
                    tools.push(json!({
                        "name": skill_ref.key(),
                        "description": skill.metadata().description,
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "args": {
                                    "type": "string",
                                    "description": "JSON string of arguments for the skill"
                                }
                            },
                        }
                    }));
                }
                McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(json!({ "tools": tools })),
                    error: None,
                }
            },
            "tools/call" => {
                let params = req.params.unwrap_or(json!({}));
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").and_then(|v| v.get("args")).and_then(|v| v.as_str()).unwrap_or("{}");

                if let Some(skill) = self.loader.skills.get(name) {
                    info!("MCP call: {} with args: {}", name, args);
                    match skill.call(args).await {
                        Ok(output) => McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: req.id,
                            result: Some(json!({
                                "content": [
                                    {
                                        "type": "text",
                                        "text": output
                                    }
                                ]
                            })),
                            error: None,
                        },
                        Err(e) => McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: req.id,
                            result: None,
                            error: Some(McpError {
                                code: -32000,
                                message: format!("Skill execution failed: {}", e),
                            }),
                        },
                    }
                } else {
                    McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(McpError {
                            code: -32601,
                            message: format!("Tool not found: {}", name),
                        }),
                    }
                }
            },
            "resources/list" => {
                let mut resources = Vec::new();
                for skill_ref in self.loader.skills.iter() {
                    let name = skill_ref.key();
                    resources.push(json!({
                        "uri": format!("skill://{}/SKILL.md", name),
                        "name": format!("{} Documentation", name),
                        "description": "The SKILL.md file explaining usage and metadata.",
                        "mimeType": "text/markdown"
                    }));
                }
                McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(json!({ "resources": resources })),
                    error: None,
                }
            },
            "resources/read" => {
                let params = req.params.unwrap_or(json!({}));
                let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                
                if let Some(caps) = regex::Regex::new(r"skill://([^/]+)/SKILL.md").unwrap().captures(uri) {
                    let skill_name = &caps[1];
                    if let Some(_skill) = self.loader.skills.get(skill_name) {
                        let skill_path = self.loader.base_path.join(skill_name).join("SKILL.md");
                        match std::fs::read_to_string(skill_path) {
                            Ok(content) => McpResponse {
                                jsonrpc: "2.0".to_string(),
                                id: req.id,
                                result: Some(json!({
                                    "contents": [
                                        {
                                            "uri": uri,
                                            "mimeType": "text/markdown",
                                            "text": content
                                        }
                                    ]
                                })),
                                error: None,
                            },
                            Err(e) => McpResponse {
                                jsonrpc: "2.0".to_string(),
                                id: req.id,
                                result: None,
                                error: Some(McpError {
                                    code: -32000,
                                    message: format!("Failed to read SKILL.md: {}", e),
                                }),
                            },
                        }
                    } else {
                        McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: req.id,
                            result: None,
                            error: Some(McpError {
                                code: -32602,
                                message: format!("Skill not found: {}", skill_name),
                            }),
                        }
                    }
                } else {
                    McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(McpError {
                            code: -32602,
                            message: "Invalid resource URI".to_string(),
                        }),
                    }
                }
            },
            "prompts/list" => {
                McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(json!({
                        "prompts": [
                            {
                                "name": "skill-help",
                                "description": "Get detailed help and usage examples for a specific skill.",
                                "arguments": [
                                    {
                                        "name": "skill_name",
                                        "description": "The name of the skill to get help for.",
                                        "required": true
                                    }
                                ]
                            }
                        ]
                    })),
                    error: None,
                }
            },
            "prompts/get" => {
                let params = req.params.unwrap_or(json!({}));
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args_fallback = json!({});
                let args = params.get("arguments").unwrap_or(&args_fallback);
                let skill_name = args.get("skill_name").and_then(|v| v.as_str()).unwrap_or("");

                if name == "skill-help" && !skill_name.is_empty() {
                    if let Some(skill) = self.loader.skills.get(skill_name) {
                        let meta = skill.value().metadata();
                        let help_text = format!(
                            "# Skill: {}\n\n{}\n\n## Parameters\n```typescript\n{}\n```\n\n## Runtime: {}\n",
                            meta.name,
                            meta.description,
                            meta.interface.as_deref().unwrap_or("No interface defined"),
                            meta.runtime.as_deref().unwrap_or("default")
                        );
                        McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: req.id,
                            result: Some(json!({
                                "messages": [
                                    {
                                        "role": "user",
                                        "content": {
                                            "type": "text",
                                            "text": format!("I need help using the '{}' skill.", skill_name)
                                        }
                                    },
                                    {
                                        "role": "assistant",
                                        "content": {
                                            "type": "text",
                                            "text": help_text
                                        }
                                    }
                                ]
                            })),
                            error: None,
                        }
                    } else {
                        McpResponse {
                            jsonrpc: "2.0".to_string(),
                            id: req.id,
                            result: None,
                            error: Some(McpError {
                                code: -32602,
                                message: format!("Skill not found: {}", skill_name),
                            }),
                        }
                    }
                } else {
                    McpResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(McpError {
                            code: -32602,
                            message: "Invalid prompt name or missing arguments".to_string(),
                        }),
                    }
                }
            },
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: "Method not found".to_string(),
                }),
            },
        }
    }
}
