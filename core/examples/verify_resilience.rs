use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::provider::CircuitBreakerConfig;
use std::time::Duration;

#[derive(Clone)]
struct MockProvider {
    name: String,
    should_fail: Arc<Mutex<bool>>,
    fail_count: Arc<Mutex<u32>>,
}

impl MockProvider {
    fn new(name: &str, should_fail: bool) -> Self {
        Self {
            name: name.to_string(),
            should_fail: Arc::new(Mutex::new(should_fail)),
            fail_count: Arc::new(Mutex::new(0)),
        }
    }

    fn set_fail(&self, fail: bool) {
        let mut f = self.should_fail.lock().unwrap();
        *f = fail;
    }

    fn get_fail_count(&self) -> u32 {
        *self.fail_count.lock().unwrap()
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    async fn stream_completion(
        &self,
        _request: aimaxxing_core::agent::provider::ChatRequest,
    ) -> Result<StreamingResponse> {
        let fail = *self.should_fail.lock().unwrap();
        if fail {
            let mut count = self.fail_count.lock().unwrap();
            *count += 1;
            println!("[{}] Simulating failure (Count: {})", self.name, *count);
            Err(Error::ProviderApi("Simulated Failure".to_string()))
        } else {
            println!("[{}] Request succeeded", self.name);
            use futures::stream;
            let stream = stream::iter(vec![Ok(StreamingChoice::Message(format!("Response from {}", self.name)))]);
            Ok(StreamingResponse::from_stream(stream))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup tracing
    tracing_subscriber::fmt::init();

    println!("--- Starting ResilientProvider Verification ---");

    let primary = MockProvider::new("Primary", true); // Starts failing
    let fallback = MockProvider::new("Fallback", false); // Always succeeds

    // Config: 2 failures to open currently
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        reset_timeout: Duration::from_millis(500), // Fast reset for test
        request_timeout: Duration::from_secs(1),
    };

    let resilient_provider = aimaxxing_core::agent::provider::ResilientProvider::new(
        primary.clone(),
        fallback.clone(),
        config
    );

    let request = aimaxxing_core::agent::provider::ChatRequest {
        model: "model".to_string(),
        messages: vec![Message::user("test")],
        ..Default::default()
    };
    
    // 1. First Failure (Primary)
    println!("\nRequest 1 (Expect Primary Fail):");
    let _ = resilient_provider.stream_completion(request.clone()).await?;
    // Note: ResilientProvider falls back immediately on failure, so this call should succeed via Fallback!

    // 2. Second Failure (Primary) -> Circuit Should Open
    println!("\nRequest 2 (Expect Primary Fail -> Open Circuit):");
    let _ = resilient_provider.stream_completion(request.clone()).await?;
    
    // 3. Third Request -> Circuit is Open, should skip Primary immediately and use Fallback
    println!("\nRequest 3 (Expect Skip Primary):");
    primary.set_fail(false); // Fix primary, but circuit is OPEN so it shouldn't be called yet
    let _ = resilient_provider.stream_completion(request.clone()).await?;

    // 4. Wait for Half-Open
    println!("\nWaiting for Reset Timeout...");
    tokio::time::sleep(Duration::from_millis(600)).await;

    // 5. Fourth Request -> Half-Open, should try Primary (which is now fixed)
    println!("\nRequest 4 (Expect Primary Success -> Close Circuit):");
    let resp = resilient_provider.stream_completion(request).await?;
    
    // Verify response comes from Primary
    use futures::StreamExt;
    let mut stream = resp.into_inner();
    if let Some(Ok(StreamingChoice::Message(content))) = stream.next().await {
        println!("Final Response: {}", content);
        assert!(content.contains("Primary"));
    }

    println!("\n--- Resilience Verification Passed! ---");
    Ok(())
}
