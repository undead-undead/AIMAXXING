use brain::agent::multi_agent::MultiAgent;
use brain::error::Result;
use crate::intent::IntentAnalysis;
use tracing::{info, instrument};

/// Routes user queries to specific virtual paths based on intent
pub struct IntentRouter {
    // We might need a reference to an Agent or LLM provider here
    // For now, we'll design the interface to take an Agent as an argument or similar
}

impl IntentRouter {
    pub fn new() -> Self {
        Self {}
    }

    /// Analyze the intent of a query and return a classified Result
    #[instrument(skip(self, agent), fields(query = %query))]
    pub async fn analyze(&self, agent: &dyn MultiAgent, query: &str) -> Result<IntentAnalysis> {
        // 1. Construct the prompt
        let prompt = format!(
            r#"You are the Intent Classifier for the AIMAXXING Knowledge Base.
Your job is to route the user's query to the correct virtual directory.

Query: "{}"

Available Intents:
- "skill": User wants to use a tool or asks about capabilities. Target: aimaxxing://skills/
- "memory": User asks about past interactions, facts, or long-term memory. Target: aimaxxing://memory/
- "code": User asks about source code, implementation, or technical details. Target: aimaxxing://codebase/
- "system": User asks about configuration, status, or system health. Target: aimaxxing://system/
- "chat": Casual conversation, no retrieval needed. Target: []

Output JSON format:
{{
  "primary_intent": "skill" | "memory" | "code" | "system" | "chat",
  "target_paths": ["aimaxxing://skills/"], 
  "keywords": "extracted keywords",
  "confidence": 0.95
}}

Return ONLY JSON."#,
            query
        );

        // 2. Call the LLM (using the Agent's provider)
        
        let response = agent.process(&prompt).await?;

        // 3. Parse JSON
        // Robust cleaning of markdown code blocks if present
        let cleaned = response.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        match serde_json::from_str::<IntentAnalysis>(cleaned) {
            Ok(analysis) => Ok(analysis),
            Err(e) => {
                info!("Failed to parse intent JSON: {}. Response: {}", e, response);
                // Fallback to global search
                Ok(IntentAnalysis::global(query))
            }
        }
    }
}
