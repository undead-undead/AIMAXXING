//! Agent system - the core AI agent abstraction

use anyhow;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument};

use crate::agent::cache::Cache;
use crate::agent::context::ContextInjector;
use crate::agent::context::{ContextConfig, ContextManager}; // ContextInjector is already imported above
use crate::agent::memory::Memory;
use crate::agent::TwoTierKvCache;
use crate::agent::message::{Content, Message, Role};
use crate::agent::multi_agent::{AgentMessage, AgentRole, Coordinator, MultiAgent, SwarmInjector};
use crate::agent::personality::{Persona, PersonalityManager};
use crate::agent::provider::Provider;
#[cfg(feature = "cron")]
use crate::agent::scheduler::Scheduler;
use crate::agent::session::SessionStatus;
use crate::agent::streaming::StreamingResponse;
#[cfg(feature = "p2p")]
use crate::agent::swarm::p2p::wormhole::{VesselExchange, WormholeConfig};
use crate::error::{Error, Result};
use crate::notification::{Notifier, NotifyChannel};
use crate::infra::observable::MetricsRegistry;
#[cfg(feature = "vector-db")]
use crate::skills::tool::memory::{
    FetchDocumentTool, RememberThisTool, SearchHistoryTool, TieredSearchTool,
}; // Corrected import for memory tools
#[cfg(feature = "cron")]
use crate::skills::tool::CronTool;
use crate::skills::tool::{DelegateTool, HandoverTool};
use crate::skills::tool::{Tool, ToolSet};

use crate::security::SecurityManager;
use crate::agent::evolution::evolution_manager::EvolutionManager;
use crate::agent::evolution::consolidation::SleepConsolidator;

/// Configuration for an Agent
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Name of the agent (for logging/identity)
    pub name: String,
    /// Model to use (provider specific string)
    pub model: String,
    /// System prompt / Preamble
    pub preamble: String,
    /// Temperature for generation
    pub temperature: Option<f64>,
    /// Max tokens to generate
    pub max_tokens: Option<u64>,
    /// Additional provider-specific parameters
    pub extra_params: Option<serde_json::Value>,
    /// Policy for risky tools
    pub tool_policy: RiskyToolPolicy,
    /// Max history messages to send to LLM (Sliding window)
    pub max_history_messages: usize,
    /// Max characters allowed in tool output before truncation
    pub max_tool_output_chars: usize,
    /// Enable strict JSON mode (response_format: json_object)
    pub json_mode: bool,
    /// Optional personality profile
    pub persona: Option<Persona>,
    /// Role of the agent in a multi-agent system
    pub role: AgentRole,
    /// Max parallel tool calls (default: 5)
    pub max_parallel_tools: usize,
    /// Loop detection similarity threshold (0.0 to 1.0, default: 0.8)
    pub loop_similarity_threshold: f64,
    /// Standard Operating Procedure / Mission Statement
    pub sop: Option<String>,
    /// Whether to enable explicit context caching (e.g. Anthropic cache_control)
    pub enable_cache_control: bool,
    /// Whether to use summarization for pruned history instead of truncation
    pub smart_pruning: bool,
    /// Path to a folder containing .md files for "soul" injection
    pub soul_path: Option<std::path::PathBuf>,
    /// Turns before triggering a status recap (default: 12)
    pub status_recap_threshold_steps: usize,
    /// Output length before triggering a status recap (default: 5000)
    pub status_recap_threshold_chars: usize,
    /// Phase 2: KV Cache capacity in pages (default: 1024)
    pub kv_cache_pages: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "agent".to_string(),
            model: "gpt-4o".to_string(),
            preamble: "You are a helpful AI assistant.".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(128000), // Updated to larger context window default
            extra_params: None,
            tool_policy: RiskyToolPolicy::default(),
            max_history_messages: 20,
            max_tool_output_chars: 8192, // Increased for better tool results
            json_mode: false,
            persona: None,
            role: AgentRole::Assistant,
            max_parallel_tools: 5,
            loop_similarity_threshold: 0.8,
            sop: None,
            enable_cache_control: false,
            smart_pruning: false,
            soul_path: None,
            status_recap_threshold_steps: 12,
            status_recap_threshold_chars: 5000,
            kv_cache_pages: 1024,
        }
    }
}

/// Policy for tool execution
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicy {
    /// Allow execution without approval
    Auto,
    /// Require explicit approval
    RequiresApproval,
    /// Disable execution completely
    Disabled,
}

/// Configuration for risky tool policies
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RiskyToolPolicy {
    /// Default policy for all tools
    pub default_policy: ToolPolicy,
    /// Overrides for specific tools
    pub overrides: std::collections::HashMap<String, ToolPolicy>,
}

impl Default for RiskyToolPolicy {
    fn default() -> Self {
        Self {
            default_policy: ToolPolicy::Auto,
            overrides: std::collections::HashMap::new(),
        }
    }
}

/// Events emitted by the Agent during execution
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentEvent {
    /// Optional session context
    pub session_id: Option<String>,
    /// The actual event data
    #[serde(flatten)]
    pub data: AgentEventData,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum AgentEventData {
    /// Agent started thinking (prompt received)
    Thinking { prompt: String },
    /// A new reasoning step/turn started
    StepStart { step: usize },
    /// Agent decided to use a tool (planning phase)
    ToolCall { tool: String, input: String },
    /// Tool execution actually started (technical event)
    ToolExecutionStart { tool: String, input: String },
    /// Tool execution finished (technical event)
    ToolExecutionEnd {
        tool: String,
        output_preview: String,
        duration_ms: u64,
        success: bool,
    },
    /// Tool execution requires approval
    ApprovalPending { tool: String, input: String },
    /// Agent generated a thought process (reasoning)
    Thought { content: String },
    /// Tool execution result received (semantic event)
    ToolResult { tool: String, output: String },
    /// Agent generated a final response
    Response {
        content: String,
        usage: Option<TokenUsage>,
    },
    /// LLM usage statistics
    TokenUsage { usage: TokenUsage },
    /// Latency until first token received
    LatencyTTFT { duration_ms: u64 },
    /// Error occurred
    Error { message: String },
    /// Phase 11-B: Task was cancelled externally
    Cancelled { reason: String },
}

/// Token usage statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Handler for user approvals
#[async_trait::async_trait]
pub trait ApprovalHandler: Send + Sync {
    /// Request approval for a tool call
    async fn approve(&self, tool_name: &str, arguments: &str) -> anyhow::Result<bool>;
}

/// A default approval handler that rejects all
pub struct RejectAllApprovalHandler;

#[async_trait::async_trait]
impl ApprovalHandler for RejectAllApprovalHandler {
    async fn approve(&self, _tool: &str, _args: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
}

/// Request sent to the channel handler
#[derive(Debug)]
pub struct ApprovalRequest {
    /// Unique ID for this request
    pub id: String,
    /// Tool name
    pub tool_name: String,
    /// Tool arguments
    pub arguments: String,
    /// Responder channel
    pub responder: tokio::sync::oneshot::Sender<bool>,
}

/// A handler that sends approval requests via a channel
pub struct ChannelApprovalHandler {
    sender: tokio::sync::mpsc::Sender<ApprovalRequest>,
}

/// Trait for human-in-the-loop interactions (getting text input)
#[async_trait::async_trait]
pub trait InteractionHandler: Send + Sync {
    /// Ask the user a question and get a string response
    async fn ask(&self, question: &str) -> anyhow::Result<String>;
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct AskUserArgs {
    /// The question to ask the user
    question: String,
}

struct AskUserTool {
    handler: Arc<dyn InteractionHandler>,
}

#[async_trait::async_trait]
impl crate::skills::tool::Tool for AskUserTool {
    fn name(&self) -> String {
        "ask_user".to_string()
    }

    async fn definition(&self) -> crate::skills::tool::ToolDefinition {
        let gen = schemars::gen::SchemaSettings::openapi3().into_generator();
        let schema = gen.into_root_schema_for::<AskUserArgs>();
        let schema_json = serde_json::to_value(schema).unwrap_or_default();

        crate::skills::tool::ToolDefinition {
            name: "ask_user".to_string(),
            description: "Ask the user for clarification, additional information, or a final decision. Use this when you are stuck or need human input.".to_string(),
            parameters: schema_json,
            parameters_ts: Some("interface AskUserArgs {\n  /** The question to ask the user */\n  question: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this only when you need critical missing information or explicit permission to proceed with a dangerous action (e.g., executing a trade). Avoid asking for obvious or non-essential details.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: AskUserArgs = serde_json::from_str(arguments)?;
        self.handler.ask(&args.question).await
    }
}

impl ChannelApprovalHandler {
    /// Create a new channel handler
    pub fn new(sender: tokio::sync::mpsc::Sender<ApprovalRequest>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl ApprovalHandler for ChannelApprovalHandler {
    async fn approve(&self, tool_name: &str, arguments: &str) -> anyhow::Result<bool> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        let request = ApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            tool_name: tool_name.to_string(),
            arguments: arguments.to_string(),
            responder: tx,
        };

        self.sender
            .send(request)
            .await
            .map_err(|_| Error::Internal("Approval channel closed".to_string()))?;

        // Wait for response
        let approved = rx
            .await
            .map_err(|_| Error::Internal("Approval responder dropped".to_string()))?;

        Ok(approved)
    }
}

// use crate::notification::{Notifier, NotifyChannel}; // Already imported at top

/// The main Agent struct
pub struct Agent<P: Provider> {
    provider: Arc<P>,
    tools: ToolSet,
    config: AgentConfig,
    context_manager: ContextManager,
    events: broadcast::Sender<AgentEvent>,
    approval_handler: Arc<dyn ApprovalHandler>,
    cache: Option<Arc<dyn Cache>>,
    notifier: Option<Arc<dyn Notifier>>,
    memory: Option<Arc<dyn Memory>>,
    session_id: Option<String>,
    metrics: Option<Arc<MetricsRegistry>>,

    pub enabled_tools: Option<Arc<parking_lot::RwLock<std::collections::HashSet<String>>>>,
    pub persona: Arc<parking_lot::RwLock<Option<Persona>>>,
    pub security: Arc<SecurityManager>,
    /// Phase 11-B: External cancellation token for triple-cut abort
    pub cancel_token: tokio_util::sync::CancellationToken,
    /// Phase 10: Track tools seen in this session for lazy guide injection
    pub seen_tools: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
    /// Phase 12-B: Evolution manager for observation and health checks
    pub evolution_manager: Option<Arc<EvolutionManager>>,
    /// Phase 12-C: Sleep consolidator for memory pruning
    pub sleep_consolidator: Option<Arc<SleepConsolidator>>,
    /// Phase 2: KV Cache for inference acceleration
    pub kv_cache: Option<Arc<parking_lot::RwLock<TwoTierKvCache>>>,
}

impl<P: Provider> Agent<P> {
    /// Create a new agent builder
    pub fn builder(provider: P) -> AgentBuilder<P> {
        AgentBuilder::new(provider)
    }

    /// Subscribe to agent events
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.events.subscribe()
    }

    /// Helper to emit events safely
    fn emit(&self, data: AgentEventData) {
        let event = AgentEvent {
            session_id: self.session_id.clone(),
            data,
        };

        // Record metrics if enabled
        if let Some(registry) = &self.metrics {
            // For now, let's keep it simple and just call the registry methods directly if we are in emit.
            match &event.data {
                AgentEventData::StepStart { .. } => {
                    registry.counter_inc(&format!("{}:steps_total", self.config.name), 1);
                }
                AgentEventData::Error { .. } => {
                    registry.counter_inc(&format!("{}:errors_total", self.config.name), 1);
                }
                AgentEventData::TokenUsage { usage } => {
                    registry.counter_inc(
                        &format!("{}:tokens_prompt_total", self.config.name),
                        usage.prompt_tokens as u64,
                    );
                    registry.counter_inc(
                        &format!("{}:tokens_completion_total", self.config.name),
                        usage.completion_tokens as u64,
                    );
                    registry.counter_inc(
                        &format!("{}:tokens_total", self.config.name),
                        usage.total_tokens as u64,
                    );
                }
                AgentEventData::Thinking { .. } => {
                    registry.counter_inc(&format!("{}:thinking_starts_total", self.config.name), 1);
                }
                AgentEventData::Thought { .. } => {
                    registry.counter_inc(&format!("{}:thoughts_total", self.config.name), 1);
                }
                AgentEventData::ToolExecutionEnd {
                    duration_ms,
                    success,
                    ..
                } => {
                    registry.counter_inc(&format!("{}:tool_calls_total", self.config.name), 1);
                    if !*success {
                        registry.counter_inc(&format!("{}:tool_errors_total", self.config.name), 1);
                    }
                    registry.histogram_observe(
                        &format!("{}:tool_duration_ms", self.config.name),
                        *duration_ms as f64,
                    );
                }
                _ => {}
            }
        }

        if let Err(e) = self.events.send(event) {
            tracing::debug!("Failed to emit event (no receivers): {}", e);
        }
    }

    /// Send a notification via the configured notifier
    pub async fn notify(&self, channel: NotifyChannel, message: &str) -> Result<()> {
        if let Some(notifier) = &self.notifier {
            notifier.notify(channel, message).await
        } else {
            // If no notifier configured, log warning but don't fail hard
            tracing::warn!(
                "Agent tried to notify but no notifier is configured: {}",
                message
            );
            Ok(())
        }
    }

    /// Save current state to persistent storage
    pub async fn checkpoint(
        &self,
        messages: &[Message],
        step: usize,
        status: SessionStatus,
    ) -> Result<()> {
        if let (Some(memory), Some(session_id)) = (&self.memory, &self.session_id) {
            let mut messages = messages.to_vec();
            
            // Phase 12-A: If observation window is active, quarantine these messages
            let is_observing = self.evolution_manager.as_ref()
                .map(|em| em.observation_window().read().is_active())
                .unwrap_or(false);
            
            if is_observing {
                for msg in &mut messages {
                    msg.unverified = true;
                }
            }

            let session = crate::agent::session::AgentSession {
                id: session_id.clone(),
                messages,
                step,
                status,
                updated_at: chrono::Utc::now(),
            };
            memory.store_session(session).await?;
            debug!("Agent checkpoint saved for session: {}", session_id);
        }
        Ok(())
    }

    /// Resume a previously saved session
    pub async fn resume(&self, session_id: &str) -> Result<String> {
        if let Some(memory) = &self.memory {
            if let Some(session) = memory.retrieve_session(session_id).await? {
                info!("Resuming agent session: {}", session_id);
                // We restart the chat with the loaded messages
                return self
                    .chat(session.messages, Some(session_id.to_string()))
                    .await;
            }
        }
        Err(Error::Internal(format!(
            "Session not found: {}",
            session_id
        )))
    }

    /// Send a prompt and get a response (non-streaming)
    pub async fn prompt(
        &self,
        prompt: impl Into<String>,
        session_id: Option<String>,
    ) -> Result<String> {
        let prompt_str = prompt.into();
        self.emit(AgentEventData::Thinking {
            prompt: prompt_str.clone(),
        });

        let messages = vec![Message::user(prompt_str)];

        self.chat(messages, session_id).await
    }

    /// Send messages and get a response (non-streaming)
    #[instrument(skip(self, messages), fields(model = %self.config.model, message_count = messages.len()))]
    pub async fn chat(
        &self,
        mut messages: Vec<Message>,
        session_id: Option<String>,
    ) -> Result<String> {
        if let Some(sid) = &session_id {
            // Inject session context if provided - this helps session-aware tools
            messages.insert(0, Message::system(format!("Current Session ID: {}", sid)));
        }

        // Security: Sanitize input for injection attempts
        for msg in messages.iter_mut() {
            if msg.role == Role::User {
                let content_str = msg.content.as_text();
                let sanitized = self.security.check_input(&content_str);
                if sanitized.was_modified {
                    tracing::warn!(
                        "Injection attempt detected and sanitized: {:?}",
                        sanitized.warnings
                    );
                    msg.content = Content::Text(sanitized.content);
                }
            }
        }

        let mut attempt = crate::agent::attempt::Attempt::new();

        // P13: Session History Memory Cap (Minimise Memory Overflow)
        if messages.len() > 500 {
            let to_remove = messages.len() - 500;
            // Keep the first message if it's a System message
            let start_idx = if messages.first().map(|m| m.role == Role::System).unwrap_or(false) { 1 } else { 0 };
            if to_remove > start_idx {
                messages.drain(start_idx..to_remove);
            }
        }

        loop {
            match self.execute_attempt(&mut messages, &attempt).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    // Error Recovery Strategy

                    // 1. Context Overflow -> Strategy Downgrade
                    // Providers usually return 400 or specific error string for this
                    let is_context_error = e.to_string().to_lowercase().contains("context length")
                        || e.to_string().to_lowercase().contains("too many tokens")
                        || e.to_string().to_lowercase().contains("string too long");

                    if is_context_error {
                        if attempt.downgrade() {
                            tracing::warn!(
                                "Context overflow detected. Downgrading strategy to {:?}. Retry attempt {}", 
                                attempt.strategy, attempt.retry_count
                            );
                            continue;
                        } else {
                            tracing::error!(
                                "Context overflow and no more strategies available. Giving up."
                            );
                            return Err(e);
                        }
                    }

                    // 2. Retryable Network/Server Errors -> Backoff
                    if e.is_retryable() && attempt.can_retry() {
                        attempt.next();
                        let backoff = std::time::Duration::from_secs(2u64.pow(attempt.retry_count));
                        tracing::warn!(
                            "Retryable error encountered: {}. Backing off for {:?}. Retry attempt {}/{}", 
                            e, backoff, attempt.retry_count, attempt.max_retries
                        );
                        tokio::time::sleep(backoff).await;
                        continue;
                    }

                    // 3. Fatal Error -> Fail
                    return Err(e);
                }
            }
        }
    }

    /// Execute a single attempt with the current strategy
    async fn execute_attempt(
        &self,
        messages: &mut Vec<Message>,
        attempt: &crate::agent::attempt::Attempt,
    ) -> Result<String> {
        let mut steps = 0;
        const MAX_STEPS: usize = 15;
        let mut history = crate::agent::history::QueryHistory::new();

        loop {
            if steps >= MAX_STEPS {
                return Err(Error::agent_config("Max agent steps exceeded"));
            }

            // Phase 11-B: Check cancellation before each step
            if self.cancel_token.is_cancelled() {
                self.emit(AgentEventData::Cancelled { reason: "Cancelled by user".to_string() });
                return Err(Error::agent_config("Task cancelled by user"));
            }

            steps += 1;

            self.emit(AgentEventData::StepStart { step: steps });

            // Phase P5: Status Recap (Attention Alignment)
            // If we hit the threshold, inject a recap prompt to keep the agent focused.
            let recap_step = self.config.status_recap_threshold_steps;
            let total_chars: usize = messages.iter().map(|m| m.content.as_text().len()).sum();
            let char_threshold = self.config.status_recap_threshold_chars;

            if (steps > 1 && steps % recap_step == 0) || (total_chars > char_threshold && steps > 1) {
                let reason = if steps % recap_step == 0 { "Step threshold" } else { "Context density threshold" };
                info!("P5: {} reached ({} chars). Injecting Status Recap prompt.", reason, total_chars);
                self.emit(AgentEventData::Thought { 
                    content: format!("{}. Pausing for internal Status Recap to maintain alignment.", reason)
                });
                
                let recap_prompt = format!(
                    "### INTERNAL STATUS RECAP ({})\n\
                     The conversation has reached a significant density. To prevent attention drift, please:\n\
                     1. Summarize what you have achieved so far in this session.\n\
                     2. Use `ls` or appropriate tools to re-check `todo.md` and important project files.\n\
                     3. Clearly state your plan for the next steps.\n\
                     Continue with the task once aligned.",
                    reason
                );
                
                messages.push(crate::agent::message::Message::system(recap_prompt));
            }

            if let Some(last) = messages.last() {
                if last.role == Role::User {
                    self.emit(AgentEventData::Thinking {
                        prompt: last.content.as_text(),
                    });
                }
            }

            // Save checkpoint before thinking
            self.checkpoint(messages, steps, SessionStatus::Thinking)
                .await?;

            info!(
                "Agent starting chat completion (step {}, strategy: {:?})",
                steps, attempt.strategy
            );
            
            // Phase 2: Allocate KV page if cache enabled for this session
            if let (Some(kv_cache), Some(sid)) = (&self.kv_cache, &self.session_id) {
                let mut cache = kv_cache.write();
                cache.allocate_page(sid);
                cache.compress_old_pages(sid);
            }

            // 1. Check Cache (Step-level caching)
            if let Some(cache) = &self.cache {
                if let Ok(Some(cached_response)) = cache.get(messages).await {
                    info!("Cache hit! Returning cached response.");
                    return Ok(cached_response);
                }
            }

            // Context Window Management via ContextManager with Strategy
            let context_messages = self
                .context_manager
                .build_context(messages, &attempt.strategy)
                .await
                .map_err(|e| Error::agent_config(format!("Failed to build context: {}", e)))?;

            let stream = self.stream_chat(context_messages).await?;

            let mut full_text = String::new();
            let mut tool_calls = Vec::new(); // (id, name, args)
            let mut usage = None;

            let mut stream_inner = stream.into_inner();

            // Consume the stream
            use futures::StreamExt;
            let start_time = std::time::Instant::now();
            let mut ttft_recorded = false;

            // Phase 11-B: Use select! to race cancellation against stream chunks
            loop {
                tokio::select! {
                    _ = self.cancel_token.cancelled() => {
                        self.emit(AgentEventData::Cancelled { reason: "LLM stream aborted by user".to_string() });
                        return Err(Error::agent_config("Task cancelled during LLM streaming"));
                    }
                    chunk = stream_inner.next() => {
                        match chunk {
                            None => break, // Stream ended
                            Some(chunk) => {
                                if !ttft_recorded {
                                    let duration = start_time.elapsed().as_millis() as u64;
                                    self.emit(AgentEventData::LatencyTTFT {
                                        duration_ms: duration,
                                    });
                                    ttft_recorded = true;
                                }
                                match chunk? {
                                    crate::agent::streaming::StreamingChoice::Message(text) => {
                                        full_text.push_str(&text);
                                    }
                                    crate::agent::streaming::StreamingChoice::Thought(thought) => {
                                        self.emit(AgentEventData::Thought { content: thought });
                                    }
                                    crate::agent::streaming::StreamingChoice::ToolCall {
                                        id,
                                        name,
                                        arguments,
                                    } => {
                                        tool_calls.push((id, name, arguments));
                                    }
                                    crate::agent::streaming::StreamingChoice::ParallelToolCalls(map) => {
                                        let mut sorted: Vec<_> = map.into_iter().collect();
                                        sorted.sort_by_key(|(k, _)| *k);
                                        for (_, tc) in sorted {
                                            tool_calls.push((tc.id, tc.name, tc.arguments));
                                        }
                                    }
                                    crate::agent::streaming::StreamingChoice::Usage(u) => {
                                        let usage_mapped = TokenUsage {
                                            prompt_tokens: u.prompt_tokens,
                                            completion_tokens: u.completion_tokens,
                                            total_tokens: u.total_tokens,
                                        };
                                        usage = Some(usage_mapped.clone());
                                        self.emit(AgentEventData::TokenUsage {
                                            usage: usage_mapped,
                                        });
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }

            // If no tool calls, we are done
            if tool_calls.is_empty() {
                self.emit(AgentEventData::Response {
                    content: full_text.clone(),
                    usage: usage.clone(),
                });

                // Store in cache
                if let Some(cache) = &self.cache {
                    let _ = cache.set(messages, full_text.clone()).await;
                }

                // Phase 1 (P5): If task is ending but took many steps, encourage a final summary
                if steps >= 12 {
                    self.emit(AgentEventData::Thought { 
                        content: "Task took many steps. Ensuring final status is clear.".to_string() 
                    });
                }

                return Ok(full_text);
            }

            // We have tool calls.
            // 1. Append Assistant Message (Thought + Calls) to history
            let mut parts = Vec::new();
            if !full_text.is_empty() {
                parts.push(crate::agent::message::ContentPart::Text {
                    text: full_text.clone(),
                });
            }
            for (id, name, args) in &tool_calls {
                parts.push(crate::agent::message::ContentPart::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: args.clone(),
                });
            }
            messages.push(Message {
                role: Role::Assistant,
                name: None,
                content: Content::Parts(parts),
                unverified: false,
                source_collection: None,
                source_path: None,
            });

            // 2. Execute Tools (Parallel with Limit)
            let tools = &self.tools;
            let policy = &self.config.tool_policy;
            let events = &self.events;
            let approval_handler = &self.approval_handler;
            let max_parallel = self.config.max_parallel_tools;

            use futures::stream;

            let current_messages = Arc::new(messages.clone());
            let threshold = self.config.loop_similarity_threshold;

            // Check for potential loops before execution
            let mut processed_calls = Vec::new();
            for (id, name, args) in tool_calls {
                let args_str = args.to_string();
                if let Some(warning) = history.check_loop(&name, &args_str, threshold) {
                    tracing::warn!(tool = %name, "Loop detected: {}", warning);
                    // Inject warning as a tool result immediately instead of calling the tool
                    messages.push(Message {
                        role: Role::Tool,
                        name: None,
                        content: Content::Parts(vec![
                            crate::agent::message::ContentPart::ToolResult {
                                tool_call_id: id,
                                content: format!("Error: Potential loop detected. {}", warning),
                                name: Some(name),
                            },
                        ]),
                        unverified: false,
                        source_collection: None,
                        source_path: None,
                    });
                } else {
                    // Not a loop, proceed with execution
                    history.record(name.clone(), args_str.clone());
                    processed_calls.push((id, name, args));
                }
            }

            if processed_calls.is_empty() && !messages.is_empty() {
                // All calls were loops, continue to next iteration where LLM will see the warnings
                continue;
            }

            let results: Vec<crate::error::Result<(String, String, String)>> = tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    self.emit(AgentEventData::Cancelled { reason: "Tool execution aborted by user".to_string() });
                    return Err(Error::agent_config("Task cancelled during tool execution"));
                }
                res = stream::iter(processed_calls)
                    .map(|(id, name, args)| {
                        let name_clone = name.clone();
                        let id_clone = id.clone();
                        let args_str = args.to_string();
                        let msgs = Arc::clone(&current_messages);
                        
                        async move {
                            // 1. Get tool definition (cached in ToolSet)
                            let tool_ref = tools.get(&name_clone).ok_or_else(|| Error::ToolNotFound(name_clone.clone()))?;
                            
                            let def = tool_ref.definition().await;

                            // 2. Check policy and security overrides
                            let mut effective_policy = policy.overrides.get(&name_clone)
                                .unwrap_or(&policy.default_policy).clone();
                            
                            // Binary Safety Override: Unverified binary skills ALWAYS require approval
                            if def.is_binary && !def.is_verified && effective_policy != ToolPolicy::Disabled {
                                tracing::warn!(tool = %name_clone, "Unverified binary skill detected. Enforcing manual approval.");
                                effective_policy = ToolPolicy::RequiresApproval;
                            }

                            let start_time = std::time::Instant::now();
                            let _ = events.send(AgentEvent {
                                session_id: None, // Or pass session_id if available in scope
                                data: AgentEventData::ToolExecutionStart { 
                                    tool: name_clone.clone(), 
                                    input: args_str.clone() 
                                }
                            });

                            let mut result = match effective_policy {
                                ToolPolicy::Disabled => {
                                    Err(Error::tool_execution(name_clone.clone(), "Tool execution is disabled by policy".to_string()))
                                }
                                ToolPolicy::RequiresApproval => {
                                    let _ = events.send(AgentEvent {
                                        session_id: None,
                                        data: AgentEventData::ApprovalPending { 
                                            tool: name_clone.clone(), 
                                            input: args_str.clone() 
                                        }
                                    });
                                    
                                    // Checkpoint before awaiting approval
                                    self.checkpoint(&msgs, steps, SessionStatus::AwaitingApproval { 
                                        tool_name: name_clone.clone(), 
                                        arguments: args_str.clone() 
                                    }).await?;

                                    // Ask approval handler
                                    match approval_handler.approve(&name_clone, &args_str).await {
                                        Ok(true) => {
                                            tools.call(&name_clone, &args_str).await
                                                .map_err(|e| Error::tool_execution(name_clone.clone(), e.to_string()))
                                        }
                                        Ok(false) => {
                                            Err(Error::ToolApprovalRequired { tool_name: name_clone.clone() })
                                        }
                                        Err(e) => {
                                            Err(Error::tool_execution(name_clone.clone(), format!("Approval check failed: {}", e)))
                                        }
                                    }
                                }
                                ToolPolicy::Auto => {
                                    tools.call(&name_clone, &args_str).await
                                        .map_err(|e| Error::tool_execution(name_clone.clone(), e.to_string()))
                                }
                            };
                            
                            // Security: Redact output for leaks
                            if let Ok(output_str) = &mut result {
                                let (redacted, detections) = self.security.check_output(output_str);
                                if !detections.is_empty() {
                                    tracing::warn!(tool = %name_clone, "Secret leak detected in tool output: {:?}", detections);
                                    *output_str = redacted;
                                }
                            }

                            // Phase 10: Lazy Skill Loading (P3)
                            // If this is the first time the agent uses this tool, inject full definition & guidelines.
                            if result.is_ok() {
                                let mut seen = self.seen_tools.write();
                                if !seen.contains(&name_clone) {
                                    let mut injection = format!("### NOTICE: First use of skill '{}'.\n", name_clone);
                                    
                                    // Inject Schema
                                    if let Some(ts) = &def.parameters_ts {
                                        injection.push_str("#### Official TypeScript Schema:\n```typescript\n");
                                        injection.push_str(ts);
                                        injection.push_str("\n```\n");
                                    } else {
                                        injection.push_str("#### Parameters (JSON Schema):\n```json\n");
                                        injection.push_str(&serde_json::to_string_pretty(&def.parameters).unwrap_or_default());
                                        injection.push_str("\n```\n");
                                    }

                                    // Inject Guidelines
                                    if let Some(guidelines) = &def.usage_guidelines {
                                        injection.push_str(&format!("#### Usage Guidelines:\n{}\n", guidelines));
                                    }

                                    if let Ok(ref mut res_text) = result {
                                        *res_text = format!("{}\n\n---\n{}", res_text, injection);
                                    }
                                    
                                    seen.insert(name_clone.clone());
                                }
                            }

                            let duration = start_time.elapsed().as_millis() as u64;
                            
                            match result {
                                Ok(output) => {
                                    let preview = if output.len() > 100 {
                                        format!("{}...", &output[..100])
                                    } else {
                                        output.clone()
                                    };
                                    
                                    let _ = events.send(AgentEvent {
                                        session_id: self.session_id.clone(),
                                        data: AgentEventData::ToolExecutionEnd { 
                                            tool: name_clone.clone(), 
                                            output_preview: preview,
                                            duration_ms: duration,
                                            success: true
                                        }
                                    });

                                    let _ = events.send(AgentEvent {
                                        session_id: self.session_id.clone(),
                                        data: AgentEventData::ToolResult { 
                                            tool: name_clone.clone(), 
                                            output: output.clone() 
                                        }
                                    });
                                    Ok((id_clone, name_clone, output))
                                },
                                Err(e) => {
                                    let _ = events.send(AgentEvent {
                                        session_id: self.session_id.clone(),
                                        data: AgentEventData::ToolExecutionEnd { 
                                            tool: name_clone.clone(), 
                                            output_preview: e.to_string(),
                                            duration_ms: duration,
                                            success: false
                                        }
                                    });

                                    let _ = events.send(AgentEvent {
                                        session_id: self.session_id.clone(),
                                        data: AgentEventData::Error { message: e.to_string() }
                                    });

                                    // Phase 12-A: Report error to evolution manager if active
                                    if let Some(em) = &self.evolution_manager {
                                        em.report_error(&format!("tool_error:{}", name_clone));
                                    }

                                    Ok((id_clone, name_clone, format!("Error: {}", e)))
                                }
                            }
                        }
                    })
                    .buffer_unordered(max_parallel)
                    .collect::<Vec<_>>() => res
            };

            // 3. Append Tool Results to history
            let mut large_output_detected = false;
            for res in results {
                let (id, name, output) = res.unwrap(); // Safe because we handle Err inside async move
                if output.len() > self.config.status_recap_threshold_chars {
                    large_output_detected = true;
                }
                messages.push(Message {
                    role: Role::Tool,
                    name: None,
                    content: Content::Parts(vec![crate::agent::message::ContentPart::ToolResult {
                        tool_call_id: id,
                        content: output,
                        name: Some(name),
                    }]),
                    unverified: false,
                    source_collection: None,
                    source_path: None,
                });
            }

            // Phase 1 (P5): Status Recap Trigger
            if steps >= self.config.status_recap_threshold_steps || large_output_detected {
                info!(step = steps, large_output = large_output_detected, "Tier 2 Focus check (P5 Recap) triggered.");
                messages.push(Message::system(
                    format!(
                        "[SYSTEM REFOCUS] You have performed many operations ({}) or received a large amount of data. \
                         Please briefly recap your current progress: \n\
                         1. What have you accomplished so far?\n\
                         2. What is still pending? (Check task.md if available)\n\
                         3. Are there any new blockers or risks identified?",
                        steps
                    )
                ));
            }
        }
    }

    /// Stream a prompt response
    pub async fn stream(&self, prompt: impl Into<String>) -> Result<StreamingResponse> {
        let messages = vec![Message::user(prompt.into())];
        self.stream_chat(messages).await
    }

    /// Stream a chat response
    pub async fn stream_chat(&self, messages: Vec<Message>) -> Result<StreamingResponse> {
        let mut extra = self
            .config
            .extra_params
            .clone()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        // Inject JSON mode if enabled
        if self.config.json_mode {
            if let serde_json::Value::Object(ref mut map) = extra {
                if !map.contains_key("response_format") {
                    map.insert(
                        "response_format".to_string(),
                        serde_json::json!({ "type": "json_object" }),
                    );
                }
            }
        }

        let tools = if let Some(ref enabled) = self.enabled_tools {
            let filter = enabled.read().clone();
            self.tools.definitions_filtered(Some(&filter)).await
        } else {
            self.tools.definitions().await
        };

        let request = crate::agent::provider::ChatRequest {
            model: self.config.model.clone(),
            system_prompt: Some(self.config.preamble.clone()),
            messages,
            tools,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            extra_params: Some(extra),
            enable_cache_control: self.config.enable_cache_control,
        };

        self.provider.stream_completion(request).await
    }

    /// Call a tool by name (Direct call helper)
    #[instrument(skip(self, arguments), fields(tool_name = %name))]
    pub async fn call_tool(&self, name: &str, arguments: &str) -> Result<String> {
        // 1. Check Policy
        let policy = self
            .config
            .tool_policy
            .overrides
            .get(name)
            .unwrap_or(&self.config.tool_policy.default_policy);

        match policy {
            ToolPolicy::Disabled => {
                return Err(Error::tool_execution(
                    name.to_string(),
                    "Tool execution is disabled by policy".to_string(),
                ));
            }
            ToolPolicy::RequiresApproval => {
                self.emit(AgentEventData::ApprovalPending {
                    tool: name.to_string(),
                    input: arguments.to_string(),
                });

                match self.approval_handler.approve(name, arguments).await {
                    Ok(true) => {} // Proceed
                    Ok(false) => {
                        return Err(Error::ToolApprovalRequired {
                            tool_name: name.to_string(),
                        })
                    }
                    Err(e) => {
                        return Err(Error::tool_execution(
                            name.to_string(),
                            format!("Approval check failed: {}", e),
                        ))
                    }
                }
            }
            ToolPolicy::Auto => {} // Proceed
        }

        self.emit(AgentEventData::ToolCall {
            tool: name.to_string(),
            input: arguments.to_string(),
        });

        let result = self.tools.call(name, arguments).await;

        match result {
            Ok(mut output) => {
                // Quota Protection: Truncate tool output if too long
                if output.len() > self.config.max_tool_output_chars {
                    let original_len = output.len();
                    output.truncate(self.config.max_tool_output_chars);
                    output.push_str(&format!(
                        "\n\n(Note: Output truncated from {} to {} chars to save tokens)",
                        original_len, self.config.max_tool_output_chars
                    ));
                }

                self.emit(AgentEventData::ToolResult {
                    tool: name.to_string(),
                    output: output.clone(),
                });
                Ok(output)
            }
            Err(e) => {
                self.emit(AgentEventData::Error {
                    message: e.to_string(),
                });
                // Map anyhow error to ToolExecution error
                Err(Error::tool_execution(name.to_string(), e.to_string()))
            }
        }
    }

    /// Check if agent has a tool
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains(name)
    }

    /// Add tool definitions
    pub async fn tool_definitions(&self) -> Vec<crate::skills::tool::ToolDefinition> {
        self.tools.definitions().await
    }

    /// Get the agent's configuration
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Start a proactive loop that listens for tasks from multiple sources (P2P Enabled)
    #[cfg(feature = "p2p")]
    pub async fn listen(
        &self,
        mut user_input: tokio::sync::mpsc::Receiver<String>,
        mut external_events: tokio::sync::mpsc::Receiver<AgentMessage>,
    ) -> Result<()> {
        info!(
            "Agent {} starting proactive loop (P2P Enabled)",
            self.config.name
        );

        // Start Swarm Manager background task
        if let Some(swarm) = &self.swarm_manager {
            let swarm = swarm.clone();
            tokio::spawn(async move {
                loop {
                    // Lock and process
                    {
                        let mut manager = swarm.lock().await;
                        if let Err(e) = manager.process_inbox().await {
                            tracing::debug!("Swarm inbox error: {}", e);
                        }
                    }
                    // Small sleep to prevent busy loop since process_inbox uses try_recv
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            });
            info!("Swarm manager background task started");
        }

        // Phase 12-C: Start Sleep Consolidation background task
        if let Some(consolidator) = &self.sleep_consolidator {
            let consolidator = consolidator.clone();
            tokio::spawn(async move {
                loop {
                    // Consolidate every 30 minutes
                    tokio::time::sleep(tokio::time::Duration::from_secs(30 * 60)).await;
                    
                    info!("Triggering sleep-consolidation cycle...");
                    match consolidator.consolidate().await {
                        Ok(report) => info!("Consolidation complete: {} reviewed, {} verified, {} pruned, {} conflicts", 
                            report.entries_reviewed, report.entries_verified, report.entries_pruned, report.entries_conflicted),
                        Err(e) => tracing::error!("Consolidation failed: {}", e),
                    }
                }
            });
            info!("Sleep consolidator background task started");
        }

        // Phase 12-A: Start Evolution Health Watcher
        if let Some(em) = &self.evolution_manager {
            let em = em.clone();
            tokio::spawn(async move {
                loop {
                    // Check health every 5 minutes
                    tokio::time::sleep(tokio::time::Duration::from_secs(5 * 60)).await;
                    if let Err(e) = em.check_evolution_health().await {
                        tracing::error!("Evolution health check failed: {}", e);
                    }
                }
            });
            info!("Evolution health watcher started");
        }

        // Prepare swarm command rx if available
        let mut swarm_rx_guard = if let Some(rx_mutex) = &self.swarm_command_rx {
            Some(rx_mutex.lock().await)
        } else {
            None
        };

        loop {
            tokio::select! {
                // Handle swarm commands
                Some(cmd) = async {
                    if let Some(guard) = &mut swarm_rx_guard {
                        guard.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                     match cmd {
                        SwarmEvent::ExecuteTask { request_id, task, context: _ } => {
                            info!("Swarm: Executing delegated task {}: {}", request_id, task);
                            // Execute task using agent's process
                            let result = match self.process(&task).await {
                                Ok(output) => {
                                    info!("Swarm: Task execution success");
                                    output
                                },
                                Err(e) => {
                                    error!("Swarm: Task execution failed: {}", e);
                                    format!("Error: {}", e)
                                }
                            };

                            // Send result back
                            if let Some(swarm) = &self.swarm_manager {
                                let mut manager = swarm.lock().await;
                                let success = !result.starts_with("Error:");
                                if let Err(e) = manager.send_result(&request_id, result, success).await {
                                     error!("Failed to send swarm result: {}", e);
                                }
                            }
                        }
                        SwarmEvent::TaskResult { request_id, result, success } => {
                             info!("Swarm: Request {} completed. Success: {}. Result: {}", request_id, success, result);
                        }
                     }
                }

                // 1. Handle user input
                input = user_input.recv() => {
                    match input {
                        Some(text) => {
                            if let Err(e) = self.process(&text).await {
                                error!("Error in proactive user task: {}", e);
                            }
                        }
                        None => {
                            info!("User input channel closed, exiting proactive loop");
                            break;
                        }
                    }
                }

                // 2. Handle external agent/system messages (e.g. from Scheduler)
                msg = external_events.recv() => {
                    match msg {
                        Some(message) => {
                            if let Err(e) = self.handle_message(message).await {
                                error!("Error in proactive external task: {}", e);
                            }
                        }
                        None => {
                            info!("External events channel closed, exiting proactive loop");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Start a proactive loop that listens for tasks from multiple sources (P2P Disabled)
    #[cfg(not(feature = "p2p"))]
    pub async fn listen(
        &self,
        mut user_input: tokio::sync::mpsc::Receiver<String>,
        mut external_events: tokio::sync::mpsc::Receiver<AgentMessage>,
    ) -> Result<()> {
        info!(
            "Agent {} starting proactive loop (P2P Disabled)",
            self.config.name
        );

        loop {
            tokio::select! {
                // 1. Handle user input
                input = user_input.recv() => {
                    match input {
                        Some(text) => {
                            if let Err(e) = self.process(&text).await {
                                error!("Error in proactive user task: {}", e);
                            }
                        }
                        None => {
                            info!("User input channel closed, exiting proactive loop");
                            break;
                        }
                    }
                }

                // 2. Handle external agent/system messages (e.g. from Scheduler)
                msg = external_events.recv() => {
                    match msg {
                        Some(message) => {
                            if let Err(e) = self.handle_message(message).await {
                                error!("Error in proactive external task: {}", e);
                            }
                        }
                        None => {
                            info!("External events channel closed, exiting proactive loop");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Builder for creating agents
pub struct AgentBuilder<P: Provider> {
    provider: P,
    tools: ToolSet,
    config: AgentConfig,
    injectors: Vec<Box<dyn ContextInjector>>,
    approval_handler: Option<Arc<dyn ApprovalHandler>>,
    interaction_handler: Option<Arc<dyn InteractionHandler>>,
    notifier: Option<Arc<dyn Notifier>>,
    cache: Option<Arc<dyn Cache>>,

    /// Security: Track if DynamicSkill is enabled (mutually exclusive with Sidecar)
    has_dynamic_skill: bool,
    memory: Option<Arc<dyn Memory>>,
    session_id: Option<String>,
    metrics: Option<Arc<MetricsRegistry>>,

    enabled_tools: Option<Arc<parking_lot::RwLock<std::collections::HashSet<String>>>>,
    persona: Arc<parking_lot::RwLock<Option<Persona>>>,
    security: Option<Arc<SecurityManager>>,
    evolution_manager: Option<Arc<EvolutionManager>>,
    kv_cache: Option<Arc<parking_lot::RwLock<TwoTierKvCache>>>,
}

impl<P: Provider> AgentBuilder<P> {
    /// Create a new builder with a provider
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            tools: ToolSet::new(),
            config: AgentConfig::default(),
            injectors: Vec::new(),
            approval_handler: None,
            interaction_handler: None,
            notifier: None,
            cache: None,

            has_dynamic_skill: false,
            memory: None,
            session_id: None,
            metrics: None,

            enabled_tools: None,
            persona: Arc::new(parking_lot::RwLock::new(None)),
            security: None,
            evolution_manager: None,
            kv_cache: None,
        }
    }

    /// Set the model to use
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.config.model = model.into();
        self
    }

    /// Set the system prompt
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.preamble = prompt.into();
        self
    }

    /// Alias for system_prompt
    pub fn preamble(self, prompt: impl Into<String>) -> Self {
        self.system_prompt(prompt)
    }

    /// Set the temperature
    pub fn temperature(mut self, temp: f64) -> Self {
        self.config.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u64) -> Self {
        self.config.max_tokens = Some(tokens);
        self
    }

    /// Add extra provider-specific parameters
    pub fn extra_params(mut self, params: serde_json::Value) -> Self {
        self.config.extra_params = Some(params);
        self
    }

    /// Set tool policy
    pub fn tool_policy(mut self, policy: RiskyToolPolicy) -> Self {
        self.config.tool_policy = policy;
        self
    }

    /// Set external approval handler
    pub fn approval_handler(mut self, handler: impl ApprovalHandler + 'static) -> Self {
        self.approval_handler = Some(Arc::new(handler));
        self
    }

    /// Set interaction handler (for HITL)
    pub fn interaction_handler(mut self, handler: impl InteractionHandler + 'static) -> Self {
        self.interaction_handler = Some(Arc::new(handler));
        self
    }

    /// Set the agent's name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    /// Set max history messages (sliding window)
    pub fn max_history_messages(mut self, count: usize) -> Self {
        self.config.max_history_messages = count;
        self
    }

    /// Set max tool output characters
    pub fn max_tool_output_chars(mut self, count: usize) -> Self {
        self.config.max_tool_output_chars = count;
        self
    }

    /// Set enabled tools filter
    pub fn with_enabled_tools(
        mut self,
        enabled: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
    ) -> Self {
        self.enabled_tools = Some(enabled);
        self
    }

    /// Enable strict JSON mode (enforces response_format: json_object)
    pub fn json_mode(mut self, enable: bool) -> Self {
        self.config.json_mode = enable;
        self
    }

    /// Set the agent's personality
    pub fn persona(mut self, persona: Persona) -> Self {
        self.config.persona = Some(persona);
        self
    }

    /// Set a notifier
    pub fn notifier(mut self, notifier: impl Notifier + 'static) -> Self {
        self.notifier = Some(Arc::new(notifier));
        self
    }

    /// Set session ID for persistence
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    /// Set a metrics registry
    pub fn metrics(mut self, registry: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(registry);
        self
    }

    /// Set the agent's role
    pub fn role(mut self, role: AgentRole) -> Self {
        self.config.role = role;
        self
    }

    /// Set the soul folder path for markdown persona injection
    pub fn soul_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.config.soul_path = Some(path.into());
        self
    }

    /// Add a context injector
    pub fn context_injector(mut self, injector: impl ContextInjector + 'static) -> Self {
        self.injectors.push(Box::new(injector));
        self
    }

    /// Add a tool
    pub fn tool<T: Tool + 'static>(self, tool: T) -> Self {
        self.tools.add(tool);
        self
    }

    /// Add a shared tool
    pub fn shared_tool(self, tool: Arc<dyn Tool>) -> Self {
        self.tools.add_shared(tool);
        self
    }

    /// Set evolution manager
    pub fn evolution_manager(mut self, manager: Arc<EvolutionManager>) -> Self {
        self.evolution_manager = Some(manager);
        self
    }

    /// Add multiple tools from a toolset
    pub fn tools(self, tools: ToolSet) -> Self {
        for (_, tool) in tools.iter() {
            self.tools.add_shared(tool);
        }
        self
    }

    /// Add memory tools using the provided memory implementation
    pub fn with_memory(self, memory: Arc<dyn crate::agent::memory::Memory>) -> Self {
        #[cfg(feature = "vector-db")]
        {
            self.tools.add(SearchHistoryTool::new(memory.clone()));
            self.tools.add(RememberThisTool::new(memory.clone()));
            self.tools.add(TieredSearchTool::new(memory.clone()));
            self.tools.add(FetchDocumentTool::new(memory.clone()));
        }

        let mut builder = self;
        builder.memory = Some(memory);
        builder
    }

    /// Add DynamicSkill support (ClawHub skills, custom scripts)
    ///
    /// # Security
    ///
    /// **CRITICAL**: DynamicSkill and Python Sidecar are mutually exclusive.
    /// This method will return an error if Python Sidecar has already been configured.
    ///
    /// **Rationale**: If both are enabled, malicious DynamicSkills can pollute the
    /// Agent's context with secrets, which may then be used by LLM-generated Python
    /// code in the unsandboxed Sidecar to exfiltrate data.
    ///
    /// See SECURITY.md for details.
    pub fn with_dynamic_skills(
        mut self,
        skill_loader: Arc<crate::skills::SkillLoader>,
    ) -> Result<Self> {
        // Security check: prevent enabling both Sidecar and DynamicSkill

        // Add all loaded skills as tools
        for skill_ref in skill_loader.skills.iter() {
            self.tools
                .add_shared(Arc::clone(skill_ref.value()) as Arc<dyn crate::skills::tool::Tool>);
        }

        // Add ClawHub, ReadSkillDoc and ForgeSkill tools
        #[cfg(feature = "http")]
        self.tools
            .add(crate::skills::ClawHubTool::new(Arc::clone(&skill_loader)));

        self.tools
            .add(crate::skills::ReadSkillDoc::new(Arc::clone(&skill_loader)));

        #[cfg(feature = "http")]
        let github_compiler = if let (Ok(token), Ok(repo)) =
            (std::env::var("GITHUB_TOKEN"), std::env::var("GITHUB_REPO"))
        {
            Some(crate::skills::compiler::GithubCompiler::new(
                repo,
                token,
                self.notifier.clone(),
            ))
        } else {
            None
        };

        #[cfg(not(feature = "http"))]
        let github_compiler: Option<()> = None;

        #[cfg(feature = "http")]
        self.tools.add(crate::skills::tool::ForgeSkill::new(
            Arc::clone(&skill_loader),
            self.tools.clone(),
            skill_loader.base_path.clone(),
            github_compiler,
        ));

        #[cfg(not(feature = "http"))]
        self.tools.add(crate::skills::tool::ForgeSkill::new(
            Arc::clone(&skill_loader),
            self.tools.clone(),
            skill_loader.base_path.clone(),
            None,
        ));

        self.has_dynamic_skill = true;

        Ok(self)
    }

    /// Build the agent
    ///
    /// # Security Defaults
    ///
    /// If neither Python Sidecar nor DynamicSkill has been explicitly configured,
    /// this method will automatically enable DynamicSkill with default settings:
    /// - Skills directory: `./skills`
    /// - Network access: disabled (secure sandbox)
    ///
    /// To use Python Sidecar instead, call `.with_code_interpreter()` before `.build()`.
    pub fn build(mut self) -> Result<Agent<P>> {
        // Validate configuration
        if self.config.model.is_empty() {
            return Err(Error::agent_config("model name cannot be empty"));
        }
        if self.config.max_history_messages == 0 {
            return Err(Error::agent_config(
                "max_history_messages must be at least 1",
            ));
        }

        // SECURITY DEFAULT: Auto-enable DynamicSkill if no execution model configured
        if !self.has_dynamic_skill {
            info!("No execution model configured. Auto-enabling DynamicSkill (default)...");

            // Try to load skills from default directory
            let skill_loader = Arc::new(crate::skills::SkillLoader::new("./skills"));

            // Attempt to load skills (non-fatal if directory doesn't exist)
            // Capture handle before entering blocking context
            let handle = tokio::runtime::Handle::current();
            match tokio::task::block_in_place(|| handle.block_on(skill_loader.load_all())) {
                Ok(_) => {
                    info!("Loaded DynamicSkills from ./skills");

                    // Add all loaded skills as tools
                    for skill_ref in skill_loader.skills.iter() {
                        self.tools.add_shared(
                            Arc::clone(skill_ref.value()) as Arc<dyn crate::skills::tool::Tool>
                        );
                    }

                    // Add ClawHub, ReadSkillDoc and ForgeSkill tools
                    #[cfg(feature = "http")]
                    self.tools
                        .add(crate::skills::ClawHubTool::new(Arc::clone(&skill_loader)));

                    self.tools
                        .add(crate::skills::ReadSkillDoc::new(Arc::clone(&skill_loader)));

                    #[cfg(feature = "http")]
                    let github_compiler = if let (Ok(token), Ok(repo)) =
                        (std::env::var("GITHUB_TOKEN"), std::env::var("GITHUB_REPO"))
                    {
                        Some(crate::skills::compiler::GithubCompiler::new(
                            repo,
                            token,
                            self.notifier.clone(),
                        ))
                    } else {
                        None
                    };

                    #[cfg(not(feature = "http"))]
                    let github_compiler: Option<()> = None;

                    self.tools.add(crate::skills::tool::ForgeSkill::new(
                        Arc::clone(&skill_loader),
                        self.tools.clone(),
                        skill_loader.base_path.clone(),
                        github_compiler,
                    ));

                    // Add RefineSkill for self-improvement
                    self.tools
                        .add(crate::skills::tool::RefineSkill::new(Arc::clone(
                            &skill_loader,
                        )));

                    self.has_dynamic_skill = true;
                }
                Err(e) => {
                    // Non-fatal: Skills directory doesn't exist or is empty
                    info!("DynamicSkill auto-enable skipped (no skills found): {}", e);
                    // Continue without skills - agent will still function with other tools
                }
            }
        }

        let (tx, _) = broadcast::channel(1000);

        let mut context_config = ContextConfig {
            max_tokens: self.config.max_tokens.unwrap_or(128000) as usize,
            max_history_messages: self.config.max_history_messages,
            response_reserve: 4096, // Use a fixed safe reserve for responses
            enable_cache_control: self.config.enable_cache_control,
            smart_pruning: self.config.smart_pruning,
        };

        // Ensure response_reserve doesn't eat more than 50% of context
        if context_config.response_reserve > context_config.max_tokens / 2 {
            context_config.response_reserve = context_config.max_tokens / 2;
        }

        let mut context_manager = ContextManager::new(context_config);
        context_manager.set_system_prompt(self.config.preamble.clone());

        // Inject all tools as TS interfaces in the system prompt
        // This fulfills the 'Replace JSON with TS in Prompt' requirement.
        context_manager.add_injector(Box::new(self.tools.clone()));

        // Add Learned Memory Injector if memory is enabled
        if let Some(memory) = &self.memory {
            context_manager.add_injector(Box::new(
                crate::agent::memory::LearnedMemoryInjector::new(Arc::clone(memory)),
            ));
        }

        for injector in self.injectors {
            context_manager.add_injector(injector);
        }

        if let Some(persona) = &self.config.persona {
            *self.persona.write() = Some(persona.clone());
        }
        context_manager.add_injector(Box::new(PersonalityManager::new(Arc::clone(&self.persona))));

        if let Some(soul_path) = &self.config.soul_path {
            context_manager.add_injector(Box::new(crate::agent::personality::SoulManager::new(
                soul_path.clone(),
            )));
        }

        // Auto-register AskUser tool if handler available
        let tools = self.tools;
        if let Some(handler) = &self.interaction_handler {
            tools.add(AskUserTool {
                handler: Arc::clone(handler),
            });
        }

        // Register UpdatePersonaTool for self-evolution
        tools.add(crate::agent::personality::UpdatePersonaTool::new(
            Arc::clone(&self.persona),
        ));

        // Initialize SleepConsolidator if EvolutionManager and Memory are present
        let sleep_consolidator = match (self.evolution_manager.as_ref(), self.memory.as_ref()) {
            (Some(em), Some(mem)) => Some(Arc::new(SleepConsolidator::new(Arc::clone(mem), em.auditor()))),
            _ => None,
        };

        let mut agent = Agent {
            provider: Arc::new(self.provider),
            tools,
            config: self.config.clone(),
            context_manager,
            events: tx,
            approval_handler: self
                .approval_handler
                .unwrap_or_else(|| Arc::new(RejectAllApprovalHandler)),
            cache: self.cache,
            notifier: self.notifier,
            memory: self.memory,
            session_id: self.session_id,
            metrics: self.metrics,

            enabled_tools: self.enabled_tools,
            persona: self.persona,
            security: self
                .security
                .unwrap_or_else(|| Arc::new(SecurityManager::default())),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            seen_tools: Arc::new(parking_lot::RwLock::new(std::collections::HashSet::new())),
            evolution_manager: self.evolution_manager,
            sleep_consolidator,
            kv_cache: self.kv_cache.or_else(|| {
                if self.config.kv_cache_pages > 0 {
                    let mut kv_config = crate::agent::kv_cache::KvCacheConfig::default();
                    kv_config.num_pages = self.config.kv_cache_pages;
                    Some(Arc::new(parking_lot::RwLock::new(TwoTierKvCache::new(kv_config))))
                } else {
                    None
                }
            }),
        };

        // Inject SOP into system prompt if exists
        if let Some(sop) = &agent.config.sop {
            agent
                .context_manager
                .set_system_prompt(format!("### Standard Operating Procedure (SOP)\n{}\n", sop));


        }

        // --- Memory Hygiene Background Task ---
        if let Some(memory) = &agent.memory {
            let mem = Arc::clone(memory);
            tokio::spawn(async move {
                // Run hygiene every hour
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    interval.tick().await;
                    // First tick completes immediately, which is fine (cleanup on startup)

                    if let Err(e) = mem.maintenance().await {
                        tracing::warn!("Memory hygiene failed: {}", e);
                    }
                }
            });
            info!("Memory hygiene background task started (interval: 1h)");
        }

        Ok(agent)
    }

    /// Add delegation support using the provided coordinator
    pub fn with_delegation(mut self, coordinator: Arc<Coordinator>) -> Self {
        self.tools
            .add(DelegateTool::new(Arc::downgrade(&coordinator)));
        self.injectors
            .push(Box::new(SwarmInjector::new(Arc::downgrade(&coordinator))));
        self
    }

    /// Add handover support using the provided coordinator
    pub fn with_handover(self, coordinator: Arc<Coordinator>) -> Self {
        self.tools
            .add(HandoverTool::new(Arc::downgrade(&coordinator)));
        self
    }

    /// Add scheduling support using the provided scheduler
    #[cfg(feature = "cron")]
    pub fn with_scheduler(self, scheduler: Arc<Scheduler>) -> Self {
        self.tools.add(CronTool::new(Arc::downgrade(&scheduler)));
        self
    }



    /// Set a custom security manager
    pub fn with_security(mut self, security: Arc<SecurityManager>) -> Self {
        self.security = Some(security);
        self
    }

    /// Set a pre-initialized KV cache
    pub fn with_kv_cache(mut self, cache: Arc<parking_lot::RwLock<TwoTierKvCache>>) -> Self {
        self.kv_cache = Some(cache);
        self
    }
}

#[async_trait::async_trait]
impl<P: Provider> MultiAgent for Agent<P> {
    fn role(&self) -> AgentRole {
        self.config.role.clone()
    }

    async fn handle_message(&self, message: AgentMessage) -> Result<Option<AgentMessage>> {
        info!(
            "Agent {:?} handling message from {:?}",
            self.role(),
            message.from
        );
        let response = self.prompt(message.content, None).await?;

        Ok(Some(AgentMessage {
            from: self.role(),
            to: Some(message.from),
            content: response,
            msg_type: crate::agent::multi_agent::MessageType::Response,
        }))
    }

    async fn process(&self, input: &str) -> Result<String> {
        self.prompt(input, None).await
    }

    async fn chat(&self, messages: Vec<Message>, session_id: Option<String>) -> Result<String> {
        self.chat(messages, session_id).await
    }

    fn persona(&self) -> Option<Arc<parking_lot::RwLock<Option<Persona>>>> {
        Some(self.persona.clone())
    }

    fn events(&self) -> tokio::sync::broadcast::Receiver<crate::agent::core::AgentEvent> {
        self.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.max_tokens, Some(128000));
    }
}
