//! OpenAI provider implementation
//!
//! Also compatible with OpenAI-compatible APIs like Groq, Mistral, etc.



use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::{Error, Result, Message, StreamingChoice, StreamingResponse, ToolDefinition, Provider, HttpConfig};
use aimaxxing_core::agent::message::{Role, Content};

/// OpenAI API client
#[derive(Clone)]
pub struct OpenAI {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAI {
    /// Create from API key
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::with_base_url(api_key, "https://api.openai.com/v1")
    }

    /// Create from environment variable
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| Error::ProviderAuth("OPENAI_API_KEY not set".to_string()))?;
        Self::new(api_key)
    }

    /// Create with custom base URL (for compatible APIs)
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self> {
        let config = HttpConfig::default();
        let client = config.build_client()?;

        Ok(Self {
            client,
            api_key: api_key.into(),
            base_url: base_url.into(),
        })
    }

    /// Create for Groq
    pub fn groq(api_key: impl Into<String>) -> Result<Self> {
        Self::with_base_url(api_key, "https://api.groq.com/openai/v1")
    }

    /// Create for Mistral
    pub fn mistral(api_key: impl Into<String>) -> Result<Self> {
        Self::with_base_url(api_key, "https://api.mistral.ai/v1")
    }

    /// Create for MiniMax
    pub fn minimax(api_key: impl Into<String>) -> Result<Self> {
        Self::with_base_url(api_key, "https://api.minimax.io/v1")
    }

    fn build_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .map_err(|e| Error::Internal(e.to_string()))?,
        );
        Ok(headers)
    }
}


#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAIFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIToolFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAITool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

/// Streaming chunk from OpenAI
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<StreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCall {
    index: Option<usize>,
    id: Option<String>,
    function: Option<StreamFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

impl OpenAI {
    fn convert_messages(
        system_prompt: Option<&str>,
        messages: Vec<Message>,
    ) -> Vec<OpenAIMessage> {
        let mut result = Vec::with_capacity(messages.len() + 1);

        // Add system message if present
        if let Some(prompt) = system_prompt {
            result.push(OpenAIMessage {
                role: "system".to_string(),
                content: serde_json::Value::String(prompt.to_string()),
                name: None,
                tool_call_id: None,
                tool_calls: None,
            });
        }

        // Convert messages
        for msg in messages {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            };

            let mut tool_calls = Vec::new();
            let mut tool_call_id = None;
            let final_content: serde_json::Value;

            match msg.content {
                Content::Text(text) => {
                    final_content = serde_json::Value::String(text);
                },
                Content::Parts(parts) => {
                    let mut json_parts = Vec::new();
                    let mut text_acc = String::new();
                    
                    for part in parts {
                        match part {
                            aimaxxing_core::agent::message::ContentPart::Text { text } => {
                                text_acc.push_str(&text);
                                json_parts.push(serde_json::json!({
                                    "type": "text",
                                    "text": text
                                }));
                            },
                                    aimaxxing_core::agent::message::ContentPart::Image { source } => {
                                // Fix #8: Support Images (Url and Base64)
                                let url = match source {
                                    aimaxxing_core::agent::message::ImageSource::Url { url } => url,
                                    aimaxxing_core::agent::message::ImageSource::Base64 { media_type, data } => {
                                        format!("data:{};base64,{}", media_type, data)
                                    }
                                };
                                
                                json_parts.push(serde_json::json!({
                                    "type": "image_url",
                                    "image_url": {
                                        "url": url
                                        // "detail": "auto" // Default
                                    }
                                }));
                            },
                             aimaxxing_core::agent::message::ContentPart::ToolCall { id, name, arguments } => {
                                tool_calls.push(OpenAIToolCall {
                                    id,
                                    call_type: "function".to_string(),
                                    function: OpenAIFunction {
                                        name,
                                        arguments: arguments.to_string(),
                                    },
                                });
                            },
                            aimaxxing_core::agent::message::ContentPart::ToolResult { tool_call_id: id, content, .. } => {
                                tool_call_id = Some(id);
                                text_acc = content; // Tool result content is simple string usually
                            },
                            // Audio/Video skipped for now

                        }
                    }
                    
                    if tool_call_id.is_some() || (!tool_calls.is_empty()) {
                        // If tool related, content is usually null or the text string
                         if text_acc.is_empty() {
                             final_content = serde_json::Value::Null;
                         } else {
                             final_content = serde_json::Value::String(text_acc);
                         }
                    } else if json_parts.iter().any(|p| p["type"] == "image_url") {
                        // Multi-modal content
                         final_content = serde_json::Value::Array(json_parts);
                    } else {
                        // Simple text
                        final_content = serde_json::Value::String(text_acc);
                    }
                }
            }

            result.push(OpenAIMessage {
                role: role.to_string(),
                content: final_content,
                name: msg.name,
                tool_call_id,
                tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            });
        }

        result
    }

    fn convert_tools(tools: Vec<ToolDefinition>) -> Vec<OpenAITool> {
        tools
            .into_iter()
            .map(|t| {
                let description = if let Some(ts) = &t.parameters_ts {
                    format!("{}\n\nUse this TypeScript interface for parameter structure:\n```typescript\n{}\n```", t.description, ts)
                } else {
                    t.description.clone()
                };

                OpenAITool {
                    tool_type: "function".to_string(),
                    function: OpenAIToolFunction {
                        name: t.name,
                        description,
                        parameters: t.parameters,
                    },
                }
            })
            .collect()
    }
}

#[async_trait]
impl Provider for OpenAI {
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
            extra_params,
            enable_cache_control: _,
        } = request;

        // Check for response_format in extra_params
        let response_format = if let Some(params) = &extra_params {
            if let Some(format_val) = params.get("response_format") {
                 serde_json::from_value(format_val.clone()).ok()
            } else {
                None
            }
        } else {
            None
        };

        let request_messages = Self::convert_messages(system_prompt.as_deref(), messages);

        // If tools have TS interfaces, we might want to prioritize them.
        // For OpenAI, we still MUST send the JSON schema in the `tools` parameter.
        // However, we can enhance the system prompt or tool descriptions.
        
        let api_request = OpenAIChatRequest {
            model: model.to_string(),
            messages: request_messages,
            temperature,
            max_tokens,
            tools: Self::convert_tools(tools),
            response_format,
            stream: true,
            stream_options: Some(StreamOptions { include_usage: true }),
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .headers(self.build_headers()?)
            .json(&api_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderApi(format!(
                "OpenAI API error {}: {}",
                status, text
            )));
        }

        // Parse SSE stream
        let stream = response.bytes_stream();
        let parsed_stream = parse_sse_stream(stream);

        Ok(StreamingResponse::from_stream(parsed_stream))
    }

    fn name(&self) -> &'static str {
        "openai"
    }

    fn metadata() -> aimaxxing_core::agent::provider::ProviderMetadata {
        aimaxxing_core::agent::provider::ProviderMetadata {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            description: "Industry standard LLM provider supporting GPT-4o, o1, and more.".to_string(),
            icon: "🤖".to_string(),
            fields: vec![
                aimaxxing_core::agent::provider::ProviderField {
                    key: "OPENAI_API_KEY".to_string(),
                    label: "API Key".to_string(),
                    field_type: "password".to_string(),
                    description: "Your OpenAI API Key".to_string(),
                    required: true,
                    default: None,
                },
                aimaxxing_core::agent::provider::ProviderField {
                    key: "OPENAI_BASE_URL".to_string(),
                    label: "Base URL".to_string(),
                    field_type: "text".to_string(),
                    description: "Optional custom endpoint (e.g. for Groq, OpenRouter)".to_string(),
                    required: false,
                    default: Some("https://api.openai.com/v1".to_string()),
                },
            ],
            capabilities: vec!["vision".into(), "tools".into(), "streaming".into()],
            preferred_models: vec!["gpt-4o".into(), "gpt-4o-mini".into(), "o1-preview".into()],
        }
    }
}

/// Parse Server-Sent Events stream from OpenAI
fn parse_sse_stream<S>(
    stream: S,
) -> impl Stream<Item = std::result::Result<StreamingChoice, Error>>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
{
    // Tool call accumulator state
    struct ToolCallState {
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    }

    let sse_buffer = crate::utils::SseBuffer::new();
    let current_tools: std::collections::HashMap<usize, ToolCallState> = std::collections::HashMap::new();
    let pending_messages: std::collections::VecDeque<String> = std::collections::VecDeque::new();

    futures::stream::unfold(
        (stream, sse_buffer, current_tools, pending_messages),
        move |(mut stream, mut bytes_buffer, mut current_tools, mut pending_messages)| async move {
            loop {
                // 1. Process pending messages from buffer first
                if let Some(message) = pending_messages.pop_front() {
                    // Parse the SSE message
                    if let Some(data) = message.strip_prefix("data: ") {
                        let data = data.trim();
                        if data == "[DONE]" {
                            return Some((Ok(StreamingChoice::Done), (stream, bytes_buffer, current_tools, pending_messages)));
                        }

                        match serde_json::from_str::<StreamChunk>(data) {
                            Ok(chunk) => {
                                // Check for usage (usually in the last chunk with stream_options)
                                if let Some(usage) = chunk.usage {
                                    return Some((
                                        Ok(StreamingChoice::Usage(aimaxxing_core::agent::streaming::Usage {
                                            prompt_tokens: usage.prompt_tokens,
                                            completion_tokens: usage.completion_tokens,
                                            total_tokens: usage.total_tokens,
                                        })),
                                        (stream, bytes_buffer, current_tools, pending_messages),
                                    ));
                                }

                                if let Some(choice) = chunk.choices.first() {
                                    // Check for content
                                    if let Some(content) = &choice.delta.content {
                                        if !content.is_empty() {
                                            return Some((
                                                Ok(StreamingChoice::Message(content.clone())),
                                                (stream, bytes_buffer, current_tools, pending_messages),
                                            ));
                                        }
                                    }

                                    // Check for tool calls
                                    if let Some(tool_calls) = &choice.delta.tool_calls {
                                        for tc in tool_calls {
                                            let index = tc.index.unwrap_or(0);
                                            let state = current_tools.entry(index).or_insert(ToolCallState {
                                                id: None,
                                                name: None,
                                                arguments: String::new(),
                                            });

                                            // Update ID
                                            if let Some(id) = &tc.id {
                                                state.id = Some(id.clone());
                                            }

                                            // Update Name
                                            if let Some(func) = &tc.function {
                                                if let Some(name) = &func.name {
                                                    state.name = Some(name.clone());
                                                }
                                                // Update Arguments
                                                if let Some(args) = &func.arguments {
                                                    state.arguments.push_str(args);
                                                }
                                            }
                                        }
                                    }

                                    // Check if tool calls are complete
                                    if choice.finish_reason.as_deref() == Some("tool_calls") {
                                        let mut tools_map = std::collections::HashMap::new();
                                        for (index, state) in current_tools.drain() {
                                            if let (Some(id), Some(name)) = (state.id, state.name) {
                                                if let Ok(args) = serde_json::from_str(&state.arguments) {
                                                     tools_map.insert(index, aimaxxing_core::agent::message::ToolCall {
                                                        id,
                                                        name,
                                                        arguments: args, 
                                                     });
                                                }
                                            }
                                        }

                                        if !tools_map.is_empty() {
                                            return Some((
                                                Ok(StreamingChoice::ParallelToolCalls(tools_map)),
                                                (stream, bytes_buffer, current_tools, pending_messages),
                                            ));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse SSE chunk: {}", e);
                            }
                        }
                    }
                    continue;
                }

                // 2. Need more data from stream
                match stream.next().await {
                    Some(Ok(bytes)) => {
                        if let Err(e) = bytes_buffer.extend_from_slice(&bytes) {
                             return Some((Err(e), (stream, bytes_buffer, current_tools, pending_messages)));
                        }
                        match bytes_buffer.extract_messages() {
                            Ok(messages) => {
                                pending_messages.extend(messages);
                            }
                            Err(e) => {
                                return Some((Err(e), (stream, bytes_buffer, current_tools, pending_messages)));
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Some((
                            Err(Error::Http(e)),
                            (stream, bytes_buffer, current_tools, pending_messages),
                        ));
                    }
                    None => return None,
                }
            }
        },
    )
}

/// Common model constants
pub const GPT_4O: &str = "gpt-4o";
/// OpenAI Models
pub const GPT_4O_MINI: &str = "gpt-4o-mini";
/// GPT-4 Turbo
pub const GPT_4_TURBO: &str = "gpt-4-turbo";
/// GPT-3.5 Turbo
pub const GPT_35_TURBO: &str = "gpt-3.5-turbo";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let converted = OpenAI::convert_messages(Some("Be helpful"), messages);
        
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
    }
}

// --- Embeddings Implementation ---

use aimaxxing_core::knowledge::rag::Embeddings;

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    input: String,
    model: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embeddings for OpenAI {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbeddingRequest {
            input: text.to_string(),
            model: "text-embedding-3-small".to_string(), // Default to small, cheap model
        };

        let response = self.client
            .post(format!("{}/embeddings", self.base_url))
            .headers(self.build_headers()?)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderApi(format!(
                "OpenAI Embeddings API error {}: {}",
                status, text
            )));
        }

        let body: EmbeddingResponse = response.json().await
            .map_err(|e| Error::ProviderApi(format!("Failed to parse embedding response: {}", e)))?;

        body.data.first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| Error::ProviderApi("No embedding returned".to_string()))
    }
}
