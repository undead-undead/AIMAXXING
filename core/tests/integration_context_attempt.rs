use brain::agent::provider::{ChatRequest, Provider};
use brain::agent::streaming::{MockStreamBuilder, StreamingResponse};
use brain::agent::Agent;
use brain::error::{Error, Result};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// A mock provider that simulates context overflow on the first attempt
struct MockOverflowProvider {
    attempts: Arc<Mutex<usize>>,
}

impl MockOverflowProvider {
    fn new() -> Self {
        Self {
            attempts: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait]
impl Provider for MockOverflowProvider {
    async fn stream_completion(&self, request: ChatRequest) -> Result<StreamingResponse> {
        let mut attempts = self.attempts.lock().unwrap();
        *attempts += 1;

        // Check if we are receiving the "compressed" signal (concise directive)
        // This confirms the strategy was downgraded
        let is_compressed = request
            .messages
            .iter()
            .any(|m| m.content.as_text().contains("NOTICE: Context compressed"));

        if *attempts == 1 {
            // First attempt: Fail with context error
            // Using a string that matches the regex in core.rs
            return Err(Error::ProviderApi(
                "Error: context length exceeded limit".to_string(),
            ));
        }

        // Second attempt: Should be compressed
        if *attempts == 2 {
            if is_compressed {
                let builder = MockStreamBuilder::new()
                    .message("Recovered with compressed context!")
                    .done();
                return Ok(builder.build());
            } else {
                return Err(Error::ProviderApi(
                    "Failed: Expected compressed context on retry".to_string(),
                ));
            }
        }

        // Subsequent attempts
        let builder = MockStreamBuilder::new().message("Normal response").done();
        Ok(builder.build())
    }

    fn name(&self) -> &'static str {
        "mock-overflow"
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_attempt_recovery() {
    // 1. Setup Mock Provider
    let provider = MockOverflowProvider::new();
    let attempts_tracker = provider.attempts.clone();

    // 2. Build Agent
    // We intentionally don't set a system prompt to verify the injected one
    let agent = Agent::builder(provider)
        .max_history_messages(10)
        .build()
        .expect("Failed to build agent");

    // 3. Run Prompt
    // This should fail internally once, then retry with compressed strategy, then succeed.
    let response = agent.prompt("Hello world", None).await;

    // 4. Verification
    match response {
        Ok(text) => {
            assert_eq!(text, "Recovered with compressed context!");
            assert_eq!(
                *attempts_tracker.lock().unwrap(),
                2,
                "Should have retried exactly once"
            );
        }
        Err(e) => {
            panic!("Agent failed to recover: {}", e);
        }
    }
}
