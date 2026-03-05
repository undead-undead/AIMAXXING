use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use brain::error::Error;
use brain::skills::tool::{Tool, ToolDefinition};
use brain::agent::memory::Memory;

/// Tool for searching historical conversations and knowledge
pub struct SearchHistoryTool {
    memory: Arc<dyn Memory>,
}

impl SearchHistoryTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for SearchHistoryTool {
    fn name(&self) -> String {
        "search_history".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Search through past conversations, trading strategies, and knowledge using natural language or keywords. \
                Use this when you need context about a topic discussed previously or to find specific historical data.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query (natural language or keywords)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max number of results to return (default: 5)"
                    }
                },
                "required": ["query"]
            }),
            parameters_ts: Some("interface SearchArgs {\n  query: string; // The search query\n  limit?: number; // Max results (default: 5)\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            query: String,
            #[serde(default = "default_limit")]
            limit: usize,
        }
        fn default_limit() -> usize { 5 }

        let args: Args = serde_json::from_str(arguments)
            .map_err(|e| Error::ToolArguments {
                tool_name: self.name(),
                message: e.to_string(),
            })?;

        // Context is currently not passed to tools, using placeholders.
        // In a multi-user environment, the Tool trait should be updated to accept context.
        let user_id = "default"; 
        let agent_id = None;

        let results = self.memory.search(user_id, agent_id, &args.query, args.limit).await
            .map_err(|e| Error::Internal(format!("Search failed: {}", e)))?;

        if results.is_empty() {
            return Ok("No relevant history found.".to_string());
        }

        let mut table = brain::infra::format::MarkdownTable::new(vec!["#", "Score", "Title", "Preview"]);
        for (i, res) in results.iter().enumerate() {
            let preview = if res.content.len() > 100 {
                format!("{}...", &res.content[..100].replace('\n', " "))
            } else {
                res.content.replace('\n', " ")
            };
            table.add_row(vec![
                (i + 1).to_string(),
                format!("{:.2}", res.score),
                res.title.clone(),
                preview,
            ]);
        }

        Ok(format!("Found {} relevant matches:\n\n{}", results.len(), table.render()))
    }
}

/// Tool for saving important insights to long-term memory
pub struct RememberThisTool {
    memory: Arc<dyn Memory>,
}

impl RememberThisTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for RememberThisTool {
    fn name(&self) -> String {
        "remember_this".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Save a key insight, fact, or trading rule to your long-term memory. \
                Use this to ensure critical information is preserved and available for future retrieval.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Short mnemonic title for this memory"
                    },
                    "content": {
                        "type": "string",
                        "description": "The detail information to be remembered"
                    },
                    "collection": {
                        "type": "string",
                        "description": "Category (e.g., 'rules', 'preferences', 'insights')"
                    }
                },
                "required": ["title", "content"]
            }),
            parameters_ts: Some("interface RememberArgs {\n  title: string; // Short title\n  content: string; // Detail information\n  collection?: string; // Category (default: 'general')\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this to preserve high-value insights, user preferences, or critical rules discovered during a conversation. DO NOT use this for temporary chat context.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            title: String,
            content: String,
            #[serde(default = "default_coll")]
            collection: String,
        }
        fn default_coll() -> String { "general".to_string() }

        let args: Args = serde_json::from_str(arguments)
            .map_err(|e| Error::ToolArguments {
                tool_name: self.name(),
                message: e.to_string(),
            })?;

        // Context is currently not passed to tools, using placeholders.
        let user_id = "default";
        let agent_id = None;

        self.memory.store_knowledge(user_id, agent_id, &args.title, &args.content, &args.collection, true).await?;

        Ok(format!("Memory successfully saved as '{}' in collection '{}'.", args.title, args.collection))
    }
}

/// Tool for tiered search - favor summaries to save tokens
pub struct TieredSearchTool {
    memory: Arc<dyn Memory>,
}

impl TieredSearchTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for TieredSearchTool {
    fn name(&self) -> String {
        "tiered_search".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Search memory and return summaries. Efficient for large datasets. \
                Use this first, then use fetch_document for full content if needed.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Max results (default: 5)" }
                },
                "required": ["query"]
            }),
            parameters_ts: Some("interface TieredSearchArgs {\n  query: string;\n  limit?: number;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { query: String, #[serde(default = "default_limit")] limit: usize }
        fn default_limit() -> usize { 5 }

        let args: Args = serde_json::from_str(arguments)?;
        let results = self.memory.search("default", None, &args.query, args.limit).await?;

        if results.is_empty() { return Ok("No results found.".to_string()); }

        let mut table = brain::infra::format::MarkdownTable::new(vec!["#", "Title", "Collection", "Path", "Summary/Snippet"]);
        for (i, res) in results.iter().enumerate() {
            let info = res.summary.as_ref().cloned().unwrap_or_else(|| {
                if res.content.len() > 150 { format!("{}...", &res.content[..150]) } else { res.content.clone() }
            }).replace('\n', " ");

            table.add_row(vec![
                (i + 1).to_string(),
                res.title.clone(),
                res.collection.as_deref().unwrap_or("-").to_string(),
                res.path.as_deref().unwrap_or("-").to_string(),
                info,
            ]);
        }

        Ok(format!("Search results (summarized):\n\n{}\n\nUse `fetch_document` with collection and path for full content.", table.render()))
    }
}

/// Tool for fetching full document content
pub struct FetchDocumentTool {
    memory: Arc<dyn Memory>,
}

impl FetchDocumentTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for FetchDocumentTool {
    fn name(&self) -> String {
        "fetch_document".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Retrieve the full content of a document by its collection and path.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "collection": { "type": "string", "description": "Document collection" },
                    "path": { "type": "string", "description": "Document virtual path" }
                },
                "required": ["collection", "path"]
            }),
            parameters_ts: Some("interface FetchArgs {\n  collection: string;\n  path: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { collection: String, path: String }
        let args: Args = serde_json::from_str(arguments)?;

        let doc = self.memory.fetch_document(&args.collection, &args.path).await?;
        match doc {
            Some(d) => {
                // Phase 14: Reward fetching (explicit utility)
                let _ = self.memory.update_utility(&args.collection, &args.path, 0.1).await;
                Ok(format!("# {}\n\n{}", d.title, d.content))
            },
            None => Ok("Document not found.".to_string()),
        }
    }
}
