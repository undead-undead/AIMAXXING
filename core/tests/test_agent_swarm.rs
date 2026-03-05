use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use async_trait::async_trait;
use brain::prelude::*;
use brain::agent::swarm::manifest::{AgentManifest, AgentStatus};
use brain::agent::swarm::discovery::LocalDiscovery;
use brain::agent::swarm::manager::SwarmManager;
use brain::agent::swarm::protocol::SwarmMessage;
use brain::agent::multi_agent::AgentRole;
use brain::agent::provider::{Provider, ChatRequest};
use brain::agent::streaming::StreamingResponse;
use brain::error::Result;

// Local MockProvider implementation
struct MockProvider;

impl MockProvider {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(&self, _request: ChatRequest) -> Result<StreamingResponse> {
        // Return mostly empty response since we are testing swarm background logic, not chat
        // We can simulate a "Thinking" message to keep the agent happy
        use futures::stream;
        let s = stream::iter(vec![Ok(brain::agent::streaming::StreamingChoice::Message("Thinking...".to_string()))]);
        Ok(StreamingResponse::from_stream(s))
    }

    fn name(&self) -> &'static str {
        "mock-provider"
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_agent_swarm_integration() {
    // 1. Setup Swarm Infrastructure
    let discovery = Arc::new(LocalDiscovery::new());
    let (bus_tx, mut bus_rx) = broadcast::channel(100);
    
    // 2. Setup Agent Identity
    let manifest = AgentManifest::new("agent-test", "TestAgent", AgentRole::Researcher)
        .with_capability("test_skill");
        
    // 3. Create SwarmManager
    // 3. Create SwarmManager
    let mut manager = SwarmManager::new(
        manifest,
        discovery.clone(),
        bus_tx.clone(),
    );
    let cmd_rx = manager.take_command_receiver().expect("Failed to get command receiver");
    let swarm_manager = Arc::new(Mutex::new(manager));
    
    // 4. Create Agent with Swarm
    let provider = MockProvider::new();
    let agent = Agent::builder(provider)
        .with_swarm(swarm_manager.clone(), cmd_rx)
        .build()
        .expect("Failed to build agent");
        
    // 5. Start Agent Listening Loop (in background)
    let (_user_tx, _user_rx) = tokio::sync::mpsc::channel::<String>(10);
    let (_event_tx, event_rx) = tokio::sync::mpsc::channel(10);
    
    let agent = Arc::new(agent);
    let agent_clone = agent.clone();
    
    // We purposefully create a user_rx here to pass to listen, 
    // but the test uses _user_rx (dropped?) No, needs to be kept alive or passed.
    // listen takes receiver.
    let (_tx_dummy, rx_dummy) = tokio::sync::mpsc::channel(10);

    tokio::spawn(async move {
        // Use rx_dummy for user input (will never receive anything)
        agent_clone.listen(rx_dummy, event_rx).await.unwrap();
    });
    
    // 6. Broadcast a Task Request verify that Agent Bids
    // We need to wait a bit for the background task to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    let request = SwarmMessage::new_request(
        "requester-1", 
        "Do some test work", 
        vec!["test_skill".to_string()]
    );
    
    bus_tx.send(request.clone()).expect("Failed to send request");
    
    // 7. Listen for Bid
    // We expect a Bid message on the bus
    
    // We need to loop because we might receive our own request or other messages
    // Note: SwarmManager logic: "Ignore own requests". 
    // The request comes from "requester-1", agent is "agent-test".
    
    let timeout = tokio::time::timeout(tokio::time::Duration::from_secs(2), async {
        loop {
            match bus_rx.recv().await {
                Ok(msg) => {
                    match msg {
                        SwarmMessage::Bid { bidder_id, .. } => {
                            if bidder_id == "agent-test" {
                                return true;
                            }
                        }
                        _ => {} // Ignore other messages
                    }
                }
                Err(_) => break,
            }
        }
        false
    }).await;
    
    assert!(timeout.is_ok(), "Timed out waiting for Bid");
    assert!(timeout.unwrap(), "Did not receive Bid from agent");
    
    // 8. Send Task Assignment
    let assignment = SwarmMessage::TaskAssignment {
        request_id: request.request_id().unwrap().to_string(), // we need request_id
        assigned_to: "agent-test".to_string(),
        task_context: "Do the work".to_string(),
    };
    bus_tx.send(assignment).expect("Failed to send assignment");
    
    // 9. Listen for Result
    let timeout_result = tokio::time::timeout(tokio::time::Duration::from_secs(2), async {
        loop {
            match bus_rx.recv().await {
                Ok(msg) => {
                    match msg {
                        SwarmMessage::Result { success, .. } => {
                            if success {
                                return true;
                            }
                        }
                        _ => {} 
                    }
                }
                Err(_) => break,
            }
        }
        false
    }).await;
    
    assert!(timeout_result.is_ok(), "Timed out waiting for Result");
    assert!(timeout_result.unwrap(), "Did not receive Result from agent");
}
