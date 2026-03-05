use async_trait::async_trait;
use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::provider::{Provider, ChatRequest};
use aimaxxing_core::agent::streaming::{StreamingResponse, StreamingChoice};
use aimaxxing_core::skills::tool::ToolSet;
use aimaxxing_core::agent::core::AgentEvent;
use futures::stream;
use std::sync::Arc;

struct MockProvider;
#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(&self, request: ChatRequest) -> aimaxxing_core::error::Result<StreamingResponse> {
        let has_tool_result = request.messages.iter().any(|m| matches!(m.role, Role::Tool));
        println!("[MockProvider] Message count: {}, has_tool_result: {}", request.messages.len(), has_tool_result);
        
        for (i, m) in request.messages.iter().enumerate() {
            println!("[MockProvider] Msg {}: {:?} ({} chars)", i, m.role, m.content.as_text().len());
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
    fn name(&self) -> &'static str { "mock" }
}

struct TestTool;
#[async_trait]
impl aimaxxing_core::skills::tool::Tool for TestTool {
    fn name(&self) -> String { "test_tool".into() }
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
    let agent_handle = tokio::spawn(async move {
        agent.prompt("trigger tool").await
    });

    println!("[Test] Event collection starting");
    let mut tool_start = false;
    let mut tool_end = false;

    while let Ok(event) = rx.recv().await {
        println!("[Test] Received event: {:?}", event);
        if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
            tool_start = true;
        }
        if matches!(event, AgentEvent::ToolExecutionEnd { .. }) {
            tool_end = true;
        }
        if matches!(event, AgentEvent::Response { .. }) {
            break;
        }
    }

    let res = agent_handle.await.unwrap().unwrap();
    println!("[Test] Agent response: {}", res);
    
    assert!(tool_start, "Missing ToolExecutionStart");
    assert!(tool_end, "Missing ToolExecutionEnd");
    println!("[Test] SUCCESS: Tool events verified");
}
