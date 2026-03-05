use std::sync::Arc;
use tokio::sync::Mutex;
use engram::hybrid_search::{Reranker, RerankCandidate, RerankScore};
use crate::provider::Provider;
use crate::message::{Message, Role, Content};
use crate::error::Result;

/// Reranker implementation that uses an AI Provider
pub struct EngramReranker {
    provider: Arc<dyn Provider>,
    model: String,
}

impl EngramReranker {
    pub fn new(provider: Arc<dyn Provider>, model: String) -> Self {
        Self { provider, model }
    }
}

impl Reranker for EngramReranker {
    fn rerank(&self, query: &str, candidates: Vec<RerankCandidate>) -> engram::error::Result<Vec<RerankScore>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        // We need to run the async completion in a blocking fashion or use a runtime handle
        // inside the trait's sync method. Note: engram's Reranker trait is sync (but called from async context in brain).
        // Since brain will use this, we can use tokio::task::block_in_place if needed, 
        // but it's better if Engram's trait was async.
        // For now, we'll use a local runtime handle if available or block_on.
        
        let provider = self.provider.clone();
        let model = self.model.clone();
        let query = query.to_string();
        
        let mut candidates_text = String::new();
        for (i, cand) in candidates.iter().enumerate() {
            candidates_text.push_str(&format!("[{}] (File: {})\nContent: {}\n\n", i, cand.file, cand.text));
        }

        let system_prompt = "You are a highly precise search result reranker. \
            Your task is to evaluate the relevance of documents to a user query. \
            Score each document from 0.0 to 1.0. \
            Output ONLY a JSON array of objects with 'file' and 'score' fields.";

        let user_prompt = format!("Query: {}\n\nCandidates:\n{}", query, candidates_text);
        
        // Execute async via handle
        let handle = tokio::runtime::Handle::current();
        let result = handle.block_on(async move {
            let messages = vec![Message::user(user_prompt)];
            let stream = provider.stream_completion(&model, Some(system_prompt), messages, vec![], None, None, None).await?;
            let response = stream.collect_text().await?;
            
            // Basic extraction of JSON array from response
            let start = response.find('[').unwrap_or(0);
            let end = response.rfind(']').unwrap_or(response.len() - 1);
            let json_str = &response[start..=end];
            
            let scores: Vec<RerankScore> = serde_json::from_str(json_str)
                .map_err(|e| crate::error::Error::Internal(format!("Failed to parse rerank scores: {}", e)))?;
            
            Ok::<Vec<RerankScore>, crate::error::Error>(scores)
        });

        match result {
            Ok(scores) => Ok(scores),
            Err(e) => Err(engram::error::EngramError::Custom(e.to_string())),
        }
    }
}
