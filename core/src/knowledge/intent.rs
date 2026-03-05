use crate::knowledge::virtual_path::VirtualPath;
use serde::{Deserialize, Serialize};

/// High-level intent of the user's query for knowledge retrieval
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RetrievalIntent {
    /// The user is looking for a specific skill or tool
    #[serde(rename = "skill")]
    Skill,
    /// The user is looking for general knowledge, memories, or documents
    #[serde(rename = "memory")]
    Memory,
    /// The user is looking for source code or technical implementation details
    #[serde(rename = "code")]
    Code,
    /// The user is asking about the system itself or configuration
    #[serde(rename = "system")]
    System,
    /// The query doesn't match any specific knowledge base intent (e.g., casual chat)
    #[serde(rename = "chat")]
    Chat,
}

/// The result of analyzing a query's intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAnalysis {
    /// The primary intent classification
    pub primary_intent: RetrievalIntent,
    /// The virtual paths that are most relevant to this intent
    pub target_paths: Vec<String>,
    /// Refined search keywords extracted from the query (removing noise)
    pub keywords: String,
    /// The confidence score of this analysis (0.0 - 1.0)
    pub confidence: f32,
}

impl IntentAnalysis {
    /// Create a default "global search" intent when analysis fails
    pub fn global(query: &str) -> Self {
        Self {
            primary_intent: RetrievalIntent::Memory,
            target_paths: vec!["aimaxxing://".to_string()],
            keywords: query.to_string(),
            confidence: 0.1,
        }
    }
}
