//! Hook Engine
//!
//! A lightweight, composable hook/middleware system for Agent execution.
//!
//! Supports:
//! - Sequential and parallel hook execution
//! - Before/After/Error timing
//! - Shell script hooks and Rust fn hooks
//! - Priority ordering
//! - Conditional activation (via `when` predicate)

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// When the hook should fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HookTiming {
    /// Before the LLM call
    BeforeLlm,
    /// After the LLM response
    AfterLlm,
    /// Before a tool call
    BeforeToolCall,
    /// After a tool call
    AfterToolCall,
    /// On error
    OnError,
    /// Before sending the final response to the user
    BeforeResponse,
}

impl std::fmt::Display for HookTiming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BeforeLlm => write!(f, "before_llm"),
            Self::AfterLlm => write!(f, "after_llm"),
            Self::BeforeToolCall => write!(f, "before_tool_call"),
            Self::AfterToolCall => write!(f, "after_tool_call"),
            Self::OnError => write!(f, "on_error"),
            Self::BeforeResponse => write!(f, "before_response"),
        }
    }
}

/// Event data passed to hooks.
#[derive(Debug, Clone)]
pub struct HookEvent {
    /// The timing when this hook is executed.
    pub timing: HookTiming,
    /// User message or context.
    pub user_input: Option<String>,
    /// LLM response text (for After hooks).
    pub llm_response: Option<String>,
    /// Tool name (for tool hooks).
    pub tool_name: Option<String>,
    /// Tool arguments (for tool hooks).
    pub tool_args: Option<String>,
    /// Tool result (for after-tool hooks).
    pub tool_result: Option<String>,
    /// Error message (for error hooks).
    pub error: Option<String>,
    /// Arbitrary metadata.
    pub metadata: std::collections::HashMap<String, String>,
}

impl HookEvent {
    /// Create a minimal event.
    pub fn new(timing: HookTiming) -> Self {
        Self {
            timing,
            user_input: None,
            llm_response: None,
            tool_name: None,
            tool_args: None,
            tool_result: None,
            error: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Builder-style setter for user input.
    pub fn with_user_input(mut self, input: impl Into<String>) -> Self {
        self.user_input = Some(input.into());
        self
    }

    /// Builder-style setter for LLM response.
    pub fn with_llm_response(mut self, response: impl Into<String>) -> Self {
        self.llm_response = Some(response.into());
        self
    }

    /// Builder-style setter for tool call.
    pub fn with_tool(mut self, name: impl Into<String>, args: impl Into<String>) -> Self {
        self.tool_name = Some(name.into());
        self.tool_args = Some(args.into());
        self
    }

    /// Builder-style setter for tool result.
    pub fn with_tool_result(mut self, result: impl Into<String>) -> Self {
        self.tool_result = Some(result.into());
        self
    }

    /// Builder-style setter for error.
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// Result from a hook execution.
#[derive(Debug, Clone)]
pub enum HookResult {
    /// Continue processing normally.
    Continue,
    /// Modify the input/output and continue.
    Modify(String),
    /// Abort processing with an error message.
    Abort(String),
    /// Skip the current operation (e.g., skip a tool call).
    Skip,
}

/// Trait for implementing hooks.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Unique name of this hook.
    fn name(&self) -> &str;

    /// Timings this hook should fire on.
    fn timings(&self) -> Vec<HookTiming>;

    /// Priority (lower = runs first). Default: 100.
    fn priority(&self) -> u32 {
        100
    }

    /// Optional predicate — return `false` to skip this hook for a given event.
    fn should_run(&self, _event: &HookEvent) -> bool {
        true
    }

    /// Execute the hook.
    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookResult>;
}

/// Shell script hook — executes a shell command.
pub struct ShellHook {
    name: String,
    command: String,
    timings: Vec<HookTiming>,
    priority: u32,
    timeout_secs: u64,
}

impl ShellHook {
    pub fn new(
        name: impl Into<String>,
        command: impl Into<String>,
        timings: Vec<HookTiming>,
    ) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            timings,
            priority: 100,
            timeout_secs: 10,
        }
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
}

#[async_trait]
impl Hook for ShellHook {
    fn name(&self) -> &str {
        &self.name
    }

    fn timings(&self) -> Vec<HookTiming> {
        self.timings.clone()
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookResult> {
        use tokio::process::Command;

        #[cfg(target_os = "windows")]
        let mut cmd = Command::new("cmd");
        #[cfg(target_os = "windows")]
        cmd.arg("/C").arg(&self.command);

        #[cfg(not(target_os = "windows"))]
        let mut cmd = Command::new("sh");
        #[cfg(not(target_os = "windows"))]
        cmd.arg("-c").arg(&self.command);

        // Pass event data as environment variables
        cmd.env("HOOK_TIMING", event.timing.to_string());
        if let Some(ref input) = event.user_input {
            cmd.env("HOOK_USER_INPUT", input);
        }
        if let Some(ref response) = event.llm_response {
            cmd.env("HOOK_LLM_RESPONSE", response);
        }
        if let Some(ref tool) = event.tool_name {
            cmd.env("HOOK_TOOL_NAME", tool);
        }
        if let Some(ref args) = event.tool_args {
            cmd.env("HOOK_TOOL_ARGS", args);
        }
        if let Some(ref result) = event.tool_result {
            cmd.env("HOOK_TOOL_RESULT", result);
        }
        if let Some(ref error) = event.error {
            cmd.env("HOOK_ERROR", error);
        }

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Hook '{}' timed out", self.name))?
        .map_err(|e| anyhow::anyhow!("Hook '{}' failed: {}", self.name, e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if stdout.is_empty() {
                Ok(HookResult::Continue)
            } else {
                Ok(HookResult::Modify(stdout))
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!(
                hook = %self.name,
                exit_code = output.status.code(),
                stderr = %stderr,
                "Shell hook failed"
            );
            // Non-zero exit = abort with error
            Ok(HookResult::Abort(format!(
                "Hook '{}' failed: {}",
                self.name, stderr
            )))
        }
    }
}

/// Closure-based hook for inline Rust hooks.
pub struct FnHook<F>
where
    F: Fn(&HookEvent) -> HookResult + Send + Sync,
{
    name: String,
    timings: Vec<HookTiming>,
    priority: u32,
    handler: F,
}

impl<F> FnHook<F>
where
    F: Fn(&HookEvent) -> HookResult + Send + Sync,
{
    pub fn new(
        name: impl Into<String>,
        timings: Vec<HookTiming>,
        handler: F,
    ) -> Self {
        Self {
            name: name.into(),
            timings,
            priority: 100,
            handler,
        }
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

#[async_trait]
impl<F> Hook for FnHook<F>
where
    F: Fn(&HookEvent) -> HookResult + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn timings(&self) -> Vec<HookTiming> {
        self.timings.clone()
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookResult> {
        Ok((self.handler)(event))
    }
}

/// Hook execution engine.
///
/// Manages registered hooks and dispatches events to them in priority order.
/// Zero-cost when no hooks are registered (the `fire` method returns immediately).
pub struct HookEngine {
    /// Hooks organized by timing, sorted by priority.
    hooks: BTreeMap<HookTiming, Vec<Arc<dyn Hook>>>,
}

impl Default for HookEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl HookEngine {
    /// Create a new empty engine.
    pub fn new() -> Self {
        Self {
            hooks: BTreeMap::new(),
        }
    }

    /// Register a hook.
    pub fn register(&mut self, hook: Arc<dyn Hook>) {
        for timing in hook.timings() {
            let entry = self.hooks.entry(timing).or_default();
            entry.push(Arc::clone(&hook));
            // Sort by priority (stable sort preserves insertion order for equal priorities)
            entry.sort_by_key(|h| h.priority());
        }
        info!(hook = hook.name(), "Hook registered");
    }

    /// Fire an event and run all matching hooks sequentially.
    ///
    /// Returns the final HookResult. If any hook returns Abort, execution stops.
    /// If a hook returns Modify, the modified value is passed to subsequent hooks.
    pub async fn fire(&self, event: &HookEvent) -> HookResult {
        let hooks = match self.hooks.get(&event.timing) {
            Some(h) => h,
            None => return HookResult::Continue, // Zero-cost path: no hooks registered
        };

        let mut current_result = HookResult::Continue;

        for hook in hooks {
            // Check predicate
            if !hook.should_run(event) {
                debug!(hook = hook.name(), "Hook skipped (predicate)");
                continue;
            }

            match hook.execute(event).await {
                Ok(HookResult::Continue) => {
                    // Keep going
                }
                Ok(HookResult::Modify(value)) => {
                    debug!(hook = hook.name(), "Hook modified output");
                    current_result = HookResult::Modify(value);
                }
                Ok(HookResult::Abort(reason)) => {
                    warn!(hook = hook.name(), reason = %reason, "Hook aborted execution");
                    return HookResult::Abort(reason);
                }
                Ok(HookResult::Skip) => {
                    debug!(hook = hook.name(), "Hook skipped operation");
                    return HookResult::Skip;
                }
                Err(e) => {
                    warn!(hook = hook.name(), error = %e, "Hook execution error (continuing)");
                    // Hook errors don't stop execution by default
                }
            }
        }

        current_result
    }

    /// Check if any hooks are registered for a given timing.
    pub fn has_hooks(&self, timing: &HookTiming) -> bool {
        self.hooks.get(timing).is_some_and(|h| !h.is_empty())
    }

    /// Get the total number of registered hooks.
    pub fn hook_count(&self) -> usize {
        self.hooks.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hook_engine_empty() {
        let engine = HookEngine::new();
        let event = HookEvent::new(HookTiming::BeforeLlm);
        let result = engine.fire(&event).await;
        assert!(matches!(result, HookResult::Continue));
    }

    #[tokio::test]
    async fn test_hook_fn_modify() {
        let mut engine = HookEngine::new();

        let hook = FnHook::new(
            "uppercase",
            vec![HookTiming::AfterLlm],
            |event| {
                if let Some(ref response) = event.llm_response {
                    HookResult::Modify(response.to_uppercase())
                } else {
                    HookResult::Continue
                }
            },
        );

        engine.register(Arc::new(hook));

        let event = HookEvent::new(HookTiming::AfterLlm)
            .with_llm_response("hello world");

        let result = engine.fire(&event).await;
        match result {
            HookResult::Modify(v) => assert_eq!(v, "HELLO WORLD"),
            _ => panic!("Expected Modify"),
        }
    }

    #[tokio::test]
    async fn test_hook_priority_ordering() {
        let mut engine = HookEngine::new();

        let hook_low = FnHook::new(
            "low_priority",
            vec![HookTiming::BeforeLlm],
            |_| HookResult::Modify("low".to_string()),
        )
        .with_priority(200);

        let hook_high = FnHook::new(
            "high_priority",
            vec![HookTiming::BeforeLlm],
            |_| HookResult::Modify("high".to_string()),
        )
        .with_priority(50);

        engine.register(Arc::new(hook_low));
        engine.register(Arc::new(hook_high));

        let event = HookEvent::new(HookTiming::BeforeLlm);
        let result = engine.fire(&event).await;

        // Last hook to run sets the final value
        match result {
            HookResult::Modify(v) => assert_eq!(v, "low"),
            _ => panic!("Expected Modify"),
        }
    }

    #[tokio::test]
    async fn test_hook_abort_stops_chain() {
        let mut engine = HookEngine::new();

        let hook1 = FnHook::new(
            "aborter",
            vec![HookTiming::BeforeToolCall],
            |event| {
                if event.tool_name.as_deref() == Some("dangerous_tool") {
                    HookResult::Abort("Dangerous tool blocked by policy".to_string())
                } else {
                    HookResult::Continue
                }
            },
        )
        .with_priority(10);

        let hook2 = FnHook::new(
            "should_not_run",
            vec![HookTiming::BeforeToolCall],
            |_| HookResult::Modify("ran".to_string()),
        )
        .with_priority(20);

        engine.register(Arc::new(hook1));
        engine.register(Arc::new(hook2));

        let event = HookEvent::new(HookTiming::BeforeToolCall)
            .with_tool("dangerous_tool", "{}");

        let result = engine.fire(&event).await;
        assert!(matches!(result, HookResult::Abort(_)));
    }
}
