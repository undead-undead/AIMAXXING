use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Weak;
use brain::agent::multi_agent::{Coordinator, AgentRole};
use brain::skills::tool::{Tool, ToolDefinition};

/// Tool that allows an agent to handover the current session to another agent role
pub struct HandoverTool {
    coordinator: Weak<Coordinator>,
}

impl HandoverTool {
    /// Create a new HandoverTool
    pub fn new(coordinator: Weak<Coordinator>) -> Self {
        Self { coordinator }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct HandoverArgs {
    /// The role to handover to (e.g., "researcher", "trader")
    role: String,
    /// The current session ID (provided in system prompt)
    session_id: String,
    /// Final message or instructions for the next agent
    message: String,
}

#[async_trait]
impl Tool for HandoverTool {
    fn name(&self) -> String {
        "handover".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Handover the entire conversation to another specialized agent. Use this when you are finished with your part and another agent is better suited to continue the dialogue indefinitely.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "role": {
                        "type": "string",
                        "description": "The target role (researcher, trader, risk_analyst, strategist, assistant)",
                        "enum": ["researcher", "trader", "risk_analyst", "strategist", "assistant"]
                    },
                    "message": {
                        "type": "string",
                        "description": "A summary or message for the next agent to help them pick up where you left off"
                    }
                },
                "required": ["role", "message"]
            }),
            parameters_ts: Some("interface HandoverArgs {\n  role: 'researcher' | 'trader' | 'risk_analyst' | 'strategist' | 'assistant';\n  message: string; \n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this for a permanent transition of the active agent role in this conversation.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: HandoverArgs = serde_json::from_str(arguments)?;
        
        let coordinator = self.coordinator.upgrade().ok_or_else(|| {
            anyhow::anyhow!("Coordinator has been dropped")
        })?;

        let role_str = args.role.clone();
        let role = match role_str.as_str() {
            "researcher" => AgentRole::Researcher,
            "trader" => AgentRole::Trader,
            "risk_analyst" => AgentRole::RiskAnalyst,
            "strategist" => AgentRole::Strategist,
            "assistant" => AgentRole::Assistant,
            _ => AgentRole::Custom(role_str),
        };

        // Switch the active agent in the coordinator for this session
        coordinator.switch_session_agent(&args.session_id, role);
        
        Ok(format!("Handover successful. The next message in session {} will be handled by the {}. Summary: {}", args.session_id, args.role, args.message))
    }
}
