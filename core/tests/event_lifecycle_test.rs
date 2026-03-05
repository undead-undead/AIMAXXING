use aimaxxing_core::agent::core::AgentEvent;
use aimaxxing_core::agent::provider::{ChatRequest, Provider};
use aimaxxing_core::agent::streaming::{StreamingChoice, StreamingResponse};
use aimaxxing_core::prelude::*;
use aimaxxing_core::skills::tool::ToolSet;
use async_trait::async_trait;
use futures::stream;
use std::sync::Arc;

struct MockProvider;
#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(
        &self,
        request: ChatRequest,
    ) -> aimaxxing_core::error::Result<StreamingResponse> {
        let has_tool_result = request
            .messages
            .iter()
            .any(|m| matches!(m.role, Role::Tool));
        println!(
            "[MockProvider] Message count: {}, has_tool_result: {}",
            request.messages.len(),
            has_tool_result
        );

        for (i, m) in request.messages.iter().enumerate() {
            println!(
                "[MockProvider] Msg {}: {:?} ({} chars)",
                i,
                m.role,
                m.content.as_text().len()
            );
        }

        let choice = if has_tool_result {
            println!("[MockProvider] Returning Message");
            StreamingChoice::Message("Done".into())
        } else {
            println!("[MockProvider] Returning ToolCall");
            StreamingChoice::ToolCall {
                id: "call_1".into(),
                name: "test_tool".into(),
                arguments: serde_json::json!({}),
            }
        };

        let stream = stream::iter(vec![Ok(choice), Ok(StreamingChoice::Done)]);
        Ok(StreamingResponse::from_stream(Box::pin(stream)))
    }
    fn name(&self) -> &'static str {
        "mock"
    }
}

struct TestTool;
#[async_trait]
impl aimaxxing_core::skills::tool::Tool for TestTool {
    fn name(&self) -> String {
        "test_tool".into()
    }
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "test_tool".into(),
            description: "A test tool".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
            parameters_ts: None,
            is_binary: false,
            is_verified: true,
            usage_guidelines: None,
        }
    }
    async fn call(&self, _args: &str) -> anyhow::Result<String> {
        Ok("tool_executed".into())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tool_events() {
    println!("[Test] Tool setup starting");
    let mut tools = ToolSet::new();
    tools.add(TestTool);

    let agent = AgentBuilder::new(MockProvider)
        .model("mock")
        .tools(tools)
        .build()
        .unwrap();

    let mut rx = agent.subscribe();

    println!("[Test] Agent prompt starting");
    let agent_handle = tokio::spawn(async move { agent.prompt("trigger tool", None).await });

    println!("[Test] Event collection starting");
    let mut tool_start = false;
    let mut tool_end = false;

    println!("[Test] Waiting for events...");
    loop {
        match rx.recv().await {
            Ok(event) => {
                println!("[Test] Received event: {:?}", event);
                if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
                    tool_start = true;
                }
                if matches!(event, AgentEvent::ToolExecutionEnd { .. }) {
                    tool_end = true;
                }
                if matches!(event, AgentEvent::Response { .. })
                    || matches!(event, AgentEvent::Error { .. })
                {
                    println!("[Test] Termination event received. Breaking loop.");
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                println!("[Test] Receiver lagged by {} events", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                println!("[Test] Channel closed. Breaking loop.");
                break;
            }
        }
    }

    let res = agent_handle.await.unwrap().unwrap();
    println!("[Test] Agent response: {}", res);

    assert!(tool_start, "Missing ToolExecutionStart");
    assert!(tool_end, "Missing ToolExecutionEnd");
    println!("[Test] SUCCESS: Tool events verified");
}
