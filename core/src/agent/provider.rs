//! Provider trait for LLM integrations

use async_trait::async_trait;

use crate::error::Result;
use crate::agent::message::Message;
use crate::agent::streaming::StreamingResponse;
use crate::skills::tool::ToolDefinition;
use serde::{Deserialize, Serialize};

/// Metadata field for provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderField {
    pub key: String,         // e.g. "openai_api_key"
    pub label: String,       // e.g. "API Key"
    pub field_type: String,  // e.g. "password", "text"
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
}

/// Metadata describing an LLM provider's capabilities and schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub id: String,          // e.g. "openai"
    pub name: String,        // e.g. "OpenAI"
    pub description: String,
    pub icon: String,        // Emoji
    pub fields: Vec<ProviderField>,
    pub capabilities: Vec<String>, // ["vision", "tools", "caching"]
    pub preferred_models: Vec<String>,
}

mod resilient;

pub use resilient::{ResilientProvider, CircuitBreakerConfig};

/// Request for a chat completion
#[derive(Debug, Clone, Default)]
pub struct ChatRequest {
    /// Model name to use
    pub model: String,
    /// Optional system prompt
    pub system_prompt: Option<String>,
    /// Conversation history
    pub messages: Vec<Message>,
    /// Available tools
    pub tools: Vec<ToolDefinition>,
    /// Optional temperature setting
    pub temperature: Option<f64>,
    /// Optional max tokens
    pub max_tokens: Option<u64>,
    /// Optional provider-specific parameters
    pub extra_params: Option<serde_json::Value>,
    /// Whether to enable explicit context caching (e.g. Anthropic cache_control)
    pub enable_cache_control: bool,
}

/// Trait for LLM aimaxxing_providers
///
/// Implement this trait to add support for a new LLM provider.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Stream a completion request
    async fn stream_completion(
        &self,
        request: ChatRequest,
    ) -> Result<StreamingResponse>;

    /// Get provider name (for logging/debugging)
    fn name(&self) -> &'static str;

    /// Check if provider supports streaming
    fn supports_streaming(&self) -> bool {
        true
    }

    /// Check if provider supports tool calls
    fn supports_tools(&self) -> bool {
        true
    }

    /// Get provider metadata schema for panel rendering
    fn metadata() -> ProviderMetadata where Self: Sized;
}

#[async_trait]
impl Provider for std::sync::Arc<dyn Provider> {
    async fn stream_completion(&self, request: ChatRequest) -> Result<StreamingResponse> {
        self.as_ref().stream_completion(request).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }

    fn supports_streaming(&self) -> bool {
        self.as_ref().supports_streaming()
    }

    fn supports_tools(&self) -> bool {
        self.as_ref().supports_tools()
    }

    fn metadata() -> ProviderMetadata where Self: Sized {
        // This is never actually called on Arc<dyn Provider> in the current architecture,
        // as metadata is fetched from concrete types in get_provider_schema.
        ProviderMetadata {
            id: "dynamic".to_string(),
            name: "Dynamic Provider".to_string(),
            description: "A dynamically assigned provider".to_string(),
            icon: "🧬".to_string(),
            fields: vec![],
            capabilities: vec![],
            preferred_models: vec![],
        }
    }
}
#[cfg(test)]
pub struct MockProvider {
    pub response: String,
}

#[cfg(test)]
impl MockProvider {
    pub fn new(response: impl Into<String>) -> Self {
        Self { response: response.into() }
    }
}

#[cfg(test)]
#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(&self, _request: ChatRequest) -> Result<StreamingResponse> {
        Ok(crate::agent::streaming::MockStreamBuilder::new()
            .message(&self.response)
            .done()
            .build())
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}
