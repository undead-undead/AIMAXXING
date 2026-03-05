//! Ollama provider implementation
//!
//! Ollama enables local model execution with OpenAI-compatible API.
//! Perfect for protecting proprietary trading strategies and reducing costs.

use async_trait::async_trait;

use crate::{Result, StreamingResponse, Provider};
use crate::openai::OpenAI;

/// Ollama API client (OpenAI compatible)
/// 
/// Ollama runs LLMs locally, providing:
/// - Complete privacy for trading strategies
/// - Zero API costs
/// - No rate limits
/// - Full control over model deployment
pub struct Ollama {
    inner: OpenAI,
}

impl Ollama {
    /// Create with custom Ollama server URL
    /// 
    /// Default Ollama URL is `http://localhost:11434/v1`
    /// 
    /// # Example
    /// ```no_run
    /// use providers::ollama::Ollama;
    /// 
    /// // Connect to local Ollama instance
    /// let ollama = Ollama::new("http://localhost:11434/v1").unwrap();
    /// 
    /// // Connect to remote Ollama server
    /// let ollama = Ollama::new("http://192.168.1.100:11434/v1").unwrap();
    /// ```
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        // Ollama doesn't require an API key, use dummy key
        let inner = OpenAI::with_base_url("ollama", base_url)?;
        Ok(Self { inner })
    }

    /// Create with default local Ollama server
    /// 
    /// Uses `http://localhost:11434/v1` as the base URL.
    /// Can be overridden with `OLLAMA_BASE_URL` environment variable.
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
        Self::new(base_url)
    }
}

#[async_trait]
impl Provider for Ollama {
    async fn stream_completion(
        &self,
        request: brain::agent::provider::ChatRequest,
    ) -> Result<StreamingResponse> {
        self.inner.stream_completion(request).await
    }

    fn name(&self) -> &'static str {
        "ollama"
    }

    fn metadata() -> brain::agent::provider::ProviderMetadata {
        brain::agent::provider::ProviderMetadata {
            id: "ollama".to_string(),
            name: "Ollama (Local)".to_string(),
            description: "Run large language models locally on your own machine.".to_string(),
            icon: "🦙".to_string(),
            fields: vec![
                brain::agent::provider::ProviderField {
                    key: "OLLAMA_BASE_URL".to_string(),
                    label: "Base URL".to_string(),
                    field_type: "text".to_string(),
                    description: "Local Ollama API endpoint".to_string(),
                    required: true,
                    default: Some("http://localhost:11434/v1".to_string()),
                },
            ],
            capabilities: vec!["local".into(), "streaming".into()],
            preferred_models: vec!["llama3.1".into(), "mistral".into(), "qwen2.5".into()],
        }
    }
}

/// Ollama model constants (popular models for trading)
/// Llama 3.1 8B - Fast and efficient
pub const LLAMA_3_1_8B: &str = "llama3.1:8b";
/// Llama 3.1 70B - Most capable
pub const LLAMA_3_1_70B: &str = "llama3.1:70b";
/// Llama 3.2 3B - Lightweight
pub const LLAMA_3_2_3B: &str = "llama3.2:3b";
/// Mistral 7B - Balanced performance
pub const MISTRAL_7B: &str = "mistral:7b";
/// DeepSeek Coder - For strategy development
pub const DEEPSEEK_CODER_6_7B: &str = "deepseek-coder:6.7b";
/// Qwen 2.5 7B - Alibaba's model
pub const QWEN_2_5_7B: &str = "qwen2.5:7b";
/// Gemma 2 9B - Google's model
pub const GEMMA_2_9B: &str = "gemma2:9b";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_creation() {
        let result = Ollama::new("http://localhost:11434/v1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_ollama_from_env() {
        let result = Ollama::from_env();
        assert!(result.is_ok());
    }
}
