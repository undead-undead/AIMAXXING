use std::sync::Arc;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use aimaxxing_core::skills::tool::{Tool, ToolDefinition};
use crate::HierarchicalRetriever;

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
            output.push_str(&format!("{}. **{}** (Collection: {}, Score: {:.2})\n", 
                i + 1, res.document.path, res.document.collection, res.rrf_score));
            
            // Tiered Context Injection
            if let Some(abs) = &res.document.abstract_content {
                output.push_str(&format!("   *Abstract*: {}\n", abs.replace('\n', " ")));
            }
            if let Some(ov) = &res.document.overview_content {
                output.push_str(&format!("   *Overview*: {}\n", ov.replace('\n', " ")));
            }

            if let Some(content) = &res.document.body {
                // If we have an overview, maybe we only need a shorter snippet of the body
                let snippet_len = if res.document.overview_content.is_some() { 500 } else { 1200 };
                let snippet = if content.len() > snippet_len {
                    format!("{}...", &content[..snippet_len])
                } else {
                    content.clone()
                };
                output.push_str(&format!("   *Content Snippet*: {}\n", snippet.replace('\n', " ")));
            }
            output.push_str("\n");
        }

        output.push_str("Tip: Use `fetch_document` if you need the full text of a specific file.");
        Ok(output)
    }
}
