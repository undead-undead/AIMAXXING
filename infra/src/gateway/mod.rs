//! WebSocket Gateway Server
//!
//! A lightweight WebSocket control plane for real-time monitoring and control.
//!
//! # Example
//!
//! ```ignore
//! use brain::infra::aimaxxing_gateway::Gateway;
//! use brain::bus::MessageBus;
//!
//! let bus = MessageBus::new(100);
//! let gateway = Gateway::builder()
//!     .port(18888)
//!     .with_bus(bus)
//!     .build();
//!
//! gateway.run().await?;
//! ```

pub mod handlers;
pub mod protocol;
pub mod state;
pub mod openai;

use axum::{
    routing::get,
    Router,
    response::IntoResponse,
};
use tower_http::services::ServeDir;
use std::fs;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
};
use tracing::info;

use brain::bus::MessageBus;
use state::GatewayState;

pub use protocol::*;
pub use state::ClientInfo;

/// Gateway server configuration
#[derive(Clone)]
pub struct GatewayConfig {
    /// Server port
    pub port: u16,
    /// Bind address
    pub host: String,
    /// Authentication token (optional)
    pub auth_token: Option<String>,
    /// Web root path (hidden path)
    pub web_root: String,
    /// Enable CORS
    pub enable_cors: bool,
    /// Enable log rotation to canvas/logs
    pub log_to_canvas: bool,
    /// Log retention in days
    pub log_retention_days: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            port: 18888,
            host: "127.0.0.1".to_string(),
            auth_token: None,
            web_root: "/aimaxxing/".to_string(),
            enable_cors: true,
            log_to_canvas: false,
            log_retention_days: 7,
        }
    }
}

/// WebSocket Gateway server
pub struct Gateway {
    config: GatewayConfig,
    state: GatewayState,
}

impl Gateway {
    /// Create a new gateway builder
    pub fn builder() -> GatewayBuilder {
        GatewayBuilder::default()
    }

    /// Create gateway with default config and message bus
    pub fn new(bus: MessageBus) -> Self {
        Self {
            config: GatewayConfig::default(),
            state: GatewayState::new(bus),
        }
    }

    /// Get gateway state for external access
    pub fn state(&self) -> &GatewayState {
        &self.state
    }

    /// Run the gateway server
    pub async fn run(self) -> brain::error::Result<()> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| brain::error::Error::Internal(format!("Invalid address: {}", e)))?;

        let mut nest_path = self.config.web_root.trim_matches('/').to_string();
        if !nest_path.is_empty() {
            nest_path = format!("/{}", nest_path);
        }

        // Initialize Canvas directory
        let canvas_path = std::env::current_dir()
            .map(|p| p.join("canvas"))
            .unwrap_or_else(|_| std::path::PathBuf::from("canvas"));
        
        if !canvas_path.exists() {
            fs::create_dir_all(&canvas_path)
                .map_err(|e| brain::error::Error::Internal(format!("Failed to create canvas dir: {}", e)))?;
            info!("📁 Created canvas directory: {:?}", canvas_path);
        }

        // Setup Log Rotation if enabled
        let mut _log_guard = None;
        if self.config.log_to_canvas {
            let log_dir = canvas_path.join("logs");
            fs::create_dir_all(&log_dir).map_err(|e| brain::error::Error::Internal(format!("Failed to create logs dir: {}", e)))?;
            
            let file_appender = tracing_appender::rolling::daily(&log_dir, "aimaxxing.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
            _log_guard = Some(guard);
            
            // Try to initialize a subscriber if none exists
            let _ = tracing_subscriber::fmt()
                .with_writer(non_blocking)
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .try_init();
            
            info!("🔄 Log rotation enabled: {:?}", log_dir);
        }

        let mut app = Router::new()
            .route(&format!("{}/", nest_path), get(serve_dashboard))
            .route(&format!("{}/ws", nest_path), get(handlers::ws_handler))
            .route(&format!("{}/api/status", nest_path), get(api_status))
            .route(&format!("{}/api/canvas/list", nest_path), get(api_canvas_list))
            .route("/v1/chat/completions", axum::routing::post(openai::chat_completions))
            .route("/v1/models", get(openai::list_models))
            .nest_service(&format!("{}/canvas", nest_path), ServeDir::new(&canvas_path))
            .route(&nest_path, get({
                let r = format!("{}/", nest_path);
                move || async move { axum::response::Redirect::to(&r) }
            }))
            .with_state(self.state.clone());

        // Add CORS if enabled
        if self.config.enable_cors {
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        let listener = TcpListener::bind(addr).await
            .map_err(|e| brain::error::Error::Internal(format!("Failed to bind: {}", e)))?;

        let display_path = if nest_path.is_empty() { "/".to_string() } else { format!("{}/", nest_path) };
        info!("🚀 Gateway running at http://{}", addr);
        info!("   Dashboard: http://{}{}", addr, display_path);
        info!("   WebSocket: ws://{}{}/ws", addr, nest_path);

        // Spawn periodic status broadcast task
        let broadcast_state = self.state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
            loop {
                interval.tick().await;
                let status = build_system_status(&broadcast_state);
                broadcast_state.broadcast(ServerMessage::Status(status));
            }
        });

        // Spawn outbound bridge task (Bus -> Gateway)
        let outbound_state = self.state.clone();
        tokio::spawn(async move {
            let mut rx = outbound_state.bus.subscribe_outbound();
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        // Forward message to all clients
                        let is_cancelled = outbound_state.fulfill_response(&msg.chat_id, msg.clone());

                        // Broadcast to all connected clients (except maybe the sync requester if needed, but for now broadcast all)
                        outbound_state.broadcast(ServerMessage::Outbound { message: msg });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Gateway outbound receiver lagged by {} messages", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::error!("Gateway outbound receiver closed, stopping worker");
                        break;
                    }
                }
            }
        });

        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(|e| brain::error::Error::Internal(format!("Server error: {}", e)))?;

        Ok(())
    }
}

/// Gateway builder
#[derive(Default)]
pub struct GatewayBuilder {
    config: GatewayConfig,
    bus: Option<MessageBus>,
}

impl GatewayBuilder {
    /// Set server port
    pub fn port(mut self, port: u16) -> Self {
        self.config.port = port;
        self
    }

    /// Set bind host
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.config.host = host.into();
        self
    }

    /// Set authentication token
    pub fn auth_token(mut self, token: impl Into<String>) -> Self {
        self.config.auth_token = Some(token.into());
        self
    }

    /// Set message bus
    pub fn with_bus(mut self, bus: MessageBus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Set web root path (hidden path)
    pub fn web_root(mut self, path: impl Into<String>) -> Self {
        self.config.web_root = path.into();
        self
    }

    /// Disable CORS
    pub fn no_cors(mut self) -> Self {
        self.config.enable_cors = false;
        self
    }

    /// Enable log rotation to canvas/logs
    pub fn log_to_canvas(mut self, enabled: bool) -> Self {
        self.config.log_to_canvas = enabled;
        self
    }

    /// Set log retention in days
    pub fn log_retention_days(mut self, days: u32) -> Self {
        self.config.log_retention_days = days;
        self
    }

    /// Build the gateway
    pub fn build(self) -> Gateway {
        let bus = self.bus.unwrap_or_else(|| MessageBus::new(100));
        let mut state = GatewayState::new(bus);
        
        if let Some(token) = self.config.auth_token.clone() {
            state = state.with_auth(token);
        }

        Gateway {
            config: self.config,
            state,
        }
    }
}

/// Serve embedded dashboard HTML
async fn serve_dashboard() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("dashboard.html"))
}

/// API status endpoint
async fn api_status(
    axum::extract::State(state): axum::extract::State<GatewayState>,
) -> axum::Json<GatewayStatus> {
    axum::Json(GatewayStatus {
        clients: state.client_count(),
        uptime_secs: state.uptime_secs(),
        agents: vec![],
        connected_clients: vec![],
        system: None,
        timestamp: chrono::Utc::now(),
    })
}

/// Canvas file info
#[derive(serde::Serialize)]
pub struct CanvasFile {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
}

/// API to list files in the canvas directory
async fn api_canvas_list() -> impl IntoResponse {
    let canvas_path = std::env::current_dir()
        .map(|p| p.join("canvas"))
        .unwrap_or_else(|_| std::path::PathBuf::from("canvas"));

    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(canvas_path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                files.push(CanvasFile {
                    name: entry.file_name().to_string_lossy().to_string(),
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                    modified: meta.modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0),
                });
            }
        }
    }
    
    // Sort by modified time descending
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    
    axum::Json(files)
}

/// Build system status with resource stats
fn build_system_status(state: &GatewayState) -> GatewayStatus {
    use sysinfo::{Disks, System};
    
    let mut sys = System::new_all();
    sys.refresh_all();
    
    // CPU usage
    let cpu = sys.global_cpu_usage();
    
    // Memory
    let mem_total = sys.total_memory();
    let mem_used = sys.used_memory();
    let mem_percent = if mem_total > 0 {
        (mem_used as f32 / mem_total as f32) * 100.0
    } else {
        0.0
    };
    
    // Disk
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
    
    let load = System::load_average();
    
    GatewayStatus {
        clients: state.client_count(),
        uptime_secs: state.uptime_secs(),
        agents: vec![],
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
        connected_clients: state.clients.iter().map(|c| {
            ClientSnapshot {
                id: *c.key(),
                role: c.value().role,
                addr: c.value().addr.clone(),
                uptime_secs: c.value().connected_at.elapsed().as_secs(),
            }
        }).collect(),
        timestamp: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_builder() {
        let gateway = Gateway::builder()
            .port(9999)
            .host("0.0.0.0")
            .build();

        assert_eq!(gateway.config.port, 9999);
        assert_eq!(gateway.config.host, "0.0.0.0");
    }

    #[test]
    fn test_default_config() {
        let config = GatewayConfig::default();
        assert_eq!(config.port, 18888);
        assert_eq!(config.host, "127.0.0.1");
        assert!(config.enable_cors);
    }
}
