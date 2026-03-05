/// Example: Markdown Memory (WriteNote Tool)
///
/// Demonstrates how to give an Agent "Explicit Memory" by allowing it to write Markdown files.
/// This acts as a persistent log or journal that is human-readable.

use brain::prelude::*;
use brain::skills::tool::{Tool, ToolDefinition};
use brain::error::{Error, Result};
use providers::gemini::{Gemini, GEMINI_2_0_FLASH};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;
use tokio::io::AsyncWriteExt;

// Define the WriteNote tool
struct WriteNote;

#[async_trait]
impl Tool for WriteNote {
    fn name(&self) -> String {
        "write_note".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_note".to_string(),
            description: "Write a structured note or log entry to a markdown file. Use this to record important decisions, daily summaries, or strategy notes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "filename": {
                        "type": "string",
                        "description": "Filename (e.g., 'daily_log.md', 'strategy_notes.md')"
                    },
                    "content": {
                        "type": "string",
                        "description": "The markdown content to write"
                    }
                },
                "required": ["filename", "content"]
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
            filename: String,
            content: String,
        }
        let args: Args = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
            tool_name: "write_note".to_string(),
            message: e.to_string(),
        })?;

        let filename = args.filename;
        let content = args.content;
        
        // Security check: simple path traversal prevention
        if filename.contains("..") || filename.starts_with('/') {
            return Err(Error::tool_execution("write_note".to_string(), "Invalid filename. Use relative paths only.".to_string()).into());
        }

        let path = Path::new("memory").join(&filename);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| Error::tool_execution("write_note".to_string(), e.to_string()))?;
        }

        // Append if exists, or create new
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await.map_err(|e| Error::tool_execution("write_note".to_string(), e.to_string()))?;
            
        let entry = format!("\n\n## [{}] Log Entry\n{}\n", chrono::Local::now().to_rfc3339(), content);
        file.write_all(entry.as_bytes()).await.map_err(|e| Error::tool_execution("write_note".to_string(), e.to_string()))?;

        Ok(format!("Note appended to {}", filename))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let provider = Gemini::from_env()?;

    let agent = Agent::builder(provider)
        .model(GEMINI_2_0_FLASH)
        .preamble(
            "You are a trading bot with explicit memory. \
            Whenever you make a decision, you MUST write it to 'trading_log.md'. \
            Keep logs concise to save tokens."
        )
        .tool(WriteNote)
        .build()?;

    println!("🤖 Agent started. Asking for advice...");

    // This single prompt will trigger the tool call
    let response = agent.prompt(
        "Analyze the current market sentiment for Crypto (simulated data: Bitcoin $98k, Greed Index 80). \
        Should we buy or sell? Log your decision.", None
    ).await?;

    println!("\n🤖 Agent Response:\n{}", response);
    
    // Using std::fs for quick verification
    println!("\n📂 Checking Memory File (memory/trading_log.md):");
    if let Ok(content) = fs::read_to_string("memory/trading_log.md") {
        println!("{}", content);
    } else {
        println!("(No log file found - did the agent fail to write?)");
    }

    Ok(())
}
