use std::sync::Arc;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use brain::skills::tool::{Tool, ToolDefinition};
use engram::HierarchicalRetriever;

/// Tool that allows the Agent to perform deep recursive knowledge searches
pub struct KnowledgeSearchTool {
    retriever: Arc<HierarchicalRetriever>,
}

impl KnowledgeSearchTool {
    pub fn new(retriever: Arc<HierarchicalRetriever>) -> Self {
        Self { retriever }
    }
}

#[async_trait]
impl Tool for KnowledgeSearchTool {
    fn name(&self) -> String {
        "knowledge_search".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Deep recursive search in the local knowledge base. Use this for complex queries that require analyzing abstracts and full content to find precise answers.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The natural language query to search for"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
            parameters_ts: Some("interface KnowledgeSearchArgs {\n  query: string;\n  limit?: number;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this when the user asks questions about project documentation, architecture, settings, or historical data stored in the knowledge base.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            query: String,
            limit: Option<usize>,
        }
        let args: Args = serde_json::from_str(arguments)?;
        let limit = args.limit.unwrap_or(5);

        // Perform recursive search
        let results = self.retriever.search_recursive(&args.query, limit).await?;

        if results.is_empty() {
            return Ok("No relevant information found in the knowledge base.".to_string());
        }

        // Format results for LLM consumption
        let mut output = String::from("### Knowledge Search Results\n\n");
        for (i, res) in results.iter().enumerate() {
            output.push_str(&format!("{}. [{}] (Score: {:.2})\n", i + 1, res.document.path, res.rrf_score));
            if let Some(content) = &res.document.body {
                // Truncate content to avoid overwhelming the context
                let snippet = if content.len() > 1000 {
                    format!("{}...", &content[..1000])
                } else {
                    content.clone()
                };
                output.push_str(&format!("   Content Snippet: {}\n", snippet.replace('\n', " ")));
            }
            output.push_str("\n");
        }

        Ok(output)
    }
}
