//! Multi-agent coordination system
//!
//! Enables multiple specialized agents to work together.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use dashmap::DashMap;
use tracing::info;

use crate::agent::memory::Memory;
use crate::agent::message::Message;
use crate::agent::personality::Persona;
#[cfg(feature = "cron")]
use crate::agent::scheduler::Scheduler;
use crate::error::{Error, Result};

/// Role of an agent in a multi-agent system
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AgentRole {
    /// Research and analysis
    Researcher,
    /// Trade execution
    Trader,
    /// Risk assessment
    RiskAnalyst,
    /// Strategy planning
    Strategist,
    /// User interaction
    Assistant,
    /// Custom role
    Custom(String),
}

impl AgentRole {
    /// Get the role name
    pub fn name(&self) -> &str {
        match self {
            Self::Researcher => "researcher",
            Self::Trader => "trader",
            Self::RiskAnalyst => "risk_analyst",
            Self::Strategist => "strategist",
            Self::Assistant => "assistant",
            Self::Custom(name) => name,
        }
    }

    /// Get the role description
    pub fn description(&self) -> &str {
        match self {
            Self::Researcher => "Specialized in deep research, web search, and data analysis.",
            Self::Trader => "Specialized in executing trades and managing orders.",
            Self::RiskAnalyst => "Specialized in assessing risks and providing safety scores.",
            Self::Strategist => "Specialized in high-level planning and decision making.",
            Self::Assistant => "General-purpose assistant for user interaction and coordination.",
            Self::Custom(_) => "Specialized agent for custom tasks.",
        }
    }
}

/// Message between agents
#[derive(Debug, Clone)]
pub struct AgentMessage {
    /// Sender role
    pub from: AgentRole,
    /// Target role (None = broadcast)
    pub to: Option<AgentRole>,
    /// Message content
    pub content: String,
    /// Message type
    pub msg_type: MessageType,
}

/// Type of inter-agent message
#[derive(Debug, Clone)]
pub enum MessageType {
    /// Request for action
    Request,
    /// Response to request
    Response,
    /// Information share
    Info,
    /// Approval request
    Approval,
    /// Denial response
    Denial,
    /// Handover to another agent
    Handover,
}

/// Trait for agents that can participate in multi-agent systems
#[async_trait]
pub trait MultiAgent: Send + Sync {
    /// Get this agent's role
    fn role(&self) -> AgentRole;

    /// Handle an incoming message from another agent
    async fn handle_message(&self, message: AgentMessage) -> Result<Option<AgentMessage>>;

    /// Process a user request
    async fn process(&self, input: &str) -> Result<String>;

    /// Process a chat conversation
    async fn chat(&self, messages: Vec<Message>, session_id: Option<String>) -> Result<String>;

    /// Get the agent's persona (if supported)
    fn persona(&self) -> Option<Arc<parking_lot::RwLock<Option<Persona>>>>;

    /// Get a receiver for agent events
    fn events(&self) -> tokio::sync::broadcast::Receiver<crate::agent::core::AgentEvent>;
}

/// Coordinator for multi-agent systems
pub struct Coordinator {
    /// Registered agents
    agents: DashMap<AgentRole, Arc<dyn MultiAgent>>,
    /// Active agent per session (for persistence)
    active_agents: DashMap<String, AgentRole>,
    /// Max rounds of coordination
    max_rounds: usize,
    /// Scheduler for proactive tasks
    #[cfg(feature = "cron")]
    pub scheduler: tokio::sync::OnceCell<Arc<Scheduler>>,
    /// Shared memory for the system
    pub memory: tokio::sync::OnceCell<Arc<dyn Memory>>,
    /// Shared metrics registry
    pub metrics: Arc<crate::infra::observable::MetricsRegistry>,
    /// System-wide approval handler
    pub approval_handler: tokio::sync::OnceCell<Arc<dyn crate::agent::core::ApprovalHandler>>,
}

impl Coordinator {
    /// Create a new coordinator
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            active_agents: DashMap::new(),
            max_rounds: 10,
            #[cfg(feature = "cron")]
            scheduler: tokio::sync::OnceCell::new(),
            memory: tokio::sync::OnceCell::new(),
            metrics: Arc::new(crate::infra::observable::MetricsRegistry::new()),
            approval_handler: tokio::sync::OnceCell::new(),
        }
    }

    /// Set max coordination rounds
    pub fn with_max_rounds(mut self, rounds: usize) -> Self {
        self.max_rounds = rounds;
        self
    }

    /// Register an agent
    pub fn register(&self, agent: Arc<dyn MultiAgent>) {
        self.agents.insert(agent.role(), agent);
    }

    /// Get an agent by role
    pub fn get(&self, role: &AgentRole) -> Option<Arc<dyn MultiAgent>> {
        self.agents.get(role).map(|r| Arc::clone(&r))
    }

    /// Start the background scheduler
    #[cfg(feature = "cron")]
    pub async fn start_scheduler(self: &Arc<Self>) -> Arc<Scheduler> {
        let scheduler = self
            .scheduler
            .get_or_init(|| async {
                let store = crate::agent::scheduler::RedbCronStore::new("data/cron.redb").ok().map(|s| Box::new(s) as Box<dyn crate::agent::scheduler::CronStore>);
                let scheduler = Scheduler::new(Arc::downgrade(self), store).await;

                // Load existing jobs from store
                let _ = scheduler.load_jobs().await;

                // Link scheduler to memory if available
                if let Some(memory) = self.memory.get() {
                    memory.link_scheduler(Arc::downgrade(&scheduler));
                }

                let s_clone = Arc::clone(&scheduler);
                tokio::spawn(async move {
                    s_clone.run().await;
                });
                scheduler
            })
            .await
            .clone();

        scheduler
    }

    /// Route a message to the appropriate agent
    pub async fn route(&self, message: AgentMessage) -> Result<Option<AgentMessage>> {
        if let Some(target_role) = &message.to {
            // Directed message
            if let Some(agent) = self.get(target_role) {
                return agent.handle_message(message).await;
            } else {
                return Err(Error::AgentCommunication(format!(
                    "No agent with role: {:?}",
                    target_role
                )));
            }
        }

        // Broadcast message - send to all agents except sender
        let from_role = message.from.clone();
        let mut responses = Vec::new();

        for entry in self.agents.iter() {
            if entry.key() != &from_role {
                if let Some(response) = entry.value().handle_message(message.clone()).await? {
                    responses.push(response);
                }
            }
        }

        // Return first response for now (could aggregate in future)
        Ok(responses.into_iter().next())
    }

    /// Orchestrate a task through a dynamic workflow of agents
    pub async fn orchestrate(&self, task: &str, workflow: Vec<AgentRole>) -> Result<String> {
        if workflow.is_empty() {
            return Err(Error::AgentCoordination(
                "Workflow cannot be empty".to_string(),
            ));
        }

        let lead_role = &workflow[0];
        let lead = self.get(lead_role).ok_or_else(|| {
            Error::AgentCoordination(format!("No lead agent found for role: {:?}", lead_role))
        })?;

        // 1. Initial processing by lead agent
        let mut current_result = lead.process(task).await?;
        let mut current_role = lead_role.clone();

        // 2. Pass result through the rest of the workflow chain OR follow handovers
        let mut i = 1;
        while i < workflow.len() {
            let next_role = &workflow[i];
            if let Some(agent) = self.get(next_role) {
                let msg_type = if i == workflow.len() - 1 {
                    MessageType::Approval
                } else {
                    MessageType::Request
                };

                let message = AgentMessage {
                    from: current_role.clone(),
                    to: Some(next_role.clone()),
                    content: current_result.clone(),
                    msg_type,
                };

                if let Some(response) = agent.handle_message(message).await? {
                    // Check for Handover
                    if matches!(response.msg_type, MessageType::Handover) {
                        // Dynamic handover: the agent specifies the next role in the content or target
                        if let Some(handover_to) = response.to {
                            // If target is specified, we diverted from static workflow
                            if let Some(_handover_agent) = self.get(&handover_to) {
                                info!("Dynamic Handover from {:?} to {:?}", next_role, handover_to);
                                current_result = response.content;
                                current_role = handover_to;
                                // We don't increment i here, we stay in the loop to process the handover
                                // To prevent infinite loops, we should have a max_rounds check
                                continue;
                            }
                        }
                    }

                    // Check for strict denial/stop signal
                    if matches!(response.msg_type, MessageType::Denial) {
                        return Err(Error::AgentCoordination(format!(
                            "Agent {:?} denied processing: {}",
                            next_role, response.content
                        )));
                    }
                    current_result = response.content;
                }
                current_role = next_role.clone();
            } else {
                return Err(Error::AgentCoordination(format!(
                    "Workflow failed: Agent {:?} not found",
                    next_role
                )));
            }
            i += 1;
        }

        Ok(current_result)
    }

    /// Process a chat session, managing active agent and handovers automatically
    pub async fn chat_session(&self, session_id: &str, messages: Vec<Message>) -> Result<String> {
        // 1. Determine active agent for this session
        let active_role = self
            .active_agents
            .entry(session_id.to_string())
            .or_insert(AgentRole::Assistant) // Default to Assistant
            .clone();

        let agent = self.get(&active_role).ok_or_else(|| {
            Error::AgentCoordination(format!("Active agent {:?} not found", active_role))
        })?;

        // 2. Call agent chat
        let response = agent.chat(messages, Some(session_id.to_string())).await?;

        // 3. Detect Handover (Simple heuristic: look for "Handover to [role]" in response or use a specific signal)
        // For now, let's keep it simple and just return the response.
        // True "handover" should probably be triggered via a tool.

        Ok(response)
    }

    /// Explicitly switch the active agent for a session
    pub fn switch_session_agent(&self, session_id: &str, role: AgentRole) {
        self.active_agents.insert(session_id.to_string(), role);
    }

    /// Get list of registered agent roles
    pub fn roles(&self) -> Vec<AgentRole> {
        self.agents.iter().map(|r| r.key().clone()).collect()
    }

    /// Snapshot of all active session → agent-role mappings
    pub fn active_agents(&self) -> Vec<(String, AgentRole)> {
        self.active_agents
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }

    /// Remove a session (returns true if it existed)
    pub fn remove_session(&self, session_id: &str) -> bool {
        self.active_agents.remove(session_id).is_some()
    }



    /// Set the shared memory for the coordinator
    pub fn set_memory(&self, memory: Arc<dyn Memory>) {
        #[cfg(feature = "cron")]
        if let Some(scheduler) = self.scheduler.get() {
            memory.link_scheduler(Arc::downgrade(scheduler));
        }
        let _ = self.memory.set(memory);
    }
}

/// A context injector that informs an agent about other agents in the swarm
pub struct SwarmInjector {
    coordinator: Weak<Coordinator>,
}

impl SwarmInjector {
    /// Create a new SwarmInjector
    pub fn new(coordinator: Weak<Coordinator>) -> Self {
        Self { coordinator }
    }
}

#[async_trait]
impl crate::agent::context::ContextInjector for SwarmInjector {
    async fn inject(&self, _history: &[Message]) -> Result<Vec<Message>> {
        if let Some(coordinator) = self.coordinator.upgrade() {
            let mut info = String::from("### Available Swarm Agents\n");
            info.push_str("You are part of a multi-agent swarm. You can delegate tasks to these specialized agents using the `delegate` tool:\n\n");

            for entry in coordinator.agents.iter() {
                let role = entry.key();
                info.push_str(&format!("- **{}**: {}\n", role.name(), role.description()));
            }

            return Ok(vec![Message::system(info)]);
        }
        Ok(Vec::new())
    }
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAgent {
        role: AgentRole,
        response: String,
    }

    #[async_trait]
    impl MultiAgent for MockAgent {
        fn role(&self) -> AgentRole {
            self.role.clone()
        }

        async fn handle_message(&self, _message: AgentMessage) -> Result<Option<AgentMessage>> {
            Ok(Some(AgentMessage {
                from: self.role.clone(),
                to: None,
                content: self.response.clone(),
                msg_type: MessageType::Response,
            }))
        }

        async fn process(&self, _input: &str) -> Result<String> {
            Ok(self.response.clone())
        }

        async fn chat(
            &self,
            _messages: Vec<Message>,
            _session_id: Option<String>,
        ) -> Result<String> {
            Ok(self.response.clone())
        }

        fn persona(&self) -> Option<Arc<parking_lot::RwLock<Option<Persona>>>> {
            None
        }

        fn events(&self) -> tokio::sync::broadcast::Receiver<crate::agent::core::AgentEvent> {
            let (_, rx) = tokio::sync::broadcast::channel(1);
            rx
        }
    }

    #[tokio::test]
    async fn test_coordinator() {
        let coordinator = Coordinator::new();

        coordinator.register(Arc::new(MockAgent {
            role: AgentRole::Researcher,
            response: "Research complete".to_string(),
        }));

        coordinator.register(Arc::new(MockAgent {
            role: AgentRole::Trader,
            response: "Trade executed".to_string(),
        }));

        assert_eq!(coordinator.roles().len(), 2);
    }
}
