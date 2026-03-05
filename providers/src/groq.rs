//! Groq provider implementation
//!
//! Groq provides ultra-fast inference with OpenAI-compatible API.
//! Perfect for real-time trading decisions requiring low latency.

use async_trait::async_trait;

use crate::{Error, Result, StreamingResponse, Provider};
use crate::openai::OpenAI;

/// Groq API client (OpenAI compatible)
/// 
/// Groq delivers industry-leading inference speed, making it ideal for:
/// - Real-time market analysis
/// - Fast decision-making in trading scenarios
/// - Low-latency agent responses
pub struct Groq {
    inner: OpenAI,
}

impl Groq {
    /// Create from API key
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let inner = OpenAI::with_base_url(api_key, "https://api.groq.com/openai/v1")?;
        Ok(Self { inner })
    }

    /// Create from environment variable GROQ_API_KEY
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("GROQ_API_KEY")
            .map_err(|_| Error::ProviderAuth("GROQ_API_KEY not set".to_string()))?;
        Self::new(api_key)
    }
}

#[async_trait]
impl Provider for Groq {
    async fn stream_completion(
        &self,
        request: brain::agent::provider::ChatRequest,
    ) -> Result<StreamingResponse> {
        self.inner.stream_completion(request).await
    }

    fn name(&self) -> &'static str {
        "groq"
    }

    fn metadata() -> brain::agent::provider::ProviderMetadata {
        brain::agent::provider::ProviderMetadata {
            id: "groq".to_string(),
            name: "Groq".to_string(),
            description: "Ultra-fast LPU inference (Llama, Mixtral, Gemma).".to_string(),
            icon: "⚡".to_string(),
            fields: vec![
                brain::agent::provider::ProviderField {
                    key: "GROQ_API_KEY".to_string(),
                    label: "API Key".to_string(),
                    field_type: "password".to_string(),
                    description: "Your Groq API Key".to_string(),
                    required: true,
                    default: None,
                },
            ],
            capabilities: vec!["streaming".into(), "fast_inference".into()],
            preferred_models: vec![
                "llama-3.3-70b-versatile".into(),
                "llama-3.1-70b-versatile".into(),
                "llama-3.1-8b-instant".into(),
            ],
        }
    }
}

/// Groq model constants
/// Ultra-fast Llama 3.3 70B - Best for trading decisions
pub const LLAMA_3_3_70B: &str = "llama-3.3-70b-versatile";
/// Llama 3.1 70B - Balanced performance
pub const LLAMA_3_1_70B: &str = "llama-3.1-70b-versatile";
/// Llama 3.1 8B - Lightweight and fast
pub const LLAMA_3_1_8B: &str = "llama-3.1-8b-instant";
/// Mixtral 8x7B - MoE model
pub const MIXTRAL_8X7B: &str = "mixtral-8x7b-32768";
/// Gemma 2 9B - Google's efficient model
pub const GEMMA_2_9B: &str = "gemma2-9b-it";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_groq_creation() {
        let result = Groq::new("test-api-key");
        assert!(result.is_ok());
    }
}
