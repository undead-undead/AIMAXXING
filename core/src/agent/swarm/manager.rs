use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use crate::agent::swarm::manifest::AgentManifest;
use crate::agent::swarm::discovery::Discovery;
use crate::agent::swarm::protocol::SwarmMessage;
use crate::error::Result;
use tracing::{info, error, debug};

/// Events that an agent receives from the swarm
#[derive(Debug, Clone)]
pub enum SwarmEvent {
    /// Execute a task delegated by another agent
    ExecuteTask {
        request_id: String,
        task: String,
        context: String,
    },
    /// Result of a task that this agent delegated
    TaskResult {
        request_id: String,
        result: String,
        success: bool,
    },
}

/// Manages an agent's participation in the swarm
pub struct SwarmManager {
    identity: AgentManifest,
    discovery: Arc<dyn Discovery>,
    bus_tx: broadcast::Sender<SwarmMessage>,
    bus_rx: broadcast::Receiver<SwarmMessage>,
    command_tx: mpsc::Sender<SwarmEvent>,
    command_rx: Option<mpsc::Receiver<SwarmEvent>>,
}

impl SwarmManager {
    pub fn new(
        identity: AgentManifest,
        discovery: Arc<dyn Discovery>,
        bus_tx: broadcast::Sender<SwarmMessage>,
    ) -> Self {
        let bus_rx = bus_tx.subscribe();
        let (command_tx, command_rx) = mpsc::channel(100);
        Self {
            identity,
            discovery,
            bus_tx,
            bus_rx,
            command_tx,
            command_rx: Some(command_rx),
        }
    }

    /// Drain all pending messages from the bus and process them
    pub async fn process_inbox(&mut self) -> Result<()> {
        loop {
            match self.bus_rx.try_recv() {
                Ok(msg) => {
                    if let Err(e) = self.process_message(msg).await {
                        debug!("Swarm: Error processing message: {}", e);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    debug!("Swarm: Lagged behind {} messages", n);
                }
                Err(broadcast::error::TryRecvError::Closed) => {
                    debug!("Swarm: Bus closed");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Take the command receiver (can only be done once)
    pub fn take_command_receiver(&mut self) -> Option<mpsc::Receiver<SwarmEvent>> {
        self.command_rx.take()
    }

    /// Announce this agent's presence to the swarm
    pub async fn announce(&self) -> Result<()> {
        info!("Swarm: Announcing presence for {}", self.identity.id);
        let msg = SwarmMessage::Announcement(self.identity.clone());
        let _ = self.bus_tx.send(msg);
        Ok(())
    }

    /// Broadcast a task request to the swarm
    pub async fn broadcast_request(&mut self, task: &str, required_capabilities: Vec<String>) -> Result<String> {
        let msg = SwarmMessage::new_request(self.identity.id.clone(), task, required_capabilities);
        let request_id = msg.request_id().unwrap_or_default().to_string();
        
        info!("Swarm: Broadcasting task request {}", request_id);
        let _ = self.bus_tx.send(msg);
        
        Ok(request_id)
    }

    /// Send a task result back to the swarm
    pub async fn send_result(&self, request_id: &str, result: String, success: bool) -> Result<()> {
        info!("Swarm: Sending result for task {}", request_id);
        let msg = SwarmMessage::Result {
            request_id: request_id.to_string(),
            performer_id: self.identity.id.clone(),
            output: result,
            success,
        };
        let _ = self.bus_tx.send(msg);
        Ok(())
    }

    /// Process an incoming message from the swarm bus
    pub async fn process_message(&self, msg: SwarmMessage) -> Result<()> {
        match msg {
            SwarmMessage::Announcement(manifest) => {
                debug!("Swarm: Discovered peer {}", manifest.id);
                self.discovery.register(manifest).await?;
            }
            SwarmMessage::TaskRequest { request_id, requester_id, task, required_capabilities } => {
                if requester_id == self.identity.id {
                    return Ok(());
                }

                // Check if we have the required capabilities
                let has_capabilities = required_capabilities.iter().all(|cap| {
                    self.identity.capabilities.contains(cap)
                });

                if has_capabilities {
                    info!("Swarm: Bidding on task {}", request_id);
                    // In a real system, we might evaluate the task more deeply.
                    // For now, auto-bid.
                    let bid = SwarmMessage::Bid {
                        request_id: request_id.clone(),
                        bidder_id: self.identity.id.clone(),
                        bid_amount: 1.0,
                    };
                    let _ = self.bus_tx.send(bid);
                }
            }
            SwarmMessage::Bid { request_id: _, bidder_id: _, bid_amount: _ } => {
                // Requester logic for selecting a bidder would go here
            }
            SwarmMessage::TaskAssignment { request_id, assigned_to, task_context } => {
                if assigned_to == self.identity.id {
                    info!("Swarm: Assigned task {}", request_id);
                    let _ = self.command_tx.send(SwarmEvent::ExecuteTask {
                        request_id,
                        task: "Assigned Task".to_string(), // In real case, original task is needed
                        context: task_context,
                    }).await;
                }
            }
            SwarmMessage::Result { request_id, performer_id: _, output, success } => {
                // If we were the requester of this task, notify the agent
                // Note: Simplified logic, usually we would track our pending requests
                let _ = self.command_tx.send(SwarmEvent::TaskResult {
                    request_id,
                    result: output,
                    success,
                }).await;
            }
        }
        Ok(())
    }
}
