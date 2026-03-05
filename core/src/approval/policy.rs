//! Approval Policy Engine
//!
//! Decides whether a tool call should be:
//! - Auto-approved (low risk, verified tools)
//! - Denied (blocked tools)
//! - Require user confirmation (high risk, unverified tools)
//!
//! Policies can be defined per-tool, by pattern, or by category.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// The decision made by the policy engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// The tool call is automatically approved.
    AutoApprove,
    /// The tool call is denied.
    Deny { reason: String },
    /// The tool call requires user confirmation.
    RequireConfirmation { message: String },
}

/// Policy for a specific tool or pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicy {
    /// Tool name pattern (exact match or glob with `*`)
    pub pattern: String,
    /// The policy action
    pub action: PolicyAction,
    /// Optional reason/description
    pub reason: Option<String>,
}

/// What to do when a tool matches a policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    /// Always allow without asking.
    Allow,
    /// Always deny.
    Deny,
    /// Ask the user for confirmation.
    Ask,
}

/// Overall approval policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    /// Default action for tools without a specific policy.
    pub default_action: PolicyAction,
    /// Per-tool policies (evaluated in order, first match wins).
    pub tool_policies: Vec<ToolPolicy>,
    /// Whether verified (built-in) tools are auto-approved.
    pub auto_approve_verified: bool,
    /// Maximum argument size to show in confirmation prompts (chars).
    pub max_args_preview: usize,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            default_action: PolicyAction::Allow,
            tool_policies: Vec::new(),
            auto_approve_verified: true,
            max_args_preview: 500,
        }
    }
}

/// The policy engine that evaluates tool calls.
pub struct PolicyEngine {
    policy: ApprovalPolicy,
    /// Per-session overrides (e.g., "always allow X for this session")
    session_overrides: HashMap<String, ApprovalDecision>,
}

impl PolicyEngine {
    /// Create a new policy engine with the given configuration.
    pub fn new(policy: ApprovalPolicy) -> Self {
        Self {
            policy,
            session_overrides: HashMap::new(),
        }
    }

    /// Create a permissive engine (auto-approves everything).
    pub fn permissive() -> Self {
        Self::new(ApprovalPolicy::default())
    }

    /// Create a strict engine (asks for confirmation by default).
    pub fn strict() -> Self {
        Self::new(ApprovalPolicy {
            default_action: PolicyAction::Ask,
            auto_approve_verified: true,
            ..Default::default()
        })
    }

    /// Add a session override for a specific tool.
    pub fn override_tool(&mut self, tool_name: &str, decision: ApprovalDecision) {
        self.session_overrides
            .insert(tool_name.to_string(), decision);
    }

    /// Evaluate whether a tool call should be approved.
    pub fn evaluate(
        &self,
        tool_name: &str,
        arguments: &str,
        is_verified: bool,
    ) -> ApprovalDecision {
        // Check session overrides first
        if let Some(override_decision) = self.session_overrides.get(tool_name) {
            debug!(
                tool = tool_name,
                decision = ?override_decision,
                "Using session override"
            );
            return override_decision.clone();
        }

        // Auto-approve verified tools if configured
        if is_verified && self.policy.auto_approve_verified {
            return ApprovalDecision::AutoApprove;
        }

        // Check specific tool policies (first match wins)
        for tp in &self.policy.tool_policies {
            if matches_pattern(&tp.pattern, tool_name) {
                return match &tp.action {
                    PolicyAction::Allow => ApprovalDecision::AutoApprove,
                    PolicyAction::Deny => ApprovalDecision::Deny {
                        reason: tp
                            .reason
                            .clone()
                            .unwrap_or_else(|| format!("Tool '{}' is denied by policy", tool_name)),
                    },
                    PolicyAction::Ask => {
                        let args_preview = if arguments.len() > self.policy.max_args_preview {
                            format!(
                                "{}... ({} chars total)",
                                &arguments[..self.policy.max_args_preview],
                                arguments.len()
                            )
                        } else {
                            arguments.to_string()
                        };
                        ApprovalDecision::RequireConfirmation {
                            message: format!(
                                "Tool '{}' wants to execute with arguments:\n{}\n{}",
                                tool_name,
                                args_preview,
                                tp.reason.as_deref().unwrap_or("Approve this action?")
                            ),
                        }
                    }
                };
            }
        }

        // Apply default action
        match &self.policy.default_action {
            PolicyAction::Allow => ApprovalDecision::AutoApprove,
            PolicyAction::Deny => ApprovalDecision::Deny {
                reason: "Denied by default policy".to_string(),
            },
            PolicyAction::Ask => {
                let args_preview = if arguments.len() > self.policy.max_args_preview {
                    format!("{}...", &arguments[..self.policy.max_args_preview])
                } else {
                    arguments.to_string()
                };
                ApprovalDecision::RequireConfirmation {
                    message: format!(
                        "Allow tool '{}' to execute?\nArguments: {}",
                        tool_name, args_preview
                    ),
                }
            }
        }
    }
}

/// Simple glob-style pattern matching.
/// Supports `*` as wildcard for any sequence of characters.
fn matches_pattern(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == name;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 2 {
        // "prefix*" or "*suffix" or "prefix*suffix"
        let prefix = parts[0];
        let suffix = parts[1];
        return name.starts_with(prefix) && name.ends_with(suffix);
    }

    // For complex patterns, fall back to simple comparison
    pattern == name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissive_engine() {
        let engine = PolicyEngine::permissive();
        let decision = engine.evaluate("any_tool", "{}", false);
        assert!(matches!(decision, ApprovalDecision::AutoApprove));
    }

    #[test]
    fn test_strict_engine_verified() {
        let engine = PolicyEngine::strict();
        // Verified tools are auto-approved even in strict mode
        let decision = engine.evaluate("web_search", "{}", true);
        assert!(matches!(decision, ApprovalDecision::AutoApprove));
    }

    #[test]
    fn test_strict_engine_unverified() {
        let engine = PolicyEngine::strict();
        let decision = engine.evaluate("unknown_tool", "{}", false);
        assert!(matches!(
            decision,
            ApprovalDecision::RequireConfirmation { .. }
        ));
    }

    #[test]
    fn test_deny_policy() {
        let policy = ApprovalPolicy {
            tool_policies: vec![ToolPolicy {
                pattern: "exec_*".to_string(),
                action: PolicyAction::Deny,
                reason: Some("Execution tools are blocked".to_string()),
            }],
            ..Default::default()
        };

        let engine = PolicyEngine::new(policy);
        let decision = engine.evaluate("exec_command", "{}", false);
        assert!(matches!(decision, ApprovalDecision::Deny { .. }));
    }

    #[test]
    fn test_pattern_matching() {
        assert!(matches_pattern("*", "anything"));
        assert!(matches_pattern("exec_*", "exec_command"));
        assert!(matches_pattern("*_tool", "web_search_tool"));
        assert!(!matches_pattern("exec_*", "web_search"));
        assert!(matches_pattern("exact_name", "exact_name"));
        assert!(!matches_pattern("exact_name", "other_name"));
    }

    #[test]
    fn test_session_override() {
        let mut engine = PolicyEngine::strict();
        engine.override_tool("web_fetch", ApprovalDecision::AutoApprove);

        let decision = engine.evaluate("web_fetch", "{}", false);
        assert!(matches!(decision, ApprovalDecision::AutoApprove));
    }
}
