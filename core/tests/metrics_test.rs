use aimaxxing_core::agent::core::Agent;
use aimaxxing_core::agent::provider::Provider;
use aimaxxing_core::agent::streaming::StreamingResponse;
use aimaxxing_core::infra::observable::MetricsRegistry;
use async_trait::async_trait;
use futures::stream;
use std::sync::Arc;

struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn stream_completion(
        &self,
        _request: aimaxxing_core::agent::provider::ChatRequest,
    ) -> aimaxxing_core::error::Result<StreamingResponse> {
        let stream = stream::once(async {
            Ok(aimaxxing_core::agent::streaming::StreamingChoice::Message(
                "Hello".to_string(),
            ))
        });
        Ok(StreamingResponse::new(Box::pin(stream)))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_agent_metrics_collection() {
    let registry = Arc::new(MetricsRegistry::new());
    let provider = MockProvider;

    let agent = Agent::builder(provider)
        .name("test_agent")
        .metrics(Arc::clone(&registry))
        .build()
        .unwrap();

    // Execute a simple prompt
    let _ = agent.prompt("test", None).await.unwrap();

    let snapshot = registry.get_snapshot();

    // Check if steps were recorded
    assert!(
        snapshot.contains_key("test_agent:steps_total"),
        "Metrics should contain steps_total"
    );
    if let Some(aimaxxing_core::infra::observable::MetricValue::Counter(count)) =
        snapshot.get("test_agent:steps_total")
    {
        assert!(*count >= 1, "Steps count should be at least 1");
    } else {
        panic!("test_agent:steps_total should be a Counter");
    }
}
