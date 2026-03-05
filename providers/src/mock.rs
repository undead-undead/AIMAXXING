//! Mock provider for testing

use async_trait::async_trait;

use crate::{Result, StreamingResponse, Provider};
use brain::agent::streaming::MockStreamBuilder;

/// A mock provider for testing
pub struct MockProvider {
    /// Response to return
    response: String,
    /// Optional tool calls to inject
    tool_calls: Vec<(String, String, serde_json::Value)>,
}

impl MockProvider {
    /// Create a new mock provider with predefined response
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            tool_calls: Vec::new(),
        }
    }

    /// Create a mock provider that returns tool calls
    pub fn with_tool_calls(
        response: impl Into<String>,
        tool_calls: Vec<(String, String, serde_json::Value)>,
    ) -> Self {
        Self {
            response: response.into(),
            tool_calls,
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(
        &self,
        request: brain::agent::provider::ChatRequest,
    ) -> Result<StreamingResponse> {
        // Simple logic to avoid infinite loops: 
        // Only return tool calls if the last message isn't already a tool result.
        let is_last_tool_result = request.messages.last().map(|m| m.role == brain::agent::message::Role::Tool).unwrap_or(false);

        // Split response into chunks for realistic streaming simulation
        let chunks: Vec<String> = self
            .response
            .chars()
            .collect::<Vec<_>>()
            .chunks(10)
            .map(|c| c.iter().collect())
            .collect();

        let mut builder = MockStreamBuilder::new();
        
        if is_last_tool_result {
            builder = builder.message("I have processed the tool result.");
        } else {
            for chunk in chunks {
                builder = builder.message(chunk);
            }

            for (id, name, args) in &self.tool_calls {
                builder = builder.tool_call(id, name, args.clone());
            }
        }

        builder = builder.done();

        Ok(builder.build())
    }

    fn name(&self) -> &'static str {
        "mock"
    }

    fn metadata() -> brain::agent::provider::ProviderMetadata {
        brain::agent::provider::ProviderMetadata {
            id: "mock".to_string(),
            name: "Mock Provider".to_string(),
            description: "A provider for testing and development".to_string(),
            icon: "🧪".to_string(),
            fields: vec![],
            capabilities: vec!["tools".to_string()],
            preferred_models: vec!["mock-model".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;

    #[tokio::test]
    async fn test_mock_provider() {
        let provider = MockProvider::new("Hello, world!");
        let stream = provider
            .stream_completion(brain::agent::provider::ChatRequest {
                model: "test".to_string(),
                messages: vec![Message::user("Hi")],
                ..Default::default()
            })
            .await
            .expect("should succeed");

        let text = stream.collect_text().await.expect("collect should succeed");
        assert_eq!(text, "Hello, world!");
    }
}

