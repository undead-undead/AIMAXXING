//! Intent Analysis for Recursive Retrieval
//!
//! Analyzes user queries to determine the best retrieval strategy and target paths.

use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Type of context to search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextType {
    /// Abstract (L0) - High level overview
    Abstract,
    /// Overview (L1) - Detailed summary
    Overview,
    /// Full Content (L2) - Actual file content
    Full,
}

/// A structured query derived from intent analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedQuery {
    pub query: String,
    pub context_type: ContextType,
    pub priority: u8,
}

/// The plan execution strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlan {
    pub original_query: String,
    pub steps: Vec<TypedQuery>,
    pub target_paths: Vec<String>,
}

pub struct IntentAnalyzer;

impl IntentAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a user query and generate a retrieval plan
    pub async fn analyze(&self, query: &str) -> Result<QueryPlan> {
        let mut steps = Vec::new();

        // Always start with a broad abstract search
        steps.push(TypedQuery {
            query: query.to_string(),
            context_type: ContextType::Abstract,
            priority: 10,
        });

        // If query seems specific, add Overview search
        if query.len() > 10 {
            steps.push(TypedQuery {
                query: query.to_string(),
                context_type: ContextType::Overview,
                priority: 8,
            });
        }

        Ok(QueryPlan {
            original_query: query.to_string(),
            steps,
            target_paths: Vec::new(),
        })
    }
}
