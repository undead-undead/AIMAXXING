//! OpenAI-Compatible Provider
//!
//! Supports any API that implements the OpenAI Chat Completions format,
//! including: OpenAI, DeepSeek, Groq, Together, Ollama, vLLM, etc.
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

/// Configuration for an OpenAI-compatible provider.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// API base URL (e.g. "https://api.openai.com/v1")
    pub base_url: String,
    /// API key (can be empty for local models e.g. Ollama)
    pub api_key: String,
    /// Default model name (can be overridden per request)
    pub default_model: String,
    /// Provider display name (for logging)
    pub name: String,
    /// Request timeout
    pub timeout: Duration,
    /// Maximum retry attempts on transient errors (429, 500, 502, 503)
    pub max_retries: u32,
    /// Base delay for exponential backoff
    pub retry_base_delay: Duration,
    /// Optional organization ID (OpenAI specific)
    pub organization: Option<String>,
}

impl Default for OpenAiCompatConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            default_model: "gpt-4o-mini".to_string(),
            name: "openai".to_string(),
            timeout: Duration::from_secs(120),
            max_retries: 3,
            retry_base_delay: Duration::from_millis(500),
            organization: None,
        }
    }
}

impl OpenAiCompatConfig {
    /// Create from environment variables.
    /// Reads: OPENAI_API_KEY, OPENAI_BASE_URL, OPENAI_MODEL, OPENAI_ORG_ID
    pub fn from_env() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            default_model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            organization: std::env::var("OPENAI_ORG_ID").ok(),
            ..Default::default()
        }
    }

    /// Create a DeepSeek configuration.
    pub fn deepseek() -> Self {
        Self {
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: std::env::var("DEEPSEEK_API_KEY").unwrap_or_default(),
            default_model: "deepseek-chat".to_string(),
            name: "deepseek".to_string(),
            ..Default::default()
        }
    }

    /// Create an Ollama (local) configuration.
    pub fn ollama() -> Self {
        Self {
            base_url: std::env::var("OLLAMA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
            api_key: String::new(), // Ollama doesn't need a key
            default_model: std::env::var("OLLAMA_MODEL")
                .unwrap_or_else(|_| "llama3".to_string()),
            name: "ollama".to_string(),
            timeout: Duration::from_secs(300), // Local models can be slow
            max_retries: 1,
            ..Default::default()
        }
    }
}

/// An OpenAI-compatible LLM provider with retry, timeout, and error categorization.
pub struct OpenAiCompatProvider {
    config: OpenAiCompatConfig,
    client: Client,
}

impl OpenAiCompatProvider {
    /// Create a new provider from config.
    pub fn new(config: OpenAiCompatConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .pool_max_idle_per_host(4)
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    /// Create from environment variables.
    pub fn from_env() -> Result<Self> {
        Self::new(OpenAiCompatConfig::from_env())
    }

    /// Convert AIMAXXING messages to OpenAI format.
    fn convert_messages(messages: &[Message]) -> Vec<OaiMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                // Handle tool results specially
                if msg.role == Role::Tool {
                    if let Content::Parts(parts) = &msg.content {
                        for part in parts {
                            if let ContentPart::ToolResult {
                                tool_call_id,
                                content,
                                ..
                            } = part
                            {
                                return OaiMessage {
                                    role: "tool".to_string(),
                                    content: Some(content.clone()),
                                    tool_call_id: Some(tool_call_id.clone()),
                                    tool_calls: None,
                                    name: None,
                                };
                            }
                        }
                    }
                }

                OaiMessage {
                    role: role.to_string(),
                    content: Some(msg.text()),
                    tool_call_id: None,
                    tool_calls: None,
                    name: msg.name.clone(),
                }
            })
            .collect()
    }

    /// Convert AIMAXXING tool definitions to OpenAI format.
    fn convert_tools(
        tools: &[crate::skills::tool::ToolDefinition],
    ) -> Vec<OaiTool> {
        tools
            .iter()
            .map(|t| OaiTool {
                r#type: "function".to_string(),
                function: OaiFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    /// Execute a request with exponential backoff retry.
    async fn execute_with_retry(
        &self,
        request: &OaiChatRequest,
    ) -> Result<OaiChatResponse> {
        let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.config.retry_base_delay * 2u32.pow(attempt - 1);
                debug!(
                    provider = %self.config.name,
                    attempt = attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying after transient error"
                );
                tokio::time::sleep(delay).await;
            }

            let mut req = self
                .client
                .post(&url)
                .header("Content-Type", "application/json");

            if !self.config.api_key.is_empty() {
                req = req.header(
                    "Authorization",
                    format!("Bearer {}", self.config.api_key),
                );
            }

            if let Some(ref org) = self.config.organization {
                req = req.header("OpenAI-Organization", org);
            }

            let response = match req.json(request).send().await {
                Ok(r) => r,
                Err(e) => {
                    if e.is_timeout() {
                        warn!(
                            provider = %self.config.name,
                            attempt = attempt,
                            "Request timed out"
                        );
                        last_error = Some(Error::ProviderApi(format!("Timeout: {}", e)));
                        continue;
                    }
                    if e.is_connect() {
                        last_error = Some(Error::ProviderApi(format!(
                            "Connection error (is {} reachable?): {}",
                            self.config.base_url, e
                        )));
                        continue;
                    }
                    return Err(Error::ProviderApi(format!("Request failed: {}", e)));
                }
            };

            let status = response.status();

            // Categorize HTTP status codes
            match status.as_u16() {
                200..=299 => {
                    let body = response.text().await.map_err(|e| {
                        Error::ProviderApi(format!("Failed to read response body: {}", e))
                    })?;

                    let parsed: OaiChatResponse = serde_json::from_str(&body).map_err(|e| {
                        Error::ProviderApi(format!(
                            "Failed to parse response (first 500 chars): {}: {}",
                            &body[..body.len().min(500)],
                            e
                        ))
                    })?;
                    return Ok(parsed);
                }
                401 => {
                    return Err(Error::ProviderAuth(format!(
                        "Invalid API key for provider '{}'",
                        self.config.name
                    )));
                }
                429 => {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(5);

                    warn!(
                        provider = %self.config.name,
                        retry_after_secs = retry_after,
                        "Rate limited"
                    );

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
                500 | 502 | 503 => {
                    let body = response.text().await.unwrap_or_default();
                    warn!(
                        provider = %self.config.name,
                        status = status.as_u16(),
                        "Server error, retrying"
                    );
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
impl Provider for OpenAiCompatProvider {
    fn name(&self) -> &'static str {
        // Leak to get 'static — safe because providers live for program duration
        Box::leak(self.config.name.clone().into_boxed_str())
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

        let oai_tools = if request.tools.is_empty() {
            None
        } else {
            Some(Self::convert_tools(&request.tools))
        };

        let mut messages = Vec::new();
        if let Some(ref sp) = request.system_prompt {
            messages.push(OaiMessage {
                role: "system".to_string(),
                content: Some(sp.clone()),
                tool_call_id: None,
                tool_calls: None,
                name: None,
            });
        }
        messages.extend(Self::convert_messages(&request.messages));

        let oai_request = OaiChatRequest {
            model,
            messages,
            tools: oai_tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
        };

        let response = self.execute_with_retry(&oai_request).await?;

        // Convert to StreamingResponse
        let mut choices = Vec::new();

        if let Some(choice) = response.choices.first() {
            // Text content
            if let Some(ref content) = choice.message.content {
                if !content.is_empty() {
                    choices.push(Ok(StreamingChoice::Message(content.clone())));
                }
            }

            // Tool calls
            if let Some(ref tool_calls) = choice.message.tool_calls {
                for tc in tool_calls {
                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                    choices.push(Ok(StreamingChoice::ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: args,
                    }));
                }
            }
        }

        // Usage
        if let Some(usage) = response.usage {
            choices.push(Ok(StreamingChoice::Usage(Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            })));
        }

        choices.push(Ok(StreamingChoice::Done));

        Ok(StreamingResponse::from_stream(stream::iter(choices)))
    }
}

// ─── OpenAI API Types ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OaiChatRequest {
    model: String,
    messages: Vec<OaiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OaiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OaiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OaiResponseToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct OaiTool {
    r#type: String,
    function: OaiFunction,
}

#[derive(Debug, Serialize)]
struct OaiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OaiChatResponse {
    choices: Vec<OaiChoice>,
    usage: Option<OaiUsage>,
}

#[derive(Debug, Deserialize)]
struct OaiChoice {
    message: OaiResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OaiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OaiResponseToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaiResponseToolCall {
    id: String,
    r#type: String,
    function: OaiResponseFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OaiResponseFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = OpenAiCompatConfig::default();
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_deepseek() {
        let config = OpenAiCompatConfig::deepseek();
        assert!(config.base_url.contains("deepseek"));
        assert_eq!(config.default_model, "deepseek-chat");
    }

    #[test]
    fn test_config_ollama() {
        let config = OpenAiCompatConfig::ollama();
        assert!(config.base_url.contains("localhost"));
        assert!(config.api_key.is_empty());
        assert_eq!(config.timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let converted = OpenAiCompatProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
    }
}
