use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Weak;
use brain::agent::multi_agent::{Coordinator, AgentRole};
use brain::skills::tool::{Tool, ToolDefinition};

/// Tool that allows an agent to delegate a task to another agent role
pub struct DelegateTool {
    coordinator: Weak<Coordinator>,
}

impl DelegateTool {
    /// Create a new DelegateTool with a weak reference to the coordinator
    pub fn new(coordinator: Weak<Coordinator>) -> Self {
        Self { coordinator }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct DelegateArgs {
    /// The role to delegate the task to (e.g., "researcher", "trader")
    role: String,
    /// The specific task or instruction for the sub-agent
    task: String,
}

#[async_trait]
impl Tool for DelegateTool {
    fn name(&self) -> String {
        "delegate".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Delegate a sub-task to another specialized agent role. Use this when you need research, risk analysis, or trade execution that is outside your primary scope.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "role": {
                        "type": "string",
                        "description": "The target role (researcher, trader, risk_analyst, strategist, assistant)",
                        "enum": ["researcher", "trader", "risk_analyst", "strategist", "assistant"]
                    },
                    "task": {
                        "type": "string",
                        "description": "The specific instruction for the delegated agent"
                    }
                },
                "required": ["role", "task"]
            }),
            parameters_ts: Some("interface DelegateArgs {\n  role: 'researcher' | 'trader' | 'risk_analyst' | 'strategist' | 'assistant';\n  task: string; // Instructions for the sub-agent\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this when a task can be performed more efficiently by a specialized specialist or when you need to run tasks in parallel. Specify the task clearly to ensure a good bid.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: DelegateArgs = serde_json::from_str(arguments)?;
        
        let coordinator = self.coordinator.upgrade().ok_or_else(|| {
            anyhow::anyhow!("Coordinator has been dropped")
        })?;

        let role = match args.role.as_str() {
            "researcher" => AgentRole::Researcher,
            "trader" => AgentRole::Trader,
            "risk_analyst" => AgentRole::RiskAnalyst,
            "strategist" => AgentRole::Strategist,
            "assistant" => AgentRole::Assistant,
            _ => AgentRole::Custom(args.role),
        };

        let agent = coordinator.get(&role).ok_or_else(|| {
            anyhow::anyhow!("No agent registered for role: {:?}", role)
        })?;

        // Execute the sub-agent's process
        // Note: In a real system, we might want to pass more context here
        let result = agent.process(&args.task).await?;
        
        Ok(result)
    }
}
