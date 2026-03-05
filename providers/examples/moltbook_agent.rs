/// Moltbook Social Agent Example
/// 
/// This example demonstrates how to build a social media agent for Moltbook
/// using AIMAXXING. The agent can:
/// - Register and authenticate with Moltbook
/// - Post content to the platform
/// - Comment on posts
/// - Upvote interesting content
/// 
/// Prerequisites:
/// - Set MOLTBOOK_API_KEY environment variable (get it from registration)
/// - Or the agent will auto-register on first run

use brain::{prelude::*, skills::tool::{Tool, ToolDefinition}, error::{Error, Result}};
use providers::openai::{OpenAI, GPT_4O};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

const MOLTBOOK_BASE_URL: &str = "https://www.moltbook.com/api/v1";

// Tool: Register with Moltbook
struct RegisterMoltbook;

#[async_trait]
impl Tool for RegisterMoltbook {
    fn name(&self) -> String { "register_moltbook".to_string() }
    
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "register_moltbook".to_string(),
            description: "Register a new agent on Moltbook social network. Only call this once!".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Agent name" },
                    "description": { "type": "string", "description": "What this agent does" }
                },
                "required": ["name", "description"]
            }),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { name: String, description: String }
        let args: Args = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments { tool_name: self.name(), message: e.to_string() })?;

        let client = reqwest::Client::new();
        let response = client.post(format!("{}/agents/register", MOLTBOOK_BASE_URL))
            .json(&json!({ "name": args.name, "description": args.description }))
            .send().await.map_err(|e| Error::tool_execution(self.name(), e.to_string()))?;
        
        let data: Value = response.json().await.map_err(|e| Error::tool_execution(self.name(), e.to_string()))?;
        Ok(format!(
            "✅ Registered! API Key: {}\n🔗 Claim URL: {}\n🔑 Verification Code: {}\n\n⚠️ SAVE YOUR API KEY!",
            data["agent"]["api_key"].as_str().unwrap_or("N/A"),
            data["agent"]["claim_url"].as_str().unwrap_or("N/A"),
            data["agent"]["verification_code"].as_str().unwrap_or("N/A")
        ))
    }
}

// Tool: Create a post
struct CreatePost;

#[async_trait]
impl Tool for CreatePost {
    fn name(&self) -> String { "create_post".to_string() }
    
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_post".to_string(),
            description: "Create a new post on Moltbook".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Post title" },
                    "content": { "type": "string", "description": "Post content" },
                    "submolt": { "type": "string", "description": "Community name (optional, default: 'general')" }
                },
                "required": ["title", "content"]
            }),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { title: String, content: String, submolt: Option<String> }
        let args: Args = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments { tool_name: self.name(), message: e.to_string() })?;

        let api_key = std::env::var("MOLTBOOK_API_KEY").map_err(|_| Error::tool_execution(self.name(), "MOLTBOOK_API_KEY not set".to_string()))?;
        
        let client = reqwest::Client::new();
        let response = client.post(format!("{}/posts", MOLTBOOK_BASE_URL))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&json!({ "title": args.title, "content": args.content, "submolt": args.submolt.unwrap_or_else(|| "general".to_string()) }))
            .send().await.map_err(|e| Error::tool_execution(self.name(), e.to_string()))?;
        
        let data: Value = response.json().await.map_err(|e| Error::tool_execution(self.name(), e.to_string()))?;
        Ok(format!(
            "📝 Post created! ID: {}\n🔗 https://www.moltbook.com/post/{}",
            data["post"]["id"].as_str().unwrap_or("N/A"),
            data["post"]["id"].as_str().unwrap_or("N/A")
        ))
    }
}

// Tool: Get feed
struct GetFeed;

#[async_trait]
impl Tool for GetFeed {
    fn name(&self) -> String { "get_feed".to_string() }
    
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_feed".to_string(),
            description: "Get recent posts from Moltbook feed".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "Number of posts to fetch (default: 10)" }
                },
                "required": []
            }),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { limit: Option<u64> }
        let args: Args = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments { tool_name: self.name(), message: e.to_string() })?;

        let api_key = std::env::var("MOLTBOOK_API_KEY").map_err(|_| Error::tool_execution(self.name(), "MOLTBOOK_API_KEY not set".to_string()))?;
        let limit = args.limit.unwrap_or(10);
        
        let client = reqwest::Client::new();
        let response = client.get(format!("{}/posts?limit={}", MOLTBOOK_BASE_URL, limit))
            .header("Authorization", format!("Bearer {}", api_key))
            .send().await.map_err(|e| Error::tool_execution(self.name(), e.to_string()))?;
        
        let data: Value = response.json().await.map_err(|e| Error::tool_execution(self.name(), e.to_string()))?;
        let default_posts = vec![];
        let posts = data["posts"].as_array().unwrap_or(&default_posts);
        
        let mut result = String::from("📰 Recent Posts:\n\n");
        for post in posts.iter().take(5) {
            result.push_str(&format!(
                "• {} (by @{})\n  ⬆️ {} | 💬 {}\n\n",
                post["title"].as_str().unwrap_or("Untitled"),
                post["author"]["username"].as_str().unwrap_or("unknown"),
                post["upvotes"].as_u64().unwrap_or(0),
                post["comment_count"].as_u64().unwrap_or(0)
            ));
        }
        Ok(result)
    }
}

// Tool: Upvote a post
struct UpvotePost;

#[async_trait]
impl Tool for UpvotePost {
    fn name(&self) -> String { "upvote_post".to_string() }
    
    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "upvote_post".to_string(),
            description: "Upvote a post on Moltbook".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "post_id": { "type": "string", "description": "ID of the post to upvote" }
                },
                "required": ["post_id"]
            }),
            is_binary: false,
            is_verified: false,
            parameters_ts: None,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { post_id: String }
        let args: Args = serde_json::from_str(arguments)?;

        let api_key = std::env::var("MOLTBOOK_API_KEY")?;
        
        let client = reqwest::Client::new();
        let response = client.post(format!("{}/posts/{}/upvote", MOLTBOOK_BASE_URL, args.post_id))
            .header("Authorization", format!("Bearer {}", api_key))
            .send().await.map_err(|e| Error::tool_execution(self.name(), e.to_string()))?;
        
        if response.status().is_success() {
            Ok(format!("⬆️ Upvoted post {}", args.post_id))
        } else {
            Ok(format!("❌ Failed to upvote: {}", response.status()))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("🦞 Moltbook Social Agent");
    println!("========================\n");

    // Check if already registered
    if std::env::var("MOLTBOOK_API_KEY").is_err() {
        println!("⚠️  No MOLTBOOK_API_KEY found.");
        println!("💡 The agent can register itself, or you can set the key manually.\n");
    }

    let provider = OpenAI::from_env()?;


    // Build agent with Moltbook tools
    let agent = Agent::builder(provider)
        .model(GPT_4O)
        .preamble(
            "You are a social media agent on Moltbook. \
            You can register, post content, read the feed, and upvote interesting posts. \
            Be friendly, engaging, and helpful to the community."
        )
        .tool(RegisterMoltbook)
        .tool(CreatePost)
        .tool(GetFeed)
        .tool(UpvotePost)
        .build()?;

    // Example interactions
    println!("=== Example 1: Check Feed ===");
    let response = agent.prompt("Show me the latest posts on Moltbook", None).await?;
    println!("{}\n", response);

    println!("=== Example 2: Create a Post ===");
    let response = agent
        .prompt("Create a post titled 'Hello from AIMAXXING!' about how AIMAXXING makes building AI agents easy", None)
        .await?;
    println!("{}\n", response);

    Ok(())
}
