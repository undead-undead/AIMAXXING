use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use clap::{Parser, Subcommand};
use tracing::{info, warn, error};
use async_trait::async_trait;
use brain::prelude::*;
use brain::agent::swarm::manifest::{AgentManifest, AgentStatus};
use brain::agent::swarm::discovery::LocalDiscovery;
use brain::agent::swarm::discovery::Discovery; // Trait
use brain::agent::swarm::manager::{SwarmManager, SwarmEvent};
use brain::agent::swarm::protocol::SwarmMessage;
use brain::agent::multi_agent::AgentRole;
use brain::agent::provider::{Provider, ChatRequest};
use brain::agent::streaming::{StreamingResponse, StreamingChoice};
use brain::error::Result;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::collections::HashMap;

// --- Mock Provider ---
struct MockProvider {
    name: String,
}

impl MockProvider {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(&self, request: ChatRequest) -> Result<StreamingResponse> {
        let content = if let Some(last) = request.messages.last() {
            format!("Mock Response from {} to: {}", self.name, last.content.as_text())
        } else {
            "Empty request".to_string()
        };
        
        use futures::stream;
        let s = stream::iter(vec![Ok(StreamingChoice::Message(content))]);
        Ok(StreamingResponse::from_stream(s))
    }

    fn name(&self) -> &'static str {
        "mock-provider"
    }
}

// --- CLI ---
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand, Debug)]
enum Mode {
    /// Start a worker agent that accepts tasks
    Worker {
        #[arg(short, long, default_value = "worker-1")]
        name: String,
        #[arg(short, long, default_value_t = 8081)]
        port: u16,
    },
    /// Start a requester agent that delegates tasks
    Requester {
        #[arg(short, long, default_value = "requester-1")]
        name: String,
        #[arg(short, long, default_value_t = 8082)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    run(args).await
}

async fn run(args: Args) -> Result<()> {
    match args.mode {
        Mode::Worker { name, port } => run_worker(name, port).await,
        Mode::Requester { name, port } => run_requester(name, port).await,
    }
}

// --- Network Transport Logic ---

async fn start_transport(
    port: u16,
    bus_tx: broadcast::Sender<SwarmMessage>,
    my_id: String,
    discovery: Arc<dyn Discovery>,
) -> Result<()> {
    // 1. Start TCP Listener (Inbound)
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await
        .map_err(|e| brain::error::Error::Internal(e.to_string()))?;
    
    let bus_clone = bus_tx.clone();
    tokio::spawn(async move {
        info!("Transport: Listening on port {}", port);
        loop {
            if let Ok((socket, addr)) = listener.accept().await {
                let bus = bus_clone.clone();
                tokio::spawn(async move {
                    handle_connection(socket, bus).await;
                });
            }
        }
    });

    // 2. Start Network Bridge (Outbound)
    let mut bus_rx = bus_tx.subscribe();
    let my_id_clone = my_id.clone();
    
    tokio::spawn(async move {
        while let Ok(msg) = bus_rx.recv().await {
            // Filter logic: Only send messages originated by me or meant for others I am routing
            // Simplification: msg variants have IDs. 
            
            let target_id = match &msg {
                SwarmMessage::TaskRequest { requester_id, .. } => {
                     // Broadcast if I am requester
                     if requester_id == &my_id_clone {
                         Some("BROADCAST") 
                     } else { None }
                },
                SwarmMessage::Bid { bidder_id, request_id, .. } => {
                    // Unicast to requester if I am bidder
                    // Wait, Bid doesn't have requester_id field. 
                    // Problem: How do I know who requested it?
                    // Discovery! I can find the requester by ID if I knew it.
                    // But I don't know requester_id from Bid msg.
                    // Actually, when I receive Request, I know requester_id. 
                    // SwarmManager tracks pending requests, but not "external requests I am bidding on".
                    // 
                    // FIX: SwarmMessage::Bid needs requester_id, OR we lookup request_id?
                    // No, Request ID is unique UUID.
                    // Maybe we default to BROADCAST for Bids too in this simple demo?
                    // If we broadcast bids, everyone sees them. Requester filters.
                    // It's inefficient but works.
                    if bidder_id == &my_id_clone { Some("BROADCAST") } else { None }
                },
                SwarmMessage::TaskAssignment { assigned_to, .. } => {
                    // I am the assigner (requester). Access protocol logic inside SwarmManager?
                    // No, SwarmManager sends this. 
                    // If I am sending an assignment, I am the requester.
                    // I should send to `assigned_to`.
                    // But wait, TaskAssignment doesn't have `requester_id` field either?
                    // It's implied the sender is the requester.
                    // So we send to `assigned_to`.
                    Some(assigned_to.as_str())
                },
                SwarmMessage::Result { performer_id, .. } => {
                     // If I performed it, broadcast it? 
                     // Or send to requester? Protocol doesn't store requester_id in Result.
                     // Broadcast for now.
                     if performer_id == &my_id_clone { Some("BROADCAST") } else { None }
                },
                SwarmMessage::Announcement(_) => Some("BROADCAST"), // Always broadcast
            };

            if let Some(target) = target_id {
                let json = serde_json::to_string(&msg).unwrap();
                
                if target == "BROADCAST" {
                    // Send to all discovered peers
                    if let Ok(peers) = discovery.list().await {
                        for peer in peers {
                            if peer.id == my_id_clone { continue; }
                            send_tcp(&peer, &json).await;
                        }
                    }
                } else {
                    // Unicast
                    if let Ok(Some(peer)) = discovery.get(target).await {
                         send_tcp(&peer, &json).await;
                    }
                }
            }
        }
    });

    Ok(())
}

async fn handle_connection(mut socket: TcpStream, bus: broadcast::Sender<SwarmMessage>) {
    let mut reader = BufReader::new(socket);
    let mut line = String::new();
    
    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 { break; } // EOF
        if let Ok(msg) = serde_json::from_str::<SwarmMessage>(&line) {
             let _ = bus.send(msg);
        }
        line.clear();
    }
}

async fn send_tcp(peer: &AgentManifest, json: &str) {
    if let Some(addr) = &peer.address {
        // addr is "ip:port"
        // Connect and send line
        if let Ok(mut stream) = TcpStream::connect(addr).await {
            let _ = stream.write_all(json.as_bytes()).await;
            let _ = stream.write_all(b"\n").await;
        } else {
            // warn!("Failed to connect to {}", addr);
        }
    }
}


// --- Worker ---

async fn run_worker(name: String, port: u16) -> Result<()> {
    let provider = MockProvider::new(&name);
    let my_ip = "127.0.0.1"; // Demo assumes local
    
    // 1. Discovery
    let discovery = Arc::new(LocalDiscovery::new());
    
    // 2. Identity
    let my_id = format!("agent-{}", name);
    let manifest = AgentManifest::new(&my_id, &name, AgentRole::Custom("Specialist".to_string()))
        .with_capability("demo_task")
        .with_capability("calc");
    
    // Hack: Manually set address before passing to SwarmManager?
    // No, SwarmManager uses identity.
    // We should update identity with address.
    let mut manifest = manifest;
    manifest.address = Some(format!("{}:{}", my_ip, port));
    
    // 3. Setup Bus & Manager
    let (bus_tx, _) = broadcast::channel(100);
    let mut manager = SwarmManager::new(manifest.clone(), discovery.clone(), bus_tx.clone());
    let cmd_rx = manager.take_command_receiver().expect("Rx");
    let swarm_manager = Arc::new(Mutex::new(manager));
    
    // 4. Start Transport
    start_transport(port, bus_tx.clone(), my_id.clone(), discovery.clone()).await?;

    // 5. Announce
    {
        let mgr = swarm_manager.lock().await;
        mgr.announce().await?;
    }
    
    // 6. Build Agent
    let agent = Agent::builder(provider)
        .with_swarm(swarm_manager.clone(), cmd_rx)
        .build()
        .expect("Build agent");
        
    let agent = Arc::new(agent);
    
    info!("Worker {} ready on {}:{}", name, my_ip, port);
    
    // 7. Listen Loop
    let (_user_tx, user_rx) = tokio::sync::mpsc::channel(10); // Dummy user input
    // We need to keep the main thread alive. 
    // Agent::listen blocks.
    
    // Dummy event rx
    let (_event_tx, event_rx) = tokio::sync::mpsc::channel(10);

    agent.listen(user_rx, event_rx).await?;
    
    Ok(())
}

// --- Requester ---

async fn run_requester(name: String, port: u16) -> Result<()> {
    let provider = MockProvider::new(&name);
    let my_ip = "127.0.0.1";
    
    let discovery = Arc::new(LocalDiscovery::new());
    let my_id = format!("agent-{}", name);
    let mut manifest = AgentManifest::new(&my_id, &name, AgentRole::Custom("Manager".to_string()));
    manifest.address = Some(format!("{}:{}", my_ip, port));

    let (bus_tx, mut bus_rx) = broadcast::channel(100);
    let mut manager = SwarmManager::new(manifest.clone(), discovery.clone(), bus_tx.clone());
    let cmd_rx = manager.take_command_receiver().expect("Rx");
    let swarm_manager = Arc::new(Mutex::new(manager));

    start_transport(port, bus_tx.clone(), my_id.clone(), discovery.clone()).await?;
    
    {
        let mgr = swarm_manager.lock().await;
        mgr.announce().await?;
    }
    
    // Start Agent in background (to handle logic)
    let agent = Agent::builder(provider)
        .with_swarm(swarm_manager.clone(), cmd_rx)
        .build()
        .expect("Build agent");
    let agent = Arc::new(agent);
    
    let agent_clone = agent.clone();
    tokio::spawn(async move {
        let (_tx, rx) = tokio::sync::mpsc::channel(10);
        let (_etx, erx) = tokio::sync::mpsc::channel(10);
        let _ = agent_clone.listen(rx, erx).await;
    });

    // Discovery Wait
    info!("Requester {} ready on {}:{}. Waiting for peers...", name, my_ip, port);
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    let peers = discovery.list().await?;
    info!("Discovered {} peers", peers.len());
    for p in &peers {
        info!(" - {} ({:?})", p.name, p.address);
    }
    
    if peers.is_empty() {
        warn!("No peers found. Ensure a worker is running.");
    }
    
    // Send Request
    info!("Broadcasting Task Request...");
    let request_id;
    {
        let mut mgr = swarm_manager.lock().await;
        request_id = mgr.broadcast_request("Calculate 2 + 2", vec!["calc".to_string()]).await?;
    }
    
    info!("Request sent: {}. Waiting for results...", request_id);
    
    // Wait for result loop
    // Since Agent runs in background, it will handle the result via swarm manager?
    // SwarmManager receives Result, sends TaskResult event to Agent.
    // Agent::process executes... wait. 
    // Requester Agent doesn't execute the result, it receives it.
    // SwarmManager process_inbox handles SwarmMessage::Result.
    // It sends SwarmEvent::TaskResult to Agent.
    // Agent::listen handles SwarmEvent::TaskResult.
    // But Agent::listen doesn't print it to stdout currently, it just logs or does whatever `process` does?
    // Actually Agent::listen implementation:
    // SwarmEvent::TaskResult => self.handle_task_result(...)
    // I should check `Agent::listen` implementation details.
    
    // For Demo purposes, let's just listen on the BUS for the result message directly,
    // so we can print it and exit.
    
    loop {
        if let Ok(msg) = bus_rx.recv().await {
            if let SwarmMessage::Result { request_id: r_id, output, success, performer_id } = msg {
                if r_id == request_id {
                    info!("!!! TASK COMPLETED !!!");
                    info!("Performer: {}", performer_id);
                    info!("Success: {}", success);
                    info!("Output: {}", output);
                    break;
                }
            }
        }
    }
    
    Ok(())
}
