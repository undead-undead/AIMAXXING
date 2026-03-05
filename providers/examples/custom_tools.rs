/// Example demonstrating custom tool creation and registration
/// 
/// This example shows:
/// - Creating custom tools by implementing the Tool trait
/// - Registering tools with an agent
/// - Agent automatically calling tools based on user input

use brain::prelude::*;
use brain::skills::tool::{Tool, ToolDefinition};
use providers::gemini::{Gemini, GEMINI_2_0_FLASH};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

// Define a simple weather tool
struct GetWeather;

#[async_trait]
impl Tool for GetWeather {
    fn name(&self) -> String {
        "get_weather".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_weather".to_string(),
            description: "Get current weather information for a city".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "city": {
                        "type": "string",
                        "description": "The name of the city"
                    }
                },
                "required": ["city"]
            }),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            city: String,
        }
        let args: Args = serde_json::from_str(arguments).map_err(|e| anyhow::anyhow!("Invalid tool arguments: {}", e))?;

        // In a real implementation, this would call a weather API
        Ok(format!("Weather in {}: Sunny, 25°C, Light breeze", args.city))
    }
}

// Define a calculator tool
struct Calculate;

#[async_trait]
impl Tool for Calculate {
    fn name(&self) -> String {
        "calculate".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "calculate".to_string(),
            description: "Perform basic arithmetic calculations".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "Mathematical expression (e.g., '2 + 2')"
                    }
                },
                "required": ["expression"]
            }),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            expression: String,
        }
        let args: Args = serde_json::from_str(arguments).map_err(|e| anyhow::anyhow!("Invalid tool arguments: {}", e))?;

        // Simplified calculation (in production, use a proper parser)
        let result = match args.expression.as_str() {
            "2 + 2" => "4",
            "10 * 5" => "50",
            _ => "Unable to calculate (mock only supports '2 + 2' or '10 * 5')",
        };
        Ok(format!("Result: {}", result))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let provider = Gemini::from_env()?;

    // Build agent with custom tools
    let agent = Agent::builder(provider)
        .model(GEMINI_2_0_FLASH)
        .preamble("You are a helpful assistant with access to weather and calculator tools.")
        .tool(GetWeather)
        .tool(Calculate)
        .build()?;

    // Test tool usage
    println!("=== Testing Weather Tool ===");
    let response = agent.prompt("What's the weather like in Tokyo?", None).await?;
    println!("Agent: {}\n", response);

    println!("=== Testing Calculator Tool ===");
    let response = agent.prompt("What is 2 + 2?", None).await?;
    println!("Agent: {}\n", response);

    Ok(())
}
