//! # AIMAXXING Providers
//!
//! LLM Provider implementations for AIMAXXING (AI Agent Trade).
//!
//! Includes support for OpenAI, Anthropic, Gemini, etc.

#![warn(missing_docs)]

// Re-export core types for convenience
pub use brain::agent::message::Message;
pub use brain::agent::provider::Provider;
pub use brain::agent::streaming::{StreamingChoice, StreamingResponse};
pub use brain::error::{Error, Result};
pub use brain::skills::tool::ToolDefinition;

pub mod mock;
pub use mock::MockProvider;
pub mod utils;

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "gemini")]
pub mod gemini;

#[cfg(feature = "deepseek")]
pub mod deepseek;

#[cfg(feature = "openrouter")]
pub mod openrouter;

#[cfg(feature = "moonshot")]
pub mod moonshot;

#[cfg(feature = "groq")]
pub mod groq;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "minimax")]
pub mod minimax;

#[cfg(feature = "llama_cpp")]
pub mod llama_cpp;

#[cfg(test)]
mod provider_tests;

/// HTTP client configuration
#[derive(Clone)]
pub struct HttpConfig {
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Connection pool idle timeout
    pub pool_idle_timeout_secs: u64,
    /// Max idle connections per host
    pub pool_max_idle_per_host: usize,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 60,
            pool_idle_timeout_secs: 90,
            pool_max_idle_per_host: 32,
        }
    }
}

impl HttpConfig {
    /// Build a reqwest client
    pub fn build_client(&self) -> Result<reqwest::Client> {
        use std::time::Duration;

        reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .pool_idle_timeout(Duration::from_secs(self.pool_idle_timeout_secs))
            .pool_max_idle_per_host(self.pool_max_idle_per_host)
            .build()
            .map_err(|e| Error::Internal(e.to_string()))
    }
}

/// Factory for creating providers by name
pub fn create_provider(
    name: &str,
    base_url: Option<String>,
    api_key: Option<String>,
) -> Result<std::sync::Arc<dyn Provider>> {
    use std::sync::Arc;

    match name.to_lowercase().as_str() {
        #[cfg(feature = "ollama")]
        "ollama" => {
            let url = base_url.unwrap_or_else(|| {
                std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:11434/v1".into())
            });
            Ok(Arc::new(ollama::Ollama::new(url)?))
        }
        #[cfg(feature = "openai")]
        "openai" => {
            let key = api_key.ok_or_else(|| Error::Internal("OpenAI API key missing".into()))?;
            let provider = if let Some(url) = base_url {
                openai::OpenAI::with_base_url(key, url)?
            } else {
                openai::OpenAI::new(key)?
            };
            Ok(Arc::new(provider))
        }
        #[cfg(feature = "anthropic")]
        "anthropic" => {
            let key = api_key.ok_or_else(|| Error::Internal("Anthropic API key missing".into()))?;
            Ok(Arc::new(anthropic::Anthropic::new(key)?))
        }
        #[cfg(feature = "gemini")]
        "gemini" => {
            let key = api_key.ok_or_else(|| Error::Internal("Gemini API key missing".into()))?;
            Ok(Arc::new(gemini::Gemini::new(key)?))
        }
        #[cfg(feature = "deepseek")]
        "deepseek" => {
            let key = api_key.ok_or_else(|| Error::Internal("DeepSeek API key missing".into()))?;
            Ok(Arc::new(deepseek::DeepSeek::new(key)?))
        }
        #[cfg(feature = "groq")]
        "groq" => {
            let key = api_key.ok_or_else(|| Error::Internal("Groq API key missing".into()))?;
            Ok(Arc::new(groq::Groq::new(key)?))
        }
        #[cfg(feature = "minimax")]
        "minimax" => {
            let key = api_key.ok_or_else(|| Error::Internal("MiniMax API key missing".into()))?;
            Ok(Arc::new(minimax::MiniMax::new(key)?))
        }
        #[cfg(feature = "moonshot")]
        "moonshot" => {
            let key = api_key.ok_or_else(|| Error::Internal("Moonshot API key missing".into()))?;
            Ok(Arc::new(openai::OpenAI::with_base_url(
                key,
                "https://api.moonshot.cn/v1",
            )?))
        }
        #[cfg(feature = "openrouter")]
        "openrouter" => {
            let key =
                api_key.ok_or_else(|| Error::Internal("OpenRouter API key missing".into()))?;
            Ok(Arc::new(openai::OpenAI::with_base_url(
                key,
                "https://openrouter.ai/api/v1",
            )?))
        }
        // Universal OpenAI-compatible carrier
        "custom" | "openai-compatible" | "universal" => {
            let key = api_key.unwrap_or_else(|| "none".to_string());
            let url = base_url
                .ok_or_else(|| Error::Internal("Base URL required for custom provider".into()))?;
            Ok(Arc::new(openai::OpenAI::with_base_url(key, url)?))
        }
        #[cfg(feature = "llama_cpp")]
        "llama-cpp" | "llama_cpp" | "local-gguf" | "gguf" => {
            let path = base_url.ok_or_else(|| {
                Error::Internal("Model path required for llama-cpp provider".into())
            })?;
            Ok(Arc::new(llama_cpp::LlamaCpp::new(path)?))
        }
        "mock" => Ok(Arc::new(mock::MockProvider::new(
            "I am a mock provider".to_string(),
        ))),
        _ => Err(Error::Internal(format!(
            "Unknown or disabled provider: {}",
            name
        ))),
    }
}
