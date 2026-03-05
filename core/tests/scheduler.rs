use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

use brain::agent::message::Message;
use brain::agent::multi_agent::{AgentMessage, AgentRole, Coordinator, MultiAgent};
use brain::agent::personality::Persona;
use brain::agent::scheduler::{JobPayload, JobSchedule};
use brain::error::Result;
use brain::skills::tool::{CronTool, Tool};

struct MockAgent {
    role: AgentRole,
    called: Arc<AtomicBool>,
}

#[async_trait]
impl MultiAgent for MockAgent {
    fn role(&self) -> AgentRole {
        self.role.clone()
    }

    async fn handle_message(&self, _message: AgentMessage) -> Result<Option<AgentMessage>> {
        Ok(None)
    }

    async fn process(&self, _input: &str) -> Result<String> {
        self.called.store(true, Ordering::SeqCst);
        Ok("Processed".to_string())
    }

    async fn chat(&self, _messages: Vec<Message>, _session_id: Option<String>) -> Result<String> {
        self.called.store(true, Ordering::SeqCst);
        Ok("Processed".to_string())
    }

    fn persona(&self) -> Option<Arc<parking_lot::RwLock<Option<Persona>>>> {
        None
    }
}

#[tokio::test]
async fn test_scheduler_proactive_turn() {
    let coordinator = Arc::new(Coordinator::new());
    let scheduler = coordinator.start_scheduler().await;

    let called = Arc::new(AtomicBool::new(false));
    let agent = Arc::new(MockAgent {
        role: AgentRole::Assistant,
        called: Arc::clone(&called),
    });

    coordinator.register(agent);

    // Schedule a task for 1 second in the future
    let now = chrono::Utc::now();
    let fire_at = now + chrono::Duration::seconds(1);

    scheduler
        .add_job(
            "test_proactive".to_string(),
            JobSchedule::At { at: fire_at },
            JobPayload::AgentTurn {
                role: AgentRole::Assistant,
                prompt: "Test prompt".to_string(),
            },
        )
        .await
        .unwrap();

    // Wait for the task to fire (tick is 1s, so wait up to 3s)
    let mut success = false;
    for _ in 0..6 {
        time::sleep(Duration::from_millis(500)).await;
        if called.load(Ordering::SeqCst) {
            success = true;
            break;
        }
    }

    assert!(
        success,
        "Agent process should have been called by scheduler"
    );
}

#[tokio::test]
async fn test_cron_tool_integration() {
    let coordinator = Arc::new(Coordinator::new());
    let scheduler = coordinator.start_scheduler().await;

    let cron_tool = CronTool::new(Arc::downgrade(&scheduler));

    // 1. List jobs (should be empty or contain header)
    let list_res = cron_tool.call(r#"{"action": "list"}"#).await.unwrap();
    assert!(list_res.contains("No scheduled tasks found") || list_res.contains("ID | Name"));

    // 2. Schedule a job
    let schedule_args = r#"{
        "action": "schedule",
        "name": "tool_job",
        "schedule": { "kind": "every", "intervalSecs": 10 },
        "prompt": "Hello from tool"
    }"#;

    let sched_res = cron_tool.call(schedule_args).await.unwrap();
    assert!(sched_res.contains("Successfully scheduled"));

    // 3. List again
    let list_res2 = cron_tool.call(r#"{"action": "list"}"#).await.unwrap();
    assert!(list_res2.contains("tool_job"));
}
