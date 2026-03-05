//! WebSocket Handlers
//!
//! Handles WebSocket connections, message processing, and broadcasting.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::protocol::{ClientMessage, ServerMessage, ClientRole};
use super::state::GatewayState;

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<GatewayState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let addr_str = addr.to_string();
    info!("WebSocket connection from: {}", addr_str);
    ws.on_upgrade(move |socket| handle_socket(socket, state, Some(addr_str)))
}

/// Handle individual WebSocket connection
async fn handle_socket(socket: WebSocket, state: GatewayState, addr: Option<String>) {
    let client_id = Uuid::new_v4();
    let mut broadcast_rx = state.register_client(client_id, addr.clone());

    let (mut ws_sink, mut ws_stream) = socket.split();

    // If no auth token is set, we can auto-authenticate as Master (for local/dev ease)
    // but better to default to Guest and require a token if one is configured.
    let needs_auth = state.get_auth_token().is_some();
    if !needs_auth {
        state.authenticate_client(&client_id, ClientRole::Master);
    }

    // Main message loop
    loop {
        tokio::select! {
            // Incoming message from client
            Some(result) = ws_stream.next() => {
                match result {
                    Ok(Message::Text(text)) => {
                        if let Err(e) = handle_client_message(&text, &client_id, &state, &mut ws_sink).await {
                            warn!("Error handling message: {}", e);
                            let error = ServerMessage::Error { 
                                code: "handler_error".to_string(), 
                                message: e 
                            };
                            if let Ok(json) = serde_json::to_string(&error) {
                                let _ = ws_sink.send(Message::Text(json.into())).await;
                            }
                        }
                    }
                    Ok(Message::Binary(data)) => {
                        if let Ok(text) = String::from_utf8(data.to_vec()) {
                            if let Err(e) = handle_client_message(&text, &client_id, &state, &mut ws_sink).await {
                                warn!("Error handling binary message: {}", e);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("Client {} disconnected", client_id);
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        let _ = ws_sink.send(Message::Pong(data)).await;
                    }
                    Ok(Message::Pong(_)) => {}
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
            // Broadcast message from server
            Ok(msg) = broadcast_rx.recv() => {
                // Ensure client is authenticated before receiving broadcasts
                let is_auth = state.clients.get(&client_id).map(|c| c.authenticated).unwrap_or(false);
                if is_auth {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if ws_sink.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Cleanup
    state.remove_client(&client_id);
    info!("Client {} cleaned up", client_id);
}

/// Handle a client message
async fn handle_client_message(
    text: &str,
    client_id: &Uuid,
    state: &GatewayState,
    ws_sink: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), String> {
    let msg: ClientMessage = serde_json::from_str(text)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    // Check authentication status
    let (is_auth, client_role) = {
        let client = state.clients.get(client_id).ok_or("Client not found")?;
        (client.authenticated, client.role)
    };

    match msg {
        ClientMessage::Connect { role, token } => {
            if is_auth {
                return Err("Already authenticated".to_string());
            }

            // Verify token
            if let Some(server_token) = state.get_auth_token() {
                if token != server_token {
                    return Err("Invalid authentication token".to_string());
                }
            }

            // Update state
            state.authenticate_client(client_id, role);
            info!("Client {} authenticated as {:?}", client_id, role);

            // Send success
            let response = ServerMessage::AuthSuccess { role };
            if let Ok(json) = serde_json::to_string(&response) {
                let _ = ws_sink.send(Message::Text(json.into())).await;
            }

            // Send initial status immediately after auth
            let status = build_status(state);
            let status_msg = ServerMessage::Status(status);
            if let Ok(json) = serde_json::to_string(&status_msg) {
                let _ = ws_sink.send(Message::Text(json.into())).await;
            }
        }
        ClientMessage::Ping => {
            let pong = ServerMessage::Pong;
            if let Ok(json) = serde_json::to_string(&pong) {
                let _ = ws_sink.send(Message::Text(json.into())).await;
            }
        }
        _ => {
            // All other messages require authentication
            if !is_auth {
                return Err("Authentication required".to_string());
            }

            match msg {
                ClientMessage::Subscribe { topics } => {
                    state.update_subscriptions(client_id, topics.clone());
                    debug!("Client {} subscribed to: {:?}", client_id, topics);
                }
                ClientMessage::Unsubscribe { topics } => {
                    if let Some(mut client) = state.clients.get_mut(client_id) {
                        client.topics.retain(|t| !topics.contains(t));
                    }
                }
                ClientMessage::Inbound { message } => {
                    // Only Nodes and Master can inject inbound messages
                    if client_role != ClientRole::Node && client_role != ClientRole::Master {
                        return Err(format!("Role {:?} not authorized to inject messages", client_role));
                    }
                    
                    debug!("Inbound message from node {}: {:?}", client_id, message);
                    state.bus.publish_inbound(message).await
                        .map_err(|e| format!("Failed to publish to bus: {}", e))?;
                }
                ClientMessage::Admin { command } => {
                    // RBAC check: only Master can run admin commands
                    if client_role != ClientRole::Master {
                        return Err("Unauthorized: Admin relative commands require Master role".to_string());
                    }

                    match command {
                        super::protocol::AdminCommand::RotateToken { new_token } => {
                            state.update_auth_token(new_token);
                            info!("Auth token rotated by client {}", client_id);
                            
                            let response = ServerMessage::Log(super::protocol::LogEntry {
                                channel: "gateway".to_string(),
                                level: "INFO".to_string(),
                                message: "Authentication token rotated successfully".to_string(),
                                timestamp: chrono::Utc::now(),
                            });
                            let _ = ws_sink.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await;
                        }
                        super::protocol::AdminCommand::EvictClient { client_id: target_id } => {
                            if target_id == *client_id {
                                return Err("Cannot evict yourself".to_string());
                            }
                            state.remove_client(&target_id);
                            info!("Client {} evicted by admin {}", target_id, client_id);
                        }
                        super::protocol::AdminCommand::SecurityAudit => {
                            let mut issues = Vec::new();
                            let token_set = state.get_auth_token().is_some();
                            
                            if !token_set {
                                issues.push("CRITICAL: No authentication token set. Gateway is unprotected.");
                            }
                            
                            // Simple security score
                            let score = if !token_set { 0 } else { 100 };
                            
                            let report = format!("Security Audit Report\nScore: {}/100\nIssues: {}", score, if issues.is_empty() { "None detected".to_string() } else { issues.join("\n") });
                            
                            let response = ServerMessage::Log(super::protocol::LogEntry {
                                channel: "security".to_string(),
                                level: "WARN".to_string(),
                                message: report,
                                timestamp: chrono::Utc::now(),
                            });
                            let _ = ws_sink.send(Message::Text(serde_json::to_string(&response).unwrap().into())).await;
                        }
                    }
                }
                _ => unreachable!(), // Connect and Ping already handled
            }
        }
    }

    Ok(())
}

/// Build current gateway status
fn build_status(state: &GatewayState) -> super::protocol::GatewayStatus {
    use sysinfo::{Disks, System};
    use super::protocol::{SystemStats, MemoryStats, DiskStats};
    
    let mut sys = System::new_all();
    sys.refresh_all();
    
    // CPU usage (average across all cores)
    let cpu = sys.global_cpu_usage();
    
    // Memory
    let mem_total = sys.total_memory();
    let mem_used = sys.used_memory();
    let mem_percent = if mem_total > 0 {
        (mem_used as f32 / mem_total as f32) * 100.0
    } else {
        0.0
    };
    
    // Disk (root partition)
    let disks = Disks::new_with_refreshed_list();
    let (disk_used, disk_total) = disks.iter()
        .find(|d| d.mount_point() == std::path::Path::new("/"))
        .map(|d| (d.total_space() - d.available_space(), d.total_space()))
        .unwrap_or((0, 0));
    let disk_percent = if disk_total > 0 {
        (disk_used as f32 / disk_total as f32) * 100.0
    } else {
        0.0
    };
    
    // Load average
    let load = System::load_average();
    
    // Connected clients
    let connected_clients = state.clients.iter().map(|c| {
        super::protocol::ClientSnapshot {
            id: *c.key(),
            role: c.value().role,
            addr: c.value().addr.clone(),
            uptime_secs: c.value().connected_at.elapsed().as_secs(),
        }
    }).collect();
    
    super::protocol::GatewayStatus {
        clients: state.client_count(),
        uptime_secs: state.uptime_secs(),
        agents: vec![],
        connected_clients,
        system: Some(SystemStats {
            cpu,
            memory: MemoryStats {
                used: mem_used,
                total: mem_total,
                percent: mem_percent,
            },
            disk: DiskStats {
                used: disk_used,
                total: disk_total,
                percent: disk_percent,
            },
            load: [load.one, load.five, load.fifteen],
            sys_uptime: System::uptime(),
        }),
        timestamp: chrono::Utc::now(),
    }
}
