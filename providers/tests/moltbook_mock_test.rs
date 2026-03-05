use aimaxxing_core::prelude::*;
use aimaxxing_core::skills::tool::{Tool, ToolDefinition};
use aimaxxing_core::error::{Error};
use aimaxxing_providers::mock::MockProvider;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;

const MOLTBOOK_BASE_URL: &str = "https://www.moltbook.com/api/v1";

// --- Tool Implementations Copied from moltbook_agent.rs ---

struct RegisterMoltbook;
#[async_trait]
impl Tool for RegisterMoltbook {
    fn name(&self) -> String { "register_moltbook".to_string() }
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "register_moltbook".to_string(),
            description: "Register a new agent on Moltbook".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "description": { "type": "string" }
                },
                "required": ["name", "description"]
            }),
            is_binary: false,
            is_verified: true,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }
    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        // Mocking the network call for test stability
        Ok("✅ Registered! API Key: MOCK_KEY_123".to_string())
    }
}

struct GetFeed;
#[async_trait]
impl Tool for GetFeed {
    fn name(&self) -> String { "get_feed".to_string() }
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_feed".to_string(),
            description: "Get recent posts".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "limit": { "type": "number" } }
            }),
            is_binary: false,
            is_verified: true,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }
    async fn call(&self, _arguments: &str) -> anyhow::Result<String> {
        Ok("📰 Recent Posts:\n• Hello World (by @user1)\n• AIMAXXING Rocks (by @dev)".to_string())
    }
}

// --- Integration Test Case ---

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_moltbook_complex_flow_with_mocks() {
    tracing_subscriber::fmt::try_init().ok();

    println!("🚀 Starting Moltbook Complex Flow Mock Test");

    // Scenario: User asks to see the feed, the model decides to register first becuase it has no key,
    // then it sees the feed. (We'll simplify to just two tools in sequence for the mock)
    
    // 1. Mock first turn: Model sees it needs to register
    let mock_response = "I need to register first.";
    let tool_calls = vec![
        ("call_reg".to_string(), "register_moltbook".to_string(), json!({
            "name": "MockBot",
            "description": "I am a mock"
        }))
    ];
    
    let provider = MockProvider::with_tool_calls(mock_response, tool_calls);

    // 2. Build Agent
    let agent = Agent::builder(provider)
        .model("mock-gpt")
        .preamble("You are a Moltbook agent.")
        .tool(RegisterMoltbook)
        .tool(GetFeed)
        .build()
        .expect("Failed to build agent");

    // 3. Prompt
    println!("👤 User: Show me the feed.");
    let response = agent.prompt("Show me the feed.", None).await.unwrap();

    println!("🤖 Agent: {}", response);

    // 4. Verification
    // The Agent logic will:
    // - Call stream_chat
    // - Receive text "I need to register first." + ToolCall "register_moltbook"
    // - Execute RegisterMoltbook.call() -> returns "✅ Registered!..."
    // - Append tool result to history
    // - Call stream_chat again
    // - MockProvider detects last message is Role::Tool, returns "I have processed the tool result."
    
    assert!(response.contains("processed the tool result"));
    assert!(response.contains("I have processed the tool result") || response.contains("Registered"));

    println!("✅ Complex flow test passed!");
}
