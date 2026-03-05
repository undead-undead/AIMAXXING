use async_trait::async_trait;
use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::provider::{Provider, ChatRequest};
use aimaxxing_core::agent::streaming::{StreamingResponse, StreamingChoice};
use aimaxxing_core::skills::tool::ToolSet;
use futures::stream;
use std::sync::Arc;

struct MockProvider;
#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(&self, _request: ChatRequest) -> aimaxxing_core::error::Result<StreamingResponse> {
        let stream = stream::iter(vec![
            Ok(StreamingChoice::Message("Hello".into())),
            Ok(StreamingChoice::Done),
        ]);
        Ok(StreamingResponse::from_stream(Box::pin(stream)))
    }
    fn name(&self) -> &'static str { "mock" }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_minimal_agent() {
    println!("MINIMAL AGENT TEST STARTED");
    
    let agent = AgentBuilder::new(MockProvider)
        .model("mock")
        .build()
        .unwrap();

    println!("Agent created");
    let res = agent.prompt("hi").await.unwrap();
    println!("Response: {}", res);
    assert_eq!(res, "Hello");
}
