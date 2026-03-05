//! Google Gemini provider implementation



use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};

use crate::{Error, Result, Message, StreamingChoice, StreamingResponse, ToolDefinition, Provider, HttpConfig};
use aimaxxing_core::agent::message::{Role, Content};

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// Gemini API client
pub struct Gemini {
    client: reqwest::Client,
    api_key: String,
}

impl Gemini {
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
        let api_key = std::env::var("GEMINI_API_KEY")
            .map_err(|_| Error::ProviderAuth("GEMINI_API_KEY not set".to_string()))?;
        Self::new(api_key)
    }
}

/// Gemini request format
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiTool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Part {
    Text { text: String },
    InlineData(InlineData),
    FunctionCall { function_call: FunctionCall },
    FunctionResponse { function_response: FunctionResponse },
}

#[derive(Debug, Serialize, Deserialize)]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct FunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u64>,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct FunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Streaming response chunk
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamChunk {
    candidates: Option<Vec<Candidate>>,
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsage {
    prompt_token_count: u32,
    candidates_token_count: u32,
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<CandidateContent>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    parts: Option<Vec<ResponsePart>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ResponsePart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: ResponseFunctionCall,
    },
    Thought {
        thought: String,
    },
}

#[derive(Debug, Deserialize)]
struct ResponseFunctionCall {
    name: String,
    args: serde_json::Value,
}

impl Gemini {
    fn convert_messages(messages: Vec<Message>) -> Vec<GeminiContent> {
        messages
            .into_iter()
            .filter(|m| m.role != Role::System)
            .map(|msg| {
                let role = match msg.role {
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "model",
                    Role::System => "user",
                };

                let parts = match msg.content {
                    Content::Text(text) => vec![Part::Text { text }],
                    Content::Parts(content_parts) => content_parts
                        .into_iter()
                        .filter_map(|p| match p {
                            aimaxxing_core::agent::message::ContentPart::Text { text } => Some(Part::Text { text }),
                            aimaxxing_core::agent::message::ContentPart::Image { source } => {
                                let (media_type, data) = match source {
                                    aimaxxing_core::agent::message::ImageSource::Url { url } => {
                                        // Gemini doesn't support URLs directly in the same way as OpenAI, 
                                        // usually needs to be downloaded or provided as data.
                                        // For now, we only support base64 for simplicity in this tool.
                                        ("image/png".to_string(), url) // This is a placeholder for URL support
                                    }
                                    aimaxxing_core::agent::message::ImageSource::Base64 { media_type, data } => {
                                        (media_type, data)
                                    }
                                };
                                Some(Part::InlineData(InlineData {
                                    mime_type: media_type,
                                    data,
                                }))
                            },
                            aimaxxing_core::agent::message::ContentPart::ToolCall { name, arguments, .. } => {
                                Some(Part::FunctionCall {
                                    function_call: FunctionCall {
                                        name,
                                        args: arguments,
                                    }
                                })
                            },
                            aimaxxing_core::agent::message::ContentPart::ToolResult { name, content, .. } => {
                                // Gemini requires a name here. If it's missing, we are in trouble.
                                // We fallback to "unknown" or hope caller provided it.
                                let name = name.unwrap_or_else(|| "unknown".to_string());
                                
                                // Parse content as JSON if possible, otherwise wrap string
                                let response_json = match serde_json::from_str::<serde_json::Value>(&content) {
                                    Ok(v) => v,
                                    Err(_) => serde_json::json!({ "result": content })
                                };
                                
                                Some(Part::FunctionResponse {
                                    function_response: FunctionResponse {
                                        name,
                                        response: response_json,
                                    }
                                })
                            },
                        })
                        .collect(),
                };

                GeminiContent {
                    role: role.to_string(),
                    parts,
                }
            })
            .collect()
    }

    fn convert_tools(tools: Vec<ToolDefinition>) -> Vec<GeminiTool> {
        if tools.is_empty() {
            return vec![];
        }

        vec![GeminiTool {
            function_declarations: tools
                .into_iter()
                .map(|t| FunctionDeclaration {
                    name: t.name,
                    description: t.description,
                    parameters: t.parameters,
                })
                .collect(),
        }]
    }
}

#[async_trait]
impl Provider for Gemini {
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

        let gemini_request = GeminiRequest {
            contents: Self::convert_messages(messages),
            system_instruction: system_prompt.map(|s| GeminiContent {
                role: "user".to_string(),
                parts: vec![Part::Text { text: s }],
            }),
            generation_config: Some(GenerationConfig {
                temperature,
                max_output_tokens: max_tokens,
            }),
            tools: Self::convert_tools(tools),
        };

        let url = format!(
            "{}{}:streamGenerateContent?alt=sse&key={}",
            GEMINI_API_BASE, model, self.api_key
        );

        let response = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(&gemini_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::ProviderApi(format!(
                "Gemini API error {}: {}",
                status, text
            )));
        }

        let stream = response.bytes_stream();
        let parsed_stream = parse_gemini_stream(stream);

        Ok(StreamingResponse::from_stream(parsed_stream))
    }

    fn name(&self) -> &'static str {
        "gemini"
    }

    fn metadata() -> aimaxxing_core::agent::provider::ProviderMetadata {
        aimaxxing_core::agent::provider::ProviderMetadata {
            id: "gemini".to_string(),
            name: "Google Gemini".to_string(),
            description: "Advanced multimodal models (1.5 Pro, Flash) from Google DeepMind.".to_string(),
            icon: "♊".to_string(),
            fields: vec![
                aimaxxing_core::agent::provider::ProviderField {
                    key: "GEMINI_API_KEY".to_string(),
                    label: "API Key".to_string(),
                    field_type: "password".to_string(),
                    description: "Your Google AI Studio API Key".to_string(),
                    required: true,
                    default: None,
                },
            ],
            capabilities: vec!["vision".into(), "tools".into(), "streaming".into()],
            preferred_models: vec![
                "gemini-1.5-pro".into(),
                "gemini-1.5-flash".into(),
                "gemini-2.0-flash-exp-001".into(),
            ],
        }
    }
}

/// Parse SSE stream from Gemini
fn parse_gemini_stream<S>(
    stream: S,
) -> impl Stream<Item = std::result::Result<StreamingChoice, Error>>
where
    S: Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
{
    let sse_buffer = crate::utils::SseBuffer::new();
    let tool_call_counter: usize = 0;
    let pending_messages: std::collections::VecDeque<String> = std::collections::VecDeque::new();

    futures::stream::unfold(
        (stream, sse_buffer, tool_call_counter, pending_messages),
        move |(mut stream, mut bytes_buffer, mut tool_counter, mut pending_messages)| async move {
            loop {
                // 1. Process pending messages from buffer
                if let Some(line) = pending_messages.pop_front() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        match serde_json::from_str::<StreamChunk>(data) {
                            Ok(chunk) => {
                                // Check for usage metadata (usually in the last chunk)
                                if let Some(usage) = chunk.usage_metadata {
                                    return Some((
                                        Ok(StreamingChoice::Usage(aimaxxing_core::agent::streaming::Usage {
                                            prompt_tokens: usage.prompt_token_count,
                                            completion_tokens: usage.candidates_token_count,
                                            total_tokens: usage.total_token_count,
                                        })),
                                        (stream, bytes_buffer, tool_counter, pending_messages),
                                    ));
                                }

                                if let Some(candidates) = chunk.candidates {
                                    if let Some(candidate) = candidates.first() {
                                        // Check finish reason
                                        if candidate.finish_reason.as_deref() == Some("STOP") {
                                            return Some((
                                                Ok(StreamingChoice::Done),
                                                (stream, bytes_buffer, tool_counter, pending_messages),
                                            ));
                                        }

                                        if let Some(content) = &candidate.content {
                                            if let Some(parts) = &content.parts {
                                                for part in parts {
                                                    match part {
                                                        ResponsePart::Text { text } => {
                                                            if !text.is_empty() {
                                                                 return Some((
                                                                    Ok(StreamingChoice::Message(text.clone())),
                                                                    (stream, bytes_buffer, tool_counter, pending_messages),
                                                                ));
                                                            }
                                                        }
                                                        ResponsePart::FunctionCall { function_call } => {
                                                            tool_counter += 1;
                                                            return Some((
                                                                Ok(StreamingChoice::ToolCall {
                                                                    id: format!("call_{}", tool_counter),
                                                                    name: function_call.name.clone(),
                                                                    arguments: function_call.args.clone(),
                                                                }),
                                                                (stream, bytes_buffer, tool_counter, pending_messages),
                                                            ));
                                                        }
                                                        ResponsePart::Thought { thought } => {
                                                            if !thought.is_empty() {
                                                                return Some((
                                                                    Ok(StreamingChoice::Thought(thought.clone())),
                                                                    (stream, bytes_buffer, tool_counter, pending_messages),
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::debug!("Failed to parse Gemini chunk: {}", e);
                            }
                        }
                    }
                    continue;
                }

                // 2. Need more data
                match stream.next().await {
                    Some(Ok(bytes)) => {
                        if let Err(e) = bytes_buffer.extend_from_slice(&bytes) {
                             return Some((Err(e), (stream, bytes_buffer, tool_counter, pending_messages)));
                        }
                        match bytes_buffer.extract_messages() {
                            Ok(messages) => {
                                pending_messages.extend(messages);
                            }
                            Err(e) => {
                                return Some((Err(e), (stream, bytes_buffer, tool_counter, pending_messages)));
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Some((
                            Err(Error::Http(e)),
                            (stream, bytes_buffer, tool_counter, pending_messages),
                        ));
                    }
                    None => return None,
                }
            }
        },
    )
}

/// Common model constants
/// Gemini 3.1 Pro - Google's most intelligent model (2026)
pub const GEMINI_3_1_PRO: &str = "gemini-3.1-pro";
/// Gemini 3 Flash - Frontier performance at scale (2026)
pub const GEMINI_3_FLASH: &str = "gemini-3-flash";
/// Gemini 2.5 Pro (Legacy/Stable)
pub const GEMINI_2_5_PRO: &str = "gemini-2.5-pro";
/// Gemini 2.5 Flash (Legacy/Stable)
pub const GEMINI_2_5_FLASH: &str = "gemini-2.5-flash";
/// Gemini 2.0 Flash
pub const GEMINI_2_0_FLASH: &str = "gemini-2.0-flash-exp";
/// Gemini 1.5 Pro
pub const GEMINI_1_5_PRO: &str = "gemini-1.5-pro";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi!"),
        ];

        let converted = Gemini::convert_messages(messages);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "model");
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

        let converted = Gemini::convert_tools(tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].function_declarations.len(), 1);
    }
}
