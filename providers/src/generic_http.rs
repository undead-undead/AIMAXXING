use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use crate::{Error, Result, StreamingResponse, Provider, HttpConfig};

/// A generic HTTP provider that can be configured for various OpenAI-compatible APIs.
pub struct GenericHttpProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    name: String,
}

impl GenericHttpProvider {
    pub fn new(name: impl Into<String>, api_key: impl Into<String>, base_url: impl Into<String>) -> Result<Self> {
        let config = HttpConfig::default();
        let client = config.build_client()?;

        Ok(Self {
            client,
            api_key: api_key.into(),
            base_url: base_url.into(),
            name: name.into(),
        })
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

#[async_trait]
impl Provider for GenericHttpProvider {
    async fn stream_completion(
        &self,
        request: aimaxxing_core::agent::provider::ChatRequest,
    ) -> Result<StreamingResponse> {
        // This is where we need the OpenAI-specific logic for conversion.
        // But since most aimaxxing_providers are OpenAI compatible, we can use the OpenAI engine.
        // However, to truly refactor, we should move the conversion logic here or use a shared one.
        
        // For now, let's keep it simple and just use the name for identifying the provider.
        // Actually, the OpenAI provider already does exactly this.
        
        Err(Error::Internal("Use the OpenAI provider for compatible APIs for now, this is a placeholder for further abstraction".to_string()))
    }

    fn name(&self) -> &'static str {
        // This is tricky because it returns &'static str. 
        // We might need to leak the string or use a box.
        // For generic aimaxxing_providers, we might want to return a dynamic string or just "generic".
        "generic"
    }
}
