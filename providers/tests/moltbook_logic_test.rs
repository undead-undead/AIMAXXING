use brain::prelude::*;
use providers::mock::MockProvider;
use serde_json::json;

// Note: In a real integration test, you would import your tools from your crate.
// For this demonstration, I'll use a simplified version of the Moltbook tools logic.

// 1. Define the actual tool for the test
struct RegisterMoltbook;
#[async_trait::async_trait]
impl Tool for RegisterMoltbook {
    fn name(&self) -> String { "register_moltbook".to_string() }
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "register_moltbook".to_string(),
            description: "Register".to_string(),
            parameters: json!({"type": "object"}),
            is_binary: false,
            is_verified: true,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }
    async fn call(&self, _args: &str) -> anyhow::Result<String> {
        Ok("Registration Successful!".to_string())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_moltbook_registration_flow() {
    // 2. Setup Mock Provider
    let mock_response = "I will register the agent now.";
    let tool_calls = vec![
        ("call_1".to_string(), "register_moltbook".to_string(), json!({
            "name": "TestAgent"
        }))
    ];
    
    let provider = MockProvider::with_tool_calls(mock_response, tool_calls);

    // 3. Build the Agent WITH the tool registered
    let agent = Agent::builder(provider)
        .model("test-model")
        .preamble("You are a Moltbook agent.")
        .tool(RegisterMoltbook) // <--- CRITICAL FIX: Add the tool
        .build()
        .unwrap();

    // 4. Execute the logic
    let response = agent.prompt("Please register me on Moltbook", None).await.unwrap();

    // 5. Assertions
    assert!(response.contains("processed the tool result") || response.contains("Registration Successful"));
    
    println!("Test passed: Agent correctly called the tool using mocks.");
}
