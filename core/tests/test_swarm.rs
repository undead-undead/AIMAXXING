use brain::prelude::*;
use brain::agent::swarm::manifest::{AgentManifest, AgentStatus};
use brain::agent::swarm::discovery::{Discovery, LocalDiscovery};
use brain::agent::multi_agent::AgentRole;
use brain::agent::swarm::protocol::SwarmMessage;

#[tokio::test]
async fn test_swarm_discovery_and_protocol() {
    // 1. Initialize Local Discovery
    let discovery = LocalDiscovery::new();
    
    // 2. Register Agent A (Researcher)
    let agent_a = AgentManifest::new("agent-a", "Alice", AgentRole::Researcher)
        .with_capability("web_search")
        .with_capability("summarize");
        
    discovery.register(agent_a.clone()).await.expect("Failed to register agent A");
    
    // 3. Register Agent B (Coder)
    let agent_b = AgentManifest::new("agent-b", "Bob", AgentRole::Custom("PythonCoder".to_string()))
        .with_capability("python")
        .with_capability("data_analysis");
        
    discovery.register(agent_b.clone()).await.expect("Failed to register agent B");
    
    // 4. Verify listing
    let agents = discovery.list().await.expect("Failed to list agents");
    assert_eq!(agents.len(), 2);
    
    // 5. Verify capability search
    let python_coders = discovery.find_by_capability("python").await.expect("Search failed");
    assert_eq!(python_coders.len(), 1);
    assert_eq!(python_coders[0].name, "Bob");
    
    // 6. Test Protocol Message Serialization
    let request = SwarmMessage::new_request(
        "agent-a", 
        "Analyze this data using pandas", 
        vec!["python".to_string(), "data_analysis".to_string()]
    );
    
    if let SwarmMessage::TaskRequest { requester_id, required_capabilities, .. } = &request {
        assert_eq!(requester_id, "agent-a");
        assert!(required_capabilities.contains(&"python".to_string()));
    } else {
        panic!("Wrong message type constructed");
    }
    
    let json = serde_json::to_string(&request).expect("Serialization failed");
    // Verify it deserializes back
    let _decoded: SwarmMessage = serde_json::from_str(&json).expect("Deserialization failed");
}
