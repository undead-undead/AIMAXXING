//! Test provider implementations (without needing API keys)
//!
//! This tests that all providers can be instantiated correctly.
//!
//! Run with: cargo test --package providers --all-features

#[cfg(test)]
mod provider_tests {
    use crate::Provider;

    #[test]
    #[cfg(feature = "openai")]
    fn test_openai_creation() {
        use crate::openai::OpenAI;
        let provider = OpenAI::new("test-key");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    #[cfg(feature = "anthropic")]
    fn test_anthropic_creation() {
        use crate::anthropic::Anthropic;
        let provider = Anthropic::new("test-key");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    #[cfg(feature = "gemini")]
    fn test_gemini_creation() {
        use crate::gemini::Gemini;
        let provider = Gemini::new("test-key");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "gemini");
    }

    #[test]
    #[cfg(feature = "deepseek")]
    fn test_deepseek_creation() {
        use crate::deepseek::DeepSeek;
        let provider = DeepSeek::new("test-key");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "deepseek");
    }

    #[test]
    #[cfg(feature = "moonshot")]
    fn test_moonshot_creation() {
        use crate::moonshot::Moonshot;
        let provider = Moonshot::new("test-key");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "moonshot");
    }

    #[test]
    #[cfg(feature = "openrouter")]
    fn test_openrouter_creation() {
        use crate::openrouter::OpenRouter;
        let provider = OpenRouter::new("test-key");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "openrouter");
    }

    #[test]
    #[cfg(feature = "groq")]
    fn test_groq_creation() {
        use crate::groq::Groq;
        let provider = Groq::new("test-key");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "groq");
    }

    #[test]
    #[cfg(feature = "ollama")]
    fn test_ollama_creation() {
        use crate::ollama::Ollama;
        let provider = Ollama::new("http://localhost:11434/v1");
        assert!(provider.is_ok());
        let provider = provider.unwrap();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    #[cfg(all(feature = "openai", feature = "groq", feature = "ollama"))]
    fn test_all_providers_unique_names() {
        use crate::groq::Groq;
        use crate::ollama::Ollama;
        use crate::openai::OpenAI;

        let openai = OpenAI::new("test").unwrap();
        let groq = Groq::new("test").unwrap();
        let ollama = Ollama::new("http://localhost:11434/v1").unwrap();

        // Ensure all providers have unique names
        assert_ne!(openai.name(), groq.name());
        assert_ne!(openai.name(), ollama.name());
        assert_ne!(groq.name(), ollama.name());
    }
}
