//! Context Management Module
//!
//! This module provides the `ContextManager` which is responsible for:
//! - Managing conversation history (short-term memory)
//! - Constructing the final prompt/messages for the LLM
//! - Handling token budgeting and windowing
//! - Injecting system prompts and dynamic context (RAG)

use crate::agent::message::Message;
use crate::error::Result;

/// Configuration for the Context Manager
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Maximum tokens allowed in the context window
    pub max_tokens: usize,
    /// Maximum number of messages to keep in history
    pub max_history_messages: usize,
    /// Reserve tokens for the response
    pub response_reserve: usize,
    /// Whether to enable explicit context caching markers
    pub enable_cache_control: bool,
    /// Whether to summarize pruned history
    pub smart_pruning: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 128000, // Modern default (e.g. GPT-4o)
            max_history_messages: 50,
            response_reserve: 4096,
            enable_cache_control: false,
            smart_pruning: false,
        }
    }
}

/// Trait for injecting dynamic context
#[async_trait::async_trait]
pub trait ContextInjector: Send + Sync {
    /// Generate messages to inject into the context
    async fn inject(&self, history: &[Message]) -> Result<Vec<Message>>;
}

/// Manages the context window for an agent
pub struct ContextManager {
    config: ContextConfig,
    system_prompt: Option<String>,
    injectors: Vec<Box<dyn ContextInjector>>,
}

impl ContextManager {
    /// Create a new ContextManager
    pub fn new(config: ContextConfig) -> Self {
        Self {
            config,
            system_prompt: None,
            injectors: Vec::new(),
        }
    }

    /// Set the system prompt
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
    }

    /// Add a context injector
    pub fn add_injector(&mut self, injector: Box<dyn ContextInjector>) {
        self.injectors.push(injector);
    }

    /// Construct the final list of messages to send to the provider
    ///
    /// This method applies:
    /// 1. System prompt injection (Protected)
    /// 2. Dynamic Context Injection (RAG, etc.) (Protected)
    /// 3. Token budgeting using tiktoken (Soft Pruning)
    /// 4. Message windowing (based on strategy)
    /// Construct the final list of messages to send to the provider
    ///
    /// This method applies:
    /// 1. System prompt injection (Protected Prefix)
    /// 2. Dynamic Context Injection (Protected Prefix)
    /// 3. Progressive Pruning (Soft Trim & Hard Clear)
    /// 4. Observation Log Anchoring (Tail-end summary)
    pub async fn build_context(
        &self,
        history: &[Message],
        strategy: &crate::agent::attempt::Strategy,
    ) -> Result<Vec<Message>> {
        // 1. Initialize Tokenizer
        let bpe = tiktoken_rs::cl100k_base().map_err(|e| {
            crate::error::Error::Internal(format!("Failed to load tokenizer: {}", e))
        })?;

        // --- SECTION A: Protected Static Prefix (P1) ---
        // This part should be as stable as possible to maximize KV Cache hits.
        let mut static_prefix = Vec::new();

        if let Some(prompt) = &self.system_prompt {
            static_prefix.push(Message::system(prompt.clone()));
        }

        // Feature: Fallback Strategy - Break early (Survival mode)
        if matches!(strategy, crate::agent::attempt::Strategy::Fallback) {
            if let Some(last) = history.last() {
                static_prefix.push(last.clone());
            }
            return Ok(static_prefix);
        }

        // Run Injectors (Static RAG, Skills indices)
        for injector in &self.injectors {
            match injector.inject(history).await {
                Ok(msgs) => static_prefix.extend(msgs),
                Err(e) => tracing::warn!("Context injector failed: {}", e),
            }
        }

        // P9: Context Metrics (Prefix Stability)
        // Calculate hash of static prefix to track KV Cache hits
        let prefix_text = static_prefix.iter().map(|m| m.content.as_text()).collect::<String>();
        let prefix_hash = fxhash::hash64(&prefix_text);
        tracing::debug!(hash = %prefix_hash, count = %static_prefix.len(), "Context Static Prefix Hash (P1)");

        // --- SECTION B: Budget Calculation ---
        const SAFETY_MARGIN: usize = 1000;
        let reserved_response = self.config.response_reserve;
        let max_window = self.config.max_tokens;

        let prefix_tokens = Self::estimate_tokens(&static_prefix);
        let total_reserved = reserved_response + SAFETY_MARGIN + prefix_tokens;
        let history_budget = max_window.saturating_sub(total_reserved);

        // --- SECTION C: Dynamic History Selection & Pruning (P2, P4) ---
        let mut selected_history = Vec::new();
        let mut history_usage = 0;
        let mut pruned_messages = Vec::new();

        // Stage 1: Determination of window size
        let effective_max_history = match strategy {
            crate::agent::attempt::Strategy::Standard => self.config.max_history_messages,
            crate::agent::attempt::Strategy::Compressed => self.config.max_history_messages / 2,
            crate::agent::attempt::Strategy::Fallback => 1,
        };

        let history_slice = if history.len() > effective_max_history {
            let (pruned, selected) = history.split_at(history.len() - effective_max_history);
            pruned_messages.extend(pruned.iter().cloned());
            selected
        } else {
            history
        };

        // Stage 2: Selection with Defensive Trimming (Soft Trim)
        // Iterate REVERSE (Latest first)
        for mut msg in history_slice.iter().rev().cloned() {
            let mut tokens = bpe.encode_with_special_tokens(&msg.content.as_text()).len();
            
            // P4: Stage 1 Pruning (Soft Trim) - If a single message is huge, trim it immediately
            // This prevents one giant tool output from flushing the whole history.
            if tokens > 2000 {
                msg.soft_trim(4000); // Keep approx 1000 tokens head/tail
                tokens = bpe.encode_with_special_tokens(&msg.content.as_text()).len();
            }

            let cost = tokens + 4;

            if history_usage + cost <= history_budget {
                history_usage += cost;
                selected_history.push(msg);
            } else {
                // P4: Stage 2 Pruning (Hard Clear) - If selected_history already has enough,
                // we treat the rest as pruned.
                pruned_messages.push(msg);
            }
        }

        selected_history.reverse();

        // --- SECTION D: Observation Log Anchoring (P5) ---
        // Instead of putting the log at the START (which breaks KV cache every turn),
        // we put it at the start of the SELECTED HISTORY or just before the latest messages.
        // Here we've opted for: [Static Prefix] -> [Observation Log] -> [History]
        // But wait, to keep prefix stable, it's better to append the log as a bridge.
        
        let mut final_messages = static_prefix;

        let enable_smart_pruning = self.config.smart_pruning
            || matches!(strategy, crate::agent::attempt::Strategy::Compressed);

        if enable_smart_pruning && !pruned_messages.is_empty() {
            let mut log = String::from("### Historical Context Summary (Pruned)\n");
            log.push_str("To save context space, early history was summarized below:\n");
            
            // Sort pruned messages back to chronological for summary
            pruned_messages.reverse(); 
            for msg in pruned_messages {
                match msg.role {
                    crate::agent::message::Role::Assistant => {
                        let text = msg.content.as_text();
                        let snippet = if text.len() > 64 {
                            format!("{}...", &text[..60].replace('\n', " "))
                        } else {
                            text.replace('\n', " ")
                        };
                        log.push_str(&format!("- Assistant decision: {}\n", snippet));
                    }
                    crate::agent::message::Role::Tool => {
                        let name = msg.name.as_deref().unwrap_or("unknown_tool");
                        log.push_str(&format!("- Result from: {}\n", name));
                    }
                    crate::agent::message::Role::User => {
                        let text = msg.content.as_text();
                        log.push_str(&format!("- User requested: {}\n", if text.len() > 40 { format!("{}...", &text[..40]) } else { text }));
                    }
                    _ => {}
                }
            }
            final_messages.push(Message::system(log));
        }

        // Finally add the selected recent history
        final_messages.extend(selected_history);

        Ok(final_messages)
    }

    /// Estimate token count for a list of messages using tiktoken
    pub fn estimate_tokens(messages: &[Message]) -> usize {
        if let Ok(bpe) = tiktoken_rs::cl100k_base() {
            messages
                .iter()
                .map(|m| bpe.encode_with_special_tokens(&m.content.as_text()).len() + 4)
                .sum()
        } else {
            // Fallback to heuristic if tokenizer fails
            messages
                .iter()
                .map(|m| m.content.as_text().len() / 4)
                .sum::<usize>()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::agent::message::Content;

    #[tokio::test]
    async fn test_smart_pruning_generation() {
        let config = ContextConfig {
            max_history_messages: 2, // Only keep 2 latest messages
            max_tokens: 10000,
            response_reserve: 1000,
            smart_pruning: true,
            ..Default::default()
        };
        let mut mgr = ContextManager::new(config);
        mgr.set_system_prompt("System Prompt");

        let history = vec![
            Message::assistant("I am thinking about the first task."),
            Message::user("What about the second one?"),
            Message::assistant("Executing the third part now."),
            Message::user("Final question."),
        ];

        // Should keep "Executing the third part now." and "Final question."
        // And summarize "I am thinking about the first task." and "What about the second one?"
        let ctx = mgr
            .build_context(&history, &crate::agent::attempt::Strategy::Standard)
            .await
            .unwrap();

        // System Prompt + Observation Log + 2 History Messages = 4 messages
        assert_eq!(
            ctx.len(),
            4,
            "Context should contain System, Log, and 2 history messages"
        );

        let log_msg = &ctx[1];
        assert!(
            log_msg.content.as_text().contains("Historical Context Summary"),
            "Should contain Historical Context Summary"
        );
        assert!(
            log_msg.content.as_text().contains("Assistant"),
            "Should mention Assistant in log"
        );
    }

    #[tokio::test]
    async fn test_tail_anchored_log() {
        let config = ContextConfig {
            max_history_messages: 1,
            max_tokens: 10000,
            smart_pruning: true,
            ..Default::default()
        };
        let mut mgr = ContextManager::new(config);
        mgr.set_system_prompt("SYSTEM_PREFIX");

        let history = vec![
            Message::assistant("Pruned message"),
            Message::user("Recent message"),
        ];

        let ctx = mgr.build_context(&history, &crate::agent::attempt::Strategy::Standard).await.unwrap();

        // Expected order: [System] -> [Log] -> [Recent Message]
        assert_eq!(ctx.len(), 3);
        assert_eq!(ctx[0].content.as_text(), "SYSTEM_PREFIX");
        assert!(ctx[1].content.as_text().contains("Historical Context Summary"));
        assert_eq!(ctx[2].content.as_text(), "Recent message");
    }

    #[test]
    fn test_soft_trim_utility() {
        let mut msg = Message::user("A".repeat(10000));
        msg.soft_trim(2000);
        let text = msg.text();
        assert!(text.contains("trimmed for context optimization"));
        assert!(text.len() < 3000);
        assert!(text.starts_with(&"A".repeat(100)));
        assert!(text.ends_with(&"A".repeat(100)));
    }
}
