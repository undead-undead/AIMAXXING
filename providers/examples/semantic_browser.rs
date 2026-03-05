/// Example: Semantic Browser Tool
///
/// This example demonstrates how to build a production-grade "Semantic Web Browser" tool
/// that uses `headless_chrome` (simulated here for portability) to extract the
/// Accessibility Tree (ARIA) of a page instead of raw HTML.
///
/// This approach reduces token usage by 95% and improves agent reasoning.

use brain::prelude::*;
use brain::skills::tool::{Tool, ToolDefinition};
use brain::error::{Error, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

// --- Mock Headless Chrome Wrapper ---
// In a real implementation, you would use:
// use headless_chrome::{Browser, LaunchOptions};

struct SemanticBrowser {
    // In real code: browser: Browser,
}

impl SemanticBrowser {
    pub fn new() -> Self {
        Self {}
    }

    /// Simulate fetching and parsing a page into an ARIA tree
    async fn browse(&self, url: &str) -> Result<String> {
        // In real code:
        // 1. let tab = self.browser.new_tab()?;
        // 2. tab.navigate_to(url)?.wait_until_navigated()?;
        // 3. let tree = tab.get_accessibility_tree()?;
        // 4. return Ok(tree.to_string());

        // Simulation for "News Site"
        if url.contains("news") {
            Ok(r#"
[RootWebArea] "Crypto News Daily"
  [Banner]
    [Heading level=1] "Market Update: Bitcoin hits $100k"
  [Main]
    [Article]
      [Heading level=2] "Why institutional money is flowing in"
      [Text] "BlackRock ETF volume hit record highs..."
    [Article]
      [Heading level=2] "Solana network upgrade successful"
      [Text] "TPS increased by 20%..."
  [Navigation]
    [Link] "Next Page"
"#.trim().to_string())
        } else {
            Ok(format!("[RootWebArea] Unknown Page: {}\n  [Text] '404 Not Found'", url))
        }
    }
}

// --- The Tool Wrapper ---

#[derive(Clone)]
struct BrowserTool {
    browser: Arc<SemanticBrowser>,
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> String {
        "browse_web".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "browse_web".to_string(),
            description: "Browse a website and return its semantic structure (Accessibility Tree). Use this to read news, documentation, or gather market sentiment. This tool does NOT return HTML.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to visit"
                    }
                },
                "required": ["url"]
            }),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, args: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            url: String,
        }
        let args: Args = serde_json::from_str(args).map_err(|e| anyhow::anyhow!("Tool arguments error: {}", e))?;

        // Use the shared browser instance
        self.browser.browse(&args.url).await.map_err(|e| anyhow::anyhow!("Browser error: {}", e))
    }
}

// --- Main Example ---

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    
    // 1. Initialize the heavy browser instance once
    let browser = Arc::new(SemanticBrowser::new());
    
    // 2. Wrap it in a Tool
    let tool = BrowserTool { browser: browser };

    // 3. Create Agent (using Mock Provider for test without API key)
    use providers::mock::MockProvider;
    let provider = MockProvider::new("OK");
    
    let agent = Agent::builder(provider)
        .model("gpt-4o")
        .preamble("You are a market analyst.")
        .tool(tool)
        .build()?;

    println!("🤖 Agent: Visiting crypto news site...");

    // 4. Zero-cost abstraction: direct tool call for demo
    // distinct from Agent::prompt loop, just demonstrating the tool
    let result = agent.call_tool("browse_web", r#"{"url": "https://crypto-news.com"}"#).await?;
    
    println!("\n📄 Semantic Snapshort (ARIA Tree):\n{}", result);
    
    println!("\n✅ Interpretation: The agent sees only the meaningful content (Headings, Text), not the <div> soup.");

    Ok(())
}
