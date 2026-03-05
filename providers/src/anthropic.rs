//! Anthropic (Claude) provider implementation



use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, Message, StreamingChoice, StreamingResponse, ToolDefinition, Provider, HttpConfig};
use aimaxxing_core::agent::message::{Role, Content};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic API client
pub struct Anthropic {
    client: reqwest::Client,
    api_key: String,
}

impl Anthropic {
    /// Create from API key
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let config = HttpConfig::default();
        let client = config.build_client()?;

        Ok(Self {
            client,
            api_key: api_key.into(),
        })
    }

    /// Create from environment variable
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| Error::ProviderAuth("ANTHROPIC_API_KEY not set".to_string()))?;
        Self::new(api_key)
    }

    fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key)
                .map_err(|e| Error::Internal(e.to_string()))?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        Ok(headers)
    }
}

/// Anthropic chat request
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

/// Streaming event from Anthropic
#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<StreamDelta>,
    #[serde(default)]
    content_block: Option<ContentBlockStart>,
    #[serde(default)]
    message: Option<StreamMessage>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(rename = "type")]
    _delta_type: Option<String>,
    text: Option<String>,
    partial_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContentBlockStart {
    #[serde(rename = "type")]
    block_type: String,
    id: Option<String>,
    name: Option<String>,
}

impl Anthropic {
    fn convert_messages(messages: Vec<Message>) -> Vec<AnthropicMessage> {
        messages
            .into_iter()
            .filter(|m| m.role != Role::System) // System is handled separately
            .map(|msg| {
                let role = match msg.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
                    Role::System => "user", // Shouldn't reach here
                };

                let content = match msg.content {
                    Content::Text(text) => AnthropicContent::Text(text),
                    Content::Parts(parts) => {
                        let blocks = parts.into_iter().map(|part| match part {
                            aimaxxing_core::agent::message::ContentPart::Text { text } => ContentBlock::Text { text },
                            aimaxxing_core::agent::message::ContentPart::ToolCall { id, name, arguments } => {
                                ContentBlock::ToolUse {
                                    id,
                                    name,
                                    input: arguments,
                                }
                            },
                            aimaxxing_core::agent::message::ContentPart::ToolResult { tool_call_id, content, .. } => {
                                ContentBlock::ToolResult {
                                    tool_use_id: tool_call_id,
                                    content,
                                }
                            },
                            _ => ContentBlock::Text { text: "[Image not supported]".to_string() },
                        }).collect();
                        AnthropicContent::Blocks(blocks)
                    }
                };

                AnthropicMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }

    fn convert_tools(tools: Vec<ToolDefinition>) -> Vec<AnthropicTool> {
        tools
            .into_iter()
            .map(|t| AnthropicTool {
                name: t.name,
                description: t.description,
                input_schema: t.parameters,
            })
            .collect()
    }
}

#[async_trait]
impl Provider for Anthropic {
    async fn stream_completion(
        &self,
        request: aimaxxing_core::agent::provider::ChatRequest,
    ) -> Result<StreamingResponse> {
        let aimaxxing_core::agent::provider::ChatRequest {
            model,
            system_prompt,
            messages,
            tools,
            temperature,
            max_tokens,
            extra_params: _,
            enable_cache_control: _,
        } = request;

        let anthropic_request = AnthropicRequest {
            model: model.to_string(),
            messages: Self::convert_messages(messages),
            max_tokens: max_tokens.unwrap_or(4096),
            system: system_prompt,
            temperature,
            tools: Self::convert_tools(tools),
            stream: true,
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .headers(self.build_headers()?)
            .json(&anthropic_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderApi(format!(
                "Anthropic API error {}: {}",
                status, text
            )));
        }

        let stream = response.bytes_stream();
        let parsed_stream = parse_anthropic_stream(stream);

        Ok(StreamingResponse::from_stream(parsed_stream))
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn metadata() -> aimaxxing_core::agent::provider::ProviderMetadata {
        aimaxxing_core::agent::provider::ProviderMetadata {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            description: "Claude models (3.5 Sonnet, Opus, Haiku) with excellent reasoning and long context.".to_string(),
            icon: "🎭".to_string(),
            fields: vec![
                aimaxxing_core::agent::provider::ProviderField {
                    key: "ANTHROPIC_API_KEY".to_string(),
                    label: "API Key".to_string(),
                    field_type: "password".to_string(),
                    description: "Your Anthropic API Key".to_string(),
                    required: true,
                    default: None,
                },
            ],
            capabilities: vec!["vision".into(), "tools".into(), "streaming".into(), "cache_control".into()],
            preferred_models: vec![
                "claude-3-5-sonnet-20241022".into(),
                "claude-3-5-haiku-20241022".into(),
                "claude-3-opus-20240229".into(),
            ],
        }
    }
}

/// Parse Server-Sent Events stream from Anthropic
fn parse_anthropic_stream<S>(
    stream: S,
) -> impl Stream<Item = std::result::Result<StreamingChoice, Error>>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
{
    struct ToolState {
        id: String,
        name: String,
        input_json: String,
    }

    let sse_buffer = crate::utils::SseBuffer::new();
    let current_tool: Option<ToolState> = None;
    let tool_call_counter: usize = 0;
    let pending_messages: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let usage = aimaxxing_core::agent::streaming::Usage::default();

    futures::stream::unfold(
        (stream, sse_buffer, current_tool, tool_call_counter, pending_messages, usage),
        move |(mut stream, mut bytes_buffer, mut current_tool, mut tool_call_counter, mut pending_messages, mut usage)| async move {
            loop {
                // 1. Process pending messages from buffer
                if let Some(line) = pending_messages.pop_front() {
                    // Parse event
                    if let Some(data) = line.strip_prefix("data: ") {
                        match serde_json::from_str::<StreamEvent>(data) {
                            Ok(event) => {
                                match event.event_type.as_str() {
                                    "message_start" => {
                                        if let Some(msg) = event.message {
                                            if let Some(u) = msg.usage {
                                                if let Some(input) = u.input_tokens {
                                                    usage.prompt_tokens = input;
                                                }
                                                if let Some(output) = u.output_tokens {
                                                    usage.completion_tokens = output;
                                                }
                                                usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
                                                return Some((
                                                    Ok(StreamingChoice::Usage(usage.clone())),
                                                    (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage),
                                                ));
                                            }
                                        }
                                    }
                                    "message_delta" => {
                                        if let Some(u) = event.usage {
                                            if let Some(input) = u.input_tokens {
                                                usage.prompt_tokens = input;
                                            }
                                            if let Some(output) = u.output_tokens {
                                                usage.completion_tokens = output;
                                            }
                                            usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
                                            return Some((
                                                Ok(StreamingChoice::Usage(usage.clone())),
                                                (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage),
                                            ));
                                        }
                                    }
                                    "content_block_start" => {
                                        if let Some(block) = event.content_block {
                                            if block.block_type == "tool_use" {
                                                current_tool = Some(ToolState {
                                                    id: block.id.unwrap_or_else(|| format!("tool_{}", tool_call_counter)),
                                                    name: block.name.unwrap_or_default(),
                                                    input_json: String::new(),
                                                });
                                                tool_call_counter += 1;
                                            }
                                        }
                                    }
                                    "content_block_delta" => {
                                        if let Some(delta) = event.delta {
                                            // Text delta
                                            if let Some(text) = delta.text {
                                                if !text.is_empty() {
                                                    return Some((
                                                        Ok(StreamingChoice::Message(text)),
                                                        (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage),
                                                    ));
                                                }
                                            }
                                            // Tool input delta
                                            if let Some(json) = delta.partial_json {
                                                if let Some(ref mut tool) = current_tool {
                                                    tool.input_json.push_str(&json);
                                                }
                                            }
                                        }
                                    }
                                    "content_block_stop" => {
                                        if let Some(tool) = current_tool.take() {
                                            let args = serde_json::from_str(&tool.input_json)
                                                .unwrap_or(serde_json::Value::Null);
                                            return Some((
                                                Ok(StreamingChoice::ToolCall {
                                                    id: tool.id,
                                                    name: tool.name,
                                                    arguments: args,
                                                }),
                                                (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage),
                                            ));
                                        }
                                    }
                                    "message_stop" => {
                                        return Some((
                                            Ok(StreamingChoice::Done),
                                            (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage),
                                        ));
                                    }
                                    _ => {}
                                }
                            }
                            Err(e) => {
                                tracing::debug!("Failed to parse Anthropic event: {}", e);
                            }
                        }
                    }
                    continue;
                }

                // 2. Need more data
                match stream.next().await {
                    Some(Ok(bytes)) => {
                        if let Err(e) = bytes_buffer.extend_from_slice(&bytes) {
                             return Some((Err(e), (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage)));
                        }
                        match bytes_buffer.extract_messages() {
                            Ok(messages) => {
                                pending_messages.extend(messages);
                            }
                            Err(e) => {
                                return Some((Err(e), (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage)));
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Some((
                            Err(Error::Http(e)),
                            (stream, bytes_buffer, current_tool, tool_call_counter, pending_messages, usage),
                        ));
                    }
                    None => return None,
                }
            }
        },
    )
}

/// Common model constants
pub const CLAUDE_3_5_SONNET: &str = "claude-3-5-sonnet-20241022";
/// Claude 3.5 Haiku
pub const CLAUDE_3_5_HAIKU: &str = "claude-3-5-haiku-20241022";
/// Claude 3 Opus
pub const CLAUDE_3_OPUS: &str = "claude-3-opus-20240229";
/// Claude 3 Sonnet
pub const CLAUDE_3_SONNET: &str = "claude-3-sonnet-20240229";
/// Claude 3 Haiku
pub const CLAUDE_3_HAIKU: &str = "claude-3-haiku-20240307";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi!"),
        ];

        let converted = Anthropic::convert_messages(messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }

    #[test]
    fn test_tool_conversion() {
        let tools = vec![ToolDefinition {
            name: "test".to_string(),
            description: "A test tool".to_string(),
            parameters: serde_json::json!({"type": "object"}),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }];

        let converted = Anthropic::convert_tools(tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].name, "test");
    }
}
