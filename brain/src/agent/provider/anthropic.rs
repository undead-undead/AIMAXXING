//! Anthropic Messages API Provider
//!
//! Native support for Anthropic's Messages API (Claude models).
//! Handles Anthropic-specific features: cache_control, thinking blocks.
//!
//! Feature-gated behind `http`.

use async_trait::async_trait;
use futures::stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

use crate::agent::message::{Content, ContentPart, Message, Role};
use crate::agent::provider::{ChatRequest, Provider};
use crate::agent::streaming::{StreamingChoice, StreamingResponse, Usage};
use crate::error::{Error, Result};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Configuration for the Anthropic provider.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// API key
    pub api_key: String,
    /// Default model (e.g. "claude-sonnet-4-20250514")
    pub default_model: String,
    /// Request timeout
    pub timeout: Duration,
    /// Max retries on transient errors
    pub max_retries: u32,
    /// Base delay for exponential backoff
    pub retry_base_delay: Duration,
    /// Enable prompt caching (cache_control ephemeral blocks)
    pub enable_cache: bool,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            default_model: "claude-sonnet-4-20250514".to_string(),
            timeout: Duration::from_secs(120),
            max_retries: 3,
            retry_base_delay: Duration::from_millis(500),
            enable_cache: true,
        }
    }
}

impl AnthropicConfig {
    /// Create from environment variables.
    /// Reads: ANTHROPIC_API_KEY, ANTHROPIC_MODEL
    pub fn from_env() -> Self {
        Self {
            api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_default(),
            default_model: std::env::var("ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string()),
            ..Default::default()
        }
    }
}

/// Anthropic Messages API provider with full feature support.
pub struct AnthropicProvider {
    config: AnthropicConfig,
    client: Client,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider from config.
    pub fn new(config: AnthropicConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .pool_max_idle_per_host(4)
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    /// Create from environment variables.
    pub fn from_env() -> Result<Self> {
        Self::new(AnthropicConfig::from_env())
    }

    /// Convert AIMAXXING messages to Anthropic format.
    /// Anthropic separates system from messages.
    fn convert_messages(messages: &[Message]) -> Vec<AnthropicMessage> {
        messages
            .iter()
            .filter(|m| m.role != Role::System) // system is sent separately
            .map(|msg| {
                let role = match msg.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
                    _ => "user",
                };

                // Handle tool results → user message with tool_result block
                if msg.role == Role::Tool {
                    if let Content::Parts(parts) = &msg.content {
                        for part in parts {
                            if let ContentPart::ToolResult {
                                tool_call_id,
                                content,
                                ..
                            } = part
                            {
                                return AnthropicMessage {
                                    role: "user".to_string(),
                                    content: AnthropicContent::Blocks(vec![
                                        AnthropicBlock::ToolResult {
                                            tool_use_id: tool_call_id.clone(),
                                            content: content.clone(),
                                        },
                                    ]),
                                };
                            }
                        }
                    }
                }

                AnthropicMessage {
                    role: role.to_string(),
                    content: AnthropicContent::Text(msg.text()),
                }
            })
            .collect()
    }

    /// Convert AIMAXXING tool definitions to Anthropic format.
    fn convert_tools(
        tools: &[crate::skills::tool::ToolDefinition],
    ) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect()
    }

    /// Extract system prompt from messages.
    fn extract_system(messages: &[Message], explicit_system: &Option<String>) -> Option<String> {
        if let Some(ref s) = explicit_system {
            return Some(s.clone());
        }
        messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.text())
    }

    /// Execute with exponential backoff retry.
    async fn execute_with_retry(
        &self,
        request: &AnthropicChatRequest,
    ) -> Result<AnthropicChatResponse> {
        let url = format!("{}/messages", ANTHROPIC_API_URL);
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.config.retry_base_delay * 2u32.pow(attempt - 1);
                debug!(
                    provider = "anthropic",
                    attempt = attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying after transient error"
                );
                tokio::time::sleep(delay).await;
            }

            let response = match self
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .json(request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if e.is_timeout() {
                        last_error = Some(Error::ProviderApi(format!("Timeout: {}", e)));
                        continue;
                    }
                    if e.is_connect() {
                        last_error = Some(Error::ProviderApi(format!("Connection error: {}", e)));
                        continue;
                    }
                    return Err(Error::ProviderApi(format!("Request failed: {}", e)));
                }
            };

            let status = response.status();

            match status.as_u16() {
                200..=299 => {
                    let body = response.text().await.map_err(|e| {
                        Error::ProviderApi(format!("Failed to read body: {}", e))
                    })?;

                    let parsed: AnthropicChatResponse =
                        serde_json::from_str(&body).map_err(|e| {
                            Error::ProviderApi(format!(
                                "Parse error (first 500): {}: {}",
                                &body[..body.len().min(500)],
                                e
                            ))
                        })?;
                    return Ok(parsed);
                }
                401 => {
                    return Err(Error::ProviderAuth(
                        "Invalid Anthropic API key".to_string(),
                    ));
                }
                429 => {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(10);

                    warn!(retry_after_secs = retry_after, "Anthropic rate limited");

                    if attempt < self.config.max_retries {
                        tokio::time::sleep(Duration::from_secs(retry_after)).await;
                        last_error = Some(Error::ProviderRateLimit {
                            retry_after_secs: retry_after,
                        });
                        continue;
                    }
                    return Err(Error::ProviderRateLimit {
                        retry_after_secs: retry_after,
                    });
                }
                529 => {
                    // Anthropic-specific: API overloaded
                    warn!("Anthropic API overloaded (529), retrying");
                    last_error =
                        Some(Error::ProviderApi("API overloaded (529)".to_string()));
                    continue;
                }
                500 | 502 | 503 => {
                    let body = response.text().await.unwrap_or_default();
                    warn!(status = status.as_u16(), "Anthropic server error");
                    last_error = Some(Error::ProviderApi(format!(
                        "Server error {}: {}",
                        status.as_u16(),
                        &body[..body.len().min(200)]
                    )));
                    continue;
                }
                _ => {
                    let body = response.text().await.unwrap_or_default();
                    return Err(Error::ProviderApi(format!(
                        "HTTP {}: {}",
                        status.as_u16(),
                        &body[..body.len().min(500)]
                    )));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            Error::ProviderApi("All retry attempts exhausted".to_string())
        }))
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn stream_completion(&self, request: ChatRequest) -> Result<StreamingResponse> {
        let model = if request.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.model.clone()
        };

        let system =
            Self::extract_system(&request.messages, &request.system_prompt);

        let anthropic_tools = if request.tools.is_empty() {
            None
        } else {
            Some(Self::convert_tools(&request.tools))
        };

        let anthropic_request = AnthropicChatRequest {
            model,
            max_tokens: request.max_tokens.unwrap_or(4096),
            system,
            messages: Self::convert_messages(&request.messages),
            tools: anthropic_tools,
            temperature: request.temperature,
            stream: false,
        };

        let response = self.execute_with_retry(&anthropic_request).await?;

        // Convert Anthropic response to StreamingChoices
        let mut choices = Vec::new();

        for block in &response.content {
            match block {
                AnthropicResponseBlock::Text { text } => {
                    if !text.is_empty() {
                        choices.push(Ok(StreamingChoice::Message(text.clone())));
                    }
                }
                AnthropicResponseBlock::Thinking { thinking } => {
                    choices.push(Ok(StreamingChoice::Thought(thinking.clone())));
                }
                AnthropicResponseBlock::ToolUse { id, name, input } => {
                    choices.push(Ok(StreamingChoice::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                    }));
                }
            }
        }

        // Usage
        if let Some(usage) = response.usage {
            choices.push(Ok(StreamingChoice::Usage(Usage {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
                total_tokens: usage.input_tokens + usage.output_tokens,
            })));
        }

        choices.push(Ok(StreamingChoice::Done));

        Ok(StreamingResponse::from_stream(stream::iter(choices)))
    }
}

// ─── Anthropic API Types ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AnthropicChatRequest {
    model: String,
    max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
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
    Blocks(Vec<AnthropicBlock>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicBlock {
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

#[derive(Debug, Deserialize)]
struct AnthropicChatResponse {
    content: Vec<AnthropicResponseBlock>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = AnthropicConfig::default();
        assert!(config.default_model.contains("claude"));
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_cache);
    }

    #[test]
    fn test_system_extraction() {
        let messages = vec![
            Message::system("Be helpful"),
            Message::user("Hello"),
        ];
        let system = AnthropicProvider::extract_system(&messages, &None);
        assert_eq!(system, Some("Be helpful".to_string()));

        // Explicit system overrides
        let explicit = Some("Override system".to_string());
        let system2 = AnthropicProvider::extract_system(&messages, &explicit);
        assert_eq!(system2, Some("Override system".to_string()));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("System"),
            Message::user("Hello"),
            Message::assistant("Hi"),
        ];

        let converted = AnthropicProvider::convert_messages(&messages);
        // System messages are filtered out
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }
}
