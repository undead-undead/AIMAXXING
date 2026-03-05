//! Tool for agents to schedule future and periodic tasks

use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Weak;
use uuid::Uuid;
use crate::agent::multi_agent::AgentRole;
use crate::agent::scheduler::{Scheduler, JobSchedule, JobPayload};
use crate::skills::tool::{Tool, ToolDefinition};
use crate::error::Error;

/// Tool for managing scheduled tasks
pub struct CronTool {
    /// Weak reference to scheduler for operations
    scheduler: Weak<Scheduler>,
}

impl CronTool {
    /// Create a new cron tool
    pub fn new(scheduler: Weak<Scheduler>) -> Self {
        Self { scheduler }
    }
}

#[derive(Debug, Deserialize)]
struct CronArgs {
    action: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    schedule: Option<serde_json::Value>,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    id: Option<String>,
}

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> String {
        "cron".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "cron".to_string(),
            description: "Manage scheduled and periodic tasks (actions: schedule, list, cancel).".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["schedule", "list", "cancel"],
                        "description": "Action to perform"
                    },
                    "name": {
                        "type": "string",
                        "description": "Name of the task"
                    },
                    "schedule": {
                        "type": "object",
                        "description": "Schedule definition"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Prompt to run"
                    },
                    "id": {
                        "type": "string",
                        "description": "ID of the task to cancel"
                    }
                },
                "required": ["action"]
            }),
            parameters_ts: Some("type Schedule = \n  | { kind: 'at', at: string } // ISO8601 timestamp\n  | { kind: 'every', intervalSecs: number };\n\ninterface CronArgs {\n  action: 'schedule' | 'list' | 'cancel';\n  name?: string;\n  schedule?: Schedule;\n  prompt?: string;\n  id?: string; // For cancel action\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this to schedule recurring tasks or reminders. Ensure the action description is clear so you understand it when it triggers in the future.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: CronArgs = serde_json::from_str(arguments)
            .map_err(|e| anyhow::Error::from(Error::tool_execution("cron", format!("Invalid arguments: {}", e))))?;
            
        let scheduler = self.scheduler.upgrade()
            .ok_or_else(|| anyhow::Error::from(Error::tool_execution("cron", "Scheduler not available")))?;

        match args.action.as_str() {
            "schedule" => {
                let schedule_json = args.schedule.ok_or_else(|| anyhow::Error::from(Error::tool_execution("cron", "schedule required for schedule action")))?;
                let schedule: JobSchedule = serde_json::from_value(schedule_json)
                    .map_err(|e| anyhow::Error::from(Error::tool_execution("cron", format!("Invalid schedule: {}", e))))?;
                
                // For simplicity, we assume the task is for the agent role calling it.
                // In a real system we might want to pass the role explicitly.
                // Here we just use Assistant as default if we don't know the role.
                let id = scheduler.add_job(
                    args.name,
                    schedule,
                    JobPayload::AgentTurn {
                        role: AgentRole::Assistant, // Defaulting to Assistant for now
                        prompt: args.prompt,
                    },
                ).await.map_err(|e| anyhow::Error::from(e))?;
                Ok(format!("Successfully scheduled task with ID: {}", id))
            },
            "list" => {
                let jobs = scheduler.list_jobs();
                if jobs.is_empty() {
                    return Ok("No scheduled tasks found.".to_string());
                }

                let mut table = crate::infra::format::MarkdownTable::new(vec!["ID", "Name", "Schedule", "Enabled"]);
                for job in jobs {
                    let schedule_str = match job.schedule {
                        JobSchedule::At { at } => format!("At {}", at),
                        JobSchedule::Every { interval_secs } => format!("Every {}s", interval_secs),
                        JobSchedule::Cron { expr } => format!("Cron: {}", expr),
                    };
                    table.add_row(vec![
                        job.id.to_string(),
                        job.name,
                        schedule_str,
                        job.enabled.to_string(),
                    ]);
                }
                Ok(table.render())
            },
            "cancel" => {
                let id_str = args.id.ok_or_else(|| anyhow::Error::from(Error::tool_execution("cron", "id required for cancel action")))?;
                let id = Uuid::parse_str(&id_str)
                    .map_err(|e| anyhow::Error::from(Error::tool_execution("cron", format!("Invalid ID format: {}", e))))?;
                
                if scheduler.remove_job(id).await.map_err(|e| anyhow::Error::from(e))? {
                    Ok(format!("Successfully canceled task {}", id))
                } else {
                    Ok(format!("Task {} not found", id))
                }
            },
            _ => Err(anyhow::Error::from(Error::tool_execution("cron", format!("Unknown action: {}", args.action)))),
        }
    }
}
