//! Phase 14: Auto-Reflection — post-interaction summary extraction.
//!
//! After each interaction cycle, automatically generates a compressed
//! summary of key information, user preferences, and action items,
//! then asynchronously stores them in the memory system.

use std::sync::Arc;
use crate::agent::memory::Memory;
use crate::agent::message::{Message, Role};
use crate::agent::provider::{Provider, ChatRequest};

/// Report from an auto-reflection cycle
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReflectionReport {
    pub key_facts: Vec<String>,
    pub user_preferences: Vec<String>,
    pub action_items: Vec<String>,
    pub summary: String,
}

impl ReflectionReport {
    pub fn empty() -> Self {
        Self {
            key_facts: Vec::new(),
            user_preferences: Vec::new(),
            action_items: Vec::new(),
            summary: String::new(),
        }
    }
}

/// Configuration for auto-reflection
#[derive(Debug, Clone)]
pub struct ReflectionConfig {
    /// Minimum messages in a conversation before triggering reflection
    pub min_messages: usize,
    /// Maximum summary length in characters
    pub max_summary_len: usize,
}

impl Default for ReflectionConfig {
    fn default() -> Self {
        Self {
            min_messages: 4,
            max_summary_len: 500,
        }
    }
}

/// Auto-reflection engine that extracts key information from conversations.
pub struct AutoReflection {
    memory: Arc<dyn Memory>,
    provider: Option<Arc<dyn Provider>>,
    model: Option<String>,
    config: ReflectionConfig,
    system_prompt: String,
}

impl AutoReflection {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self {
            memory,
            provider: None,
            model: None,
            config: ReflectionConfig::default(),
            system_prompt: concat!(
                "You are an Interaction Analyst for AIMAXXING. \n",
                "Analyze the provided conversation history and extract critical insights. \n",
                "OUTPUT FORMAT: Valid JSON only: \n",
                "{ \n",
                "  \"key_facts\": [\"fact 1\", \"fact 2\"], \n",
                "  \"user_preferences\": [\"pref 1\", \"pref 2\"], \n",
                "  \"action_items\": [\"todo 1\", \"todo 2\"], \n",
                "  \"summary\": \"One-sentence high-level summary\" \n",
                "}"
            ).to_string(),
        }
    }

    pub fn with_provider(mut self, provider: Arc<dyn Provider>, model: String) -> Self {
        self.provider = Some(provider);
        self.model = Some(model);
        self
    }

    pub fn with_config(mut self, config: ReflectionConfig) -> Self {
        self.config = config;
        self
    }

    /// Run reflection on a completed conversation
    pub async fn reflect(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        messages: &[Message],
    ) -> anyhow::Result<ReflectionReport> {
        if messages.len() < self.config.min_messages {
            return Ok(ReflectionReport::empty());
        }

        if let (Some(provider), Some(model)) = (&self.provider, &self.model) {
            self.llm_reflect(user_id, agent_id, messages, provider, model).await
        } else {
            // Fallback to heuristic if no provider
            self.heuristic_reflect(user_id, agent_id, messages).await
        }
    }

    async fn llm_reflect(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        messages: &[Message],
        provider: &Arc<dyn Provider>,
        model: &str,
    ) -> anyhow::Result<ReflectionReport> {
        let mut history_text = String::new();
        for m in messages {
            history_text.push_str(&format!("{}: {}\n", m.role.as_str(), m.text()));
        }

        let request = ChatRequest {
            model: model.to_string(),
            system_prompt: Some(self.system_prompt.clone()),
            messages: vec![Message::user(format!("Reflect on this conversation:\n\n{}", history_text))],
            temperature: Some(0.3),
            ..Default::default()
        };

        let stream = provider.stream_completion(request).await?;
        let full_text = stream.collect_text().await?;

        // Parse JSON
        let json_start = full_text.find('{');
        let json_end = full_text.rfind('}');
        
        if let (Some(start), Some(end)) = (json_start, json_end) {
            let json_str = &full_text[start..=end];
            let report: ReflectionReport = serde_json::from_str(json_str)?;
            
            // Store reflection in memory if meaningful
            if !report.summary.is_empty() {
                let reflection_msg = Message::system(format!("[AUTO-REFLECTION] {}", report.summary));
                let _ = self.memory.store(user_id, agent_id, reflection_msg).await;
                
                // Phase 14: Reward memories that contributed to a successful reflection
                for msg in messages {
                    if let (Some(coll), Some(path)) = (&msg.source_collection, &msg.source_path) {
                        let _ = self.memory.update_utility(coll, path, 0.2).await;
                    }
                }
            }
            
            Ok(report)
        } else {
            anyhow::bail!("Reflector failed to produce JSON: {}", full_text)
        }
    }

    async fn heuristic_reflect(
        &self,
        user_id: &str,
        agent_id: Option<&str>,
        messages: &[Message],
    ) -> anyhow::Result<ReflectionReport> {

        // Heuristic extraction (would be LLM-based in production)
        let mut key_facts = Vec::new();
        let mut user_preferences = Vec::new();
        let mut action_items = Vec::new();

        for msg in messages {
            let text = msg.text().to_string();
            let lower = text.to_lowercase();

            // Extract user preferences (heuristic patterns)
            if msg.role == Role::User {
                if lower.contains("i prefer") || lower.contains("i like") || lower.contains("i want") {
                    user_preferences.push(truncate(&text, 100));
                }
                // Extract action items
                if lower.contains("todo") || lower.contains("remind me")
                    || lower.contains("don't forget") || lower.contains("make sure")
                {
                    action_items.push(truncate(&text, 100));
                }
            }

            // Extract key facts from assistant responses
            if msg.role == Role::Assistant && text.len() > 50 {
                // Take the first sentence as a key fact
                if let Some(first_sentence) = text.split('.').next() {
                    if first_sentence.len() > 20 {
                        key_facts.push(truncate(first_sentence, 120));
                    }
                }
            }
        }

        // Deduplicate
        key_facts.dedup();
        key_facts.truncate(5);
        user_preferences.dedup();
        action_items.dedup();

        // Build summary
        let summary = build_summary(&key_facts, &user_preferences, &action_items, self.config.max_summary_len);

        // Store reflection in memory if meaningful
        if !summary.is_empty() {
            let reflection_msg = Message::system(format!("[AUTO-REFLECTION] {}", summary));
            let _ = self.memory.store(user_id, agent_id, reflection_msg).await;
            
            // Phase 14: Reward memories that contributed to a successful heuristic reflection
            for msg in messages {
                if let (Some(coll), Some(path)) = (&msg.source_collection, &msg.source_path) {
                    let _ = self.memory.update_utility(coll, path, 0.2).await;
                }
            }
        }

        Ok(ReflectionReport {
            key_facts,
            user_preferences,
            action_items,
            summary,
        })
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

fn build_summary(
    facts: &[String],
    prefs: &[String],
    actions: &[String],
    max_len: usize,
) -> String {
    let mut parts = Vec::new();

    if !facts.is_empty() {
        parts.push(format!("Key: {}", facts.join("; ")));
    }
    if !prefs.is_empty() {
        parts.push(format!("Prefs: {}", prefs.join("; ")));
    }
    if !actions.is_empty() {
        parts.push(format!("TODO: {}", actions.join("; ")));
    }

    let summary = parts.join(" | ");
    if summary.len() > max_len {
        format!("{}...", &summary[..max_len])
    } else {
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::memory::InMemoryMemory;

    #[tokio::test]
    async fn test_reflection_too_short() {
        let memory = Arc::new(InMemoryMemory::new());
        let reflector = AutoReflection::new(memory);
        let messages = vec![
            Message::user("hi"),
            Message::assistant("hello"),
        ];
        let report = reflector.reflect("u1", None, &messages).await.unwrap();
        assert!(report.summary.is_empty());
    }

    #[tokio::test]
    async fn test_reflection_extracts_preferences() {
        let memory = Arc::new(InMemoryMemory::new());
        let reflector = AutoReflection::new(memory);
        let messages = vec![
            Message::user("I prefer dark mode for all my applications"),
            Message::assistant("Sure, I'll note that you prefer dark mode. This is a helpful preference to remember for future interactions."),
            Message::user("Also remind me to review the PR tomorrow"),
            Message::assistant("Got it, I'll remind you to review the PR. Here's a summary of what we discussed."),
            Message::user("Thanks!"),
        ];
        let report = reflector.reflect("u1", None, &messages).await.unwrap();
        assert!(!report.user_preferences.is_empty());
        assert!(!report.action_items.is_empty());
    }
}
