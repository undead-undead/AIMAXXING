/// Mock Agent Demo
/// 
/// This example demonstrates how to use the MockProvider to simulate an LLM 
/// without requiring an API key. This is useful for testing tool integrations
/// and agent logic locally.

use brain::prelude::*;
use brain::skills::tool::{Tool, ToolDefinition};
use providers::mock::MockProvider;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

// 1. Define a Mock Tool
struct MockWeather;

#[async_trait]
impl Tool for MockWeather {
    fn name(&self) -> String { "get_weather".to_string() }
    
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get weather info".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"]
            }),
            is_binary: false,
            is_verified: true,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { city: String }
        let args: Args = serde_json::from_str(arguments)?;
        Ok(format!("The weather in {} is currently sunny and 75°F.", args.city))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("🧪 AIMAXXING Mock Provider Testing");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 2. Initialize MockProvider with a response that simulates a tool call
    let provider = MockProvider::with_tool_calls(
        "I will check the weather for you.",
        vec![("call_123".to_string(), "get_weather".to_string(), json!({"city": "San Francisco"}))]
    );

    // 3. Build the Agent
    let agent = Agent::builder(provider)
        .model("mock-model")
        .preamble("You are a helpful assistant.")
        .tool(MockWeather)
        .build()?;

    println!("🤖 Agent (Mocked): How can I help you today?");
    println!("👤 User: What's the weather in San Francisco?");
    
    // In this demo, since MockProvider is simple, it will just return "I will check the weather for you."
    // It won't actually trigger the tool unless the MockProvider returns a ToolCall StreamingChoice.
    
    let response = agent.prompt("What's the weather in San Francisco?", None).await?;
    println!("🤖 Agent: {}", response);

    println!("\n✅ Mock test completed successfully.");
    Ok(())
}
