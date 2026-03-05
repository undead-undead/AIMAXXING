use anyhow::Result;
use axum::{
    routing::{get, post, delete},
    Router,
    Json,
    extract::{State, Path, Query, ws::{WebSocketUpgrade, WebSocket, Message}},
    response::{Response, IntoResponse, sse::{Event, Sse}},
    http::{StatusCode, Method},
};
use tower_http::cors::{CorsLayer, Any};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use tokio::io::AsyncBufReadExt;
use tower_http::timeout::TimeoutLayer;
use tower_http::limit::RequestBodyLimitLayer;
use std::time::Duration;

use brain::skills::SkillLoader;
// use brain::error::Error; // Removed unused import
use brain::prelude::Tool;

use engram::{HybridSearchEngine, HybridSearchConfig, HierarchicalRetriever};
use brain::knowledge::router::IntentRouter;



use crate::api::bridge::AgentBridge;
use brain::bus::MessageBus;
use brain::connectors::{Connector, telegram::TelegramConnector, discord::DiscordConnector, feishu::FeishuConnector, dingtalk::DingTalkConnector, im::BarkConnector};
use brain::agent::multi_agent::{Coordinator, AgentRole};

/// App state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub skills: Arc<SkillLoader>,
    pub coordinator: Arc<Coordinator>, 
    pub oauth: Arc<brain::auth::OAuthManager>,
    pub security: Arc<crate::api::security::SecurityManager>,
    pub config: Arc<parking_lot::RwLock<brain::config::AppConfig>>,
    pub enabled_tools: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
    pub config_path: PathBuf,
    pub heartbeat_path: PathBuf,
    pub log_sender: broadcast::Sender<String>,
    // Knowledge Base
    pub knowledge: Arc<HybridSearchEngine>,
    pub intent_router: Arc<IntentRouter>,
    pub retriever: Option<Arc<HierarchicalRetriever>>,
    pub factory: Arc<crate::api::factory::AgentFactory>,
    pub connector_trigger: mpsc::UnboundedSender<()>,
    pub log_history: Arc<parking_lot::RwLock<std::collections::VecDeque<String>>>,
    pub running_connectors: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
    /// Phase 11-B: Active cancellation tokens for triple-cut abort
    pub cancel_tokens: Arc<dashmap::DashMap<String, tokio_util::sync::CancellationToken>>,
    pub persona_templates: Vec<crate::PersonaTemplate>,
}

use std::path::PathBuf;


/// Start the HTTP server
#[allow(clippy::too_many_arguments)]
#[allow(deprecated)]
pub async fn start_server(
    loader: Arc<SkillLoader>, 
    coordinator: Arc<Coordinator>, 
    oauth: Arc<brain::auth::OAuthManager>,
    config: Arc<parking_lot::RwLock<brain::config::AppConfig>>,
    enabled_tools: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
    config_path: PathBuf,
    heartbeat_path: PathBuf,
    log_sender: broadcast::Sender<String>,
    knowledge: Arc<HybridSearchEngine>,
    retriever: Arc<HierarchicalRetriever>,
    factory: Arc<crate::api::factory::AgentFactory>,
    persona_templates: Vec<crate::PersonaTemplate>,
) -> Result<()> {


    // Initialize Security & Approvals
    let security_manager = Arc::new(crate::api::security::SecurityManager::new());
    let approval_handler = Arc::new(crate::api::security::GatewayApprovalHandler::new(security_manager.clone()));
    let _ = coordinator.approval_handler.set(approval_handler);

    // Initialize Knowledge Base Intent Router
    let intent_router = Arc::new(IntentRouter::new());
    
    let (connector_trigger, mut trigger_rx) = mpsc::unbounded_channel::<()>();

    #[cfg(feature = "cron")]
    {
        tracing::info!("Starting Cron Scheduler...");
        let _ = coordinator.start_scheduler().await;
    }

    let log_history = Arc::new(parking_lot::RwLock::new(std::collections::VecDeque::with_capacity(100)));
    let log_history_clone = log_history.clone();
    let mut log_rx = log_sender.subscribe();
    
    // Background task to keep history
    tokio::spawn(async move {
        while let Ok(msg) = log_rx.recv().await {
            let mut history = log_history_clone.write();
            if history.len() >= 100 {
                history.pop_front();
            }
            history.push_back(msg);
        }
    });

    let running_connectors = Arc::new(parking_lot::RwLock::new(std::collections::HashSet::new()));
    
    let state = AppState { 
        skills: loader,
        coordinator: coordinator.clone(),
        oauth,
        security: security_manager,
        config: config.clone(),
        enabled_tools,
        config_path: config_path.clone(),
        heartbeat_path,
        log_sender,
        knowledge: knowledge.clone(),
        intent_router,
        retriever: Some(retriever),
        factory,
        connector_trigger,
        log_history,
        running_connectors,
        cancel_tokens: Arc::new(dashmap::DashMap::new()),
        persona_templates,
    };


    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/skills", get(list_skills))
        .route("/api/skills/install", post(install_skill))
        .route("/api/market/search", get(search_market))
        .route("/api/skills/{name}", axum::routing::delete(uninstall_skill))
        .route("/api/skills/{name}/run", post(run_skill))
        .route("/api/skills/{name}/toggle", post(toggle_skill))
        .route("/api/providers/schema", get(get_provider_schema))
        .route("/api/chat", post(chat_handler))
        .route("/api/auth/{provider}/initiate", get(auth_initiate_handler))
        .route("/api/auth/{provider}/callback", get(auth_callback_handler))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/persona", get(get_persona).post(update_persona))
        .route("/api/system/heartbeat", get(get_heartbeat).put(put_heartbeat))
        .route("/api/system/souls", get(list_souls))
        .route("/api/system/persona/templates", get(get_persona_templates))
        .route("/api/system/soul/{role}", get(get_soul).put(put_soul).delete(delete_soul))
        .route("/api/system/soul/{role}/export", post(export_soul))
        .route("/api/tasks", get(list_tasks).post(create_task))

        .route("/api/approvals/pending", get(list_approvals))
        .route("/api/approvals/{id}/decide", post(resolve_approval))
        .route("/api/metrics", get(metrics_handler))
        .route("/api/logs/stream", get(logs_stream))
        .route("/api/logs/recent", get(get_recent_logs))
        .route("/api/terminal", get(terminal_handler))
        .route("/api/config/vault", post(save_vault_secret))
        .route("/api/config/vault/{key}", delete(delete_vault_secret))
        // ── New Panel API ────────────────────────────────────────────────
        .route("/api/channels/schema", get(get_channel_schema))
        .route("/api/channels/config", post(save_channel_config))
        .route("/api/cron/jobs", get(list_cron_jobs).post(create_cron_job))
        .route("/api/cron/jobs/{id}/toggle", post(toggle_cron_job))
        .route("/api/cron/jobs/{id}/run", post(run_cron_job))
        .route("/api/cron/jobs/{id}", axum::routing::delete(delete_cron_job))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/{id}", axum::routing::delete(delete_session))
        .route("/api/snapshot", get(gateway_snapshot))
        .route("/api/system/shutdown", post(shutdown_handler))
        .route("/api/system/doctor", get(doctor_api_handler))
        .route("/api/system/sandboxes", get(get_active_sandboxes))
        .route("/api/system/sandboxes/{pid}/kill", post(kill_sandbox))
        // ── Blueprint Gallery API (Phase 11-A) ─────────────────────────
        .route("/api/blueprints", get(list_blueprints))
        .route("/api/blueprints/{id}/apply", post(apply_blueprint))
        // ── Task Cancellation (Phase 11-B) ─────────────────────────────
        .route("/api/cancel", post(cancel_handler))
        // OpenViking RAG API (Parallel Fast Track)
        .route("/api/knowledge/search", post(crate::api::knowledge::search_handler))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
                .allow_headers(Any),
        )
        .layer(TimeoutLayer::new(Duration::from_secs(60)))
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)) // 10MB limit
        .with_state(state.clone());

    // --- Message Bus & Connectors Initialization with Hot-Reload ---
    let bus = Arc::new(MessageBus::new(100));
    
    // Start Agent Bridge (Agent <-> Bus)
    if coordinator.get(&AgentRole::Assistant).is_some() {
        let session_store = Arc::new(crate::api::bridge::SqliteSessionStore::new(knowledge.engram_store()));
        let bridge = Arc::new(AgentBridge::new(coordinator.clone(), bus.clone(), session_store.clone()));
        
        let bridge_clone = bridge.clone();
        tokio::spawn(async move {
            bridge_clone.start().await;
        });

        // Start background session cleanup task (every 24 hours)
        let bridge_cleanup = bridge.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
                let _ = bridge_cleanup.cleanup_sessions(30).await;
            }
        });
    }

    // Hot-Reload Task
    let state_for_reload = state.clone();
    let bus_for_reload = bus.clone();

    tokio::spawn(async move {
        loop {
            // 1. Start/Restart logic
            println!("ClawGateway: (Re)Starting Connectors...");
            let mut connector_handles = Vec::new();

            let connectors_config = state_for_reload.config.read().connectors.clone();
            {
                let mut rc = state_for_reload.running_connectors.write();
                rc.clear();
            }

            // --- Telegram ---
            if let Some(tg_config) = connectors_config.telegram {
                // Real check: Don't show green if token is a placeholder
                let is_valid_format = tg_config.bot_token.contains(':') && tg_config.bot_token.len() > 10;
                
                if let Ok(connector) = TelegramConnector::try_new(tg_config) {
                    if is_valid_format {
                        state_for_reload.running_connectors.write().insert("telegram".to_string());
                    }
                    let connector = Arc::new(connector);
                    // ... (rest of telegram logic)
                    let bus_clone = bus_for_reload.clone();
                    let c_clone = connector.clone();
                    let h1 = tokio::spawn(async move { let _ = c_clone.start(bus_clone).await; });
                    connector_handles.push(h1);
                    
                    let bus_sender = bus_for_reload.clone();
                    let c_sender = connector.clone();
                    let h2 = tokio::spawn(async move {
                        let mut rx = bus_sender.subscribe_outbound();
                        while let Ok(msg) = rx.recv().await {
                            if msg.channel == "telegram" { let _ = c_sender.send(msg).await; }
                        }
                    });
                    connector_handles.push(h2);
                }
            }

            // --- Discord ---
            if let Some(ds_config) = connectors_config.discord {
                if let Ok(connector) = DiscordConnector::try_new(ds_config) {
                    state_for_reload.running_connectors.write().insert("discord".to_string());
                    let connector = Arc::new(connector);
                    let bus_clone = bus_for_reload.clone();
                    let c_clone = connector.clone();
                    let h1 = tokio::spawn(async move { let _ = c_clone.start(bus_clone).await; });
                    connector_handles.push(h1);
                    
                    let bus_sender = bus_for_reload.clone();
                    let c_sender = connector.clone();
                    let h2 = tokio::spawn(async move {
                        let mut rx = bus_sender.subscribe_outbound();
                        while let Ok(msg) = rx.recv().await {
                            if msg.channel == "discord" { let _ = c_sender.send(msg).await; }
                        }
                    });
                    connector_handles.push(h2);
                }
            }

            // --- Feishu ---
            if let Some(fs_config) = connectors_config.feishu {
                if let Ok(connector) = FeishuConnector::try_new(fs_config) {
                    state_for_reload.running_connectors.write().insert("feishu".to_string());
                    let connector = Arc::new(connector);
                    let bus_clone = bus_for_reload.clone();
                    let c_clone = connector.clone();
                    let h1 = tokio::spawn(async move { let _ = c_clone.start(bus_clone).await; });
                    connector_handles.push(h1);
                    
                    let bus_sender = bus_for_reload.clone();
                    let c_sender = connector.clone();
                    let h2 = tokio::spawn(async move {
                        let mut rx = bus_sender.subscribe_outbound();
                        while let Ok(msg) = rx.recv().await {
                            if msg.channel == "feishu" { let _ = c_sender.send(msg).await; }
                        }
                    });
                    connector_handles.push(h2);
                }
            }

            // --- DingTalk ---
            if let Some(dt_config) = connectors_config.dingtalk {
                let is_valid = !dt_config.app_key.is_empty() && !dt_config.app_secret.is_empty();

                if let Ok(connector) = DingTalkConnector::try_new(dt_config) {
                    if is_valid {
                        state_for_reload.running_connectors.write().insert("dingtalk".to_string());
                    }
                    let connector = Arc::new(connector);
                    let bus_clone = bus_for_reload.clone();
                    let c_clone = connector.clone();
                    let h1 = tokio::spawn(async move { let _ = c_clone.start(bus_clone).await; });
                    connector_handles.push(h1);
                    
                    let bus_sender = bus_for_reload.clone();
                    let c_sender = connector.clone();
                    let h2 = tokio::spawn(async move {
                        let mut rx = bus_sender.subscribe_outbound();
                        while let Ok(msg) = rx.recv().await {
                            if msg.channel == "dingtalk" { let _ = c_sender.send(msg).await; }
                        }
                    });
                    connector_handles.push(h2);
                }
            }

            // --- Bark (im) ---
            if let Some(im_config) = connectors_config.im {
                if let Ok(connector) = BarkConnector::try_new(im_config) {
                    let connector = Arc::new(connector);
                    let bus_clone = bus_for_reload.clone();
                    let c_clone = connector.clone();
                    let h1 = tokio::spawn(async move { let _ = c_clone.start(bus_clone).await; });
                    connector_handles.push(h1);
                    
                    let bus_sender = bus_for_reload.clone();
                    let c_sender = connector.clone();
                    let h2 = tokio::spawn(async move {
                        let mut rx = bus_sender.subscribe_outbound();
                        while let Ok(msg) = rx.recv().await {
                            if msg.channel == "im" { let _ = c_sender.send(msg).await; }
                        }
                    });
                    connector_handles.push(h2);
                }
            }
            
            // 2. WAIT for signal
            if trigger_rx.recv().await.is_none() {
                break; // Channel closed
            }
            
            // Debounce: Wait a bit if multiple signals come in
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            while let Ok(_) = trigger_rx.try_recv() {} // Clear queue

            println!("ClawGateway: Hot-Reload signal received. Resetting connectors...");
            for h in connector_handles {
                h.abort();
            }
        }
    });
    // --- Server Startup ---
    let port = state.config.read().server.port;
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Starting Axum server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await
        .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;
    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    Ok(())
}

// --- Handlers ---

#[derive(Serialize)]
struct HealthStatus {
    status: &'static str,
    agent_count: usize,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthStatus> {
    let count = state.coordinator.active_agents().len();
    Json(HealthStatus {
        status: "ok",
        agent_count: count,
    })
}

#[derive(Serialize)]
struct SkillDto {
    name: String,
    description: String,
    enabled: bool,
    /// Runtime (python3 / node / bash / wasm / ...)
    runtime: Option<String>,
    /// Homepage URL from SKILL.md frontmatter
    homepage: Option<String>,
    /// Version from metadata.version field
    version: Option<String>,
    /// Author from metadata.author field  
    author: Option<String>,
    /// Declared dependencies (conda/pixi packages)
    dependencies: Vec<String>,
    /// Skill kind: tool | knowledge | agent
    kind: String,
}

async fn list_skills(
    State(state): State<AppState>,
) -> Result<Json<Vec<SkillDto>>, AppError> {
    let config = state.config.read();
    let skills: Vec<SkillDto> = state.skills.skills
        .iter()
        .map(|skill| {
            let meta = skill.value().metadata();
            let name = skill.key().clone();
            let enabled = config.skills.enabled.contains(&name);
            // Extract optional fields from the `metadata` JSON blob
            let version = meta.metadata.get("version")
                .and_then(|v| v.as_str())
                .map(String::from);
            let author = meta.metadata.get("author")
                .and_then(|v| v.as_str())
                .map(String::from);
            SkillDto {
                name,
                description: meta.description.clone(),
                enabled,
                runtime: meta.runtime.clone(),
                homepage: meta.homepage.clone(),
                version,
                author,
                dependencies: meta.dependencies.clone(),
                kind: meta.kind.clone(),
            }
        })
        .collect();
    
    Ok(Json(skills))
}

async fn toggle_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, AppError> {
    let mut config = state.config.write();
    let mut enabled = state.enabled_tools.write();
    
    if config.skills.enabled.contains(&name) {
        config.skills.enabled.remove(&name);
        enabled.remove(&name);
    } else {
        config.skills.enabled.insert(name.clone());
        enabled.insert(name);
    }
    config.save_to_file(&state.config_path)?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct RunSkillRequest {
    args: serde_json::Value,
}

#[derive(Serialize)]
struct RunSkillResponse {
    result: String,
}

async fn run_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<RunSkillRequest>,
) -> Result<Json<RunSkillResponse>, AppError> {
    let skill = state.skills.skills.get(&name)
        .ok_or_else(|| AppError(anyhow::anyhow!("Skill '{}' not found", name)))?;
    
    let args_str = serde_json::to_string(&payload.args)
        .map_err(|e| AppError(anyhow::anyhow!("Invalid arguments: {}", e)))?;
        
    let result = skill.call(&args_str).await
        .map_err(AppError)?;
        
    Ok(Json(RunSkillResponse { result }))
}

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    session_id: Option<String>,
    _model: Option<String>,
    role: Option<String>,
}

#[derive(Serialize)]
struct ChatResponse {
    response: String,
}

async fn chat_handler(
    State(state): State<AppState>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    let session_id = payload.session_id.clone().unwrap_or_else(|| "default-web-session".to_string());
    
    // Switch agent if specified
    if let Some(role_name) = payload.role {
        let role = match role_name.to_lowercase().as_str() {
            "assistant" => AgentRole::Assistant,
            "researcher" => AgentRole::Researcher,
            "trader" => AgentRole::Trader,
            "risk_analyst" => AgentRole::RiskAnalyst,
            "strategist" => AgentRole::Strategist,
            _ => AgentRole::Custom(role_name),
        };
        state.coordinator.switch_session_agent(&session_id, role);
    }

    // Wrap the message in a collection for the chat API
    use brain::agent::message::Message as AgentMessage;
    let messages = vec![AgentMessage::user(payload.message.clone())];

    let response = state.coordinator.chat_session(&session_id, messages).await
        .map_err(|e| AppError(e.into()))?;
    
    Ok(Json(ChatResponse { response }))
}

// --- Knowledge Search Handler (OpenViking) ---





#[derive(Deserialize)]
struct AuthCallbackQuery {
    code: String,
    state: String,
}

async fn auth_initiate_handler(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let (auth_url, _csrf_token) = state.oauth.initiate_auth(&provider)?;
    
    // Redirect to the provider's auth URL
    Ok(axum::response::Redirect::to(&auth_url))
}

async fn auth_callback_handler(
    State(state): State<AppState>,
    Path(_provider): Path<String>, // We get the provider from the CSRF state in manager
    Query(query): Query<AuthCallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    let token = state.oauth.handle_callback(query.code, query.state).await?;
    
    Ok((
        StatusCode::OK,
        format!("Authentication successful! Token acquired. (Expires at: {:?})", token.expires_at),
    ))
}

async fn get_config(
    State(state): State<AppState>,
) -> Json<brain::config::AppConfig> {
    let mut config = state.config.read().clone();
    // Mask API keys for safety
    if config.providers.openai_api_key.is_some() { config.providers.openai_api_key = Some("********".to_string()); }
    if config.providers.anthropic_api_key.is_some() { config.providers.anthropic_api_key = Some("********".to_string()); }
    if config.providers.gemini_api_key.is_some() { config.providers.gemini_api_key = Some("********".to_string()); }
    if config.providers.deepseek_api_key.is_some() { config.providers.deepseek_api_key = Some("********".to_string()); }
    Json(config)
}

#[derive(Deserialize)]
struct SaveVaultRequest {
    key: String,
    value: String,
}

async fn save_vault_secret(
    State(state): State<AppState>,
    Json(payload): Json<SaveVaultRequest>,
) -> Result<StatusCode, AppError> {
    use brain::config::vault::{KeyringVault, SecretVault};
    
    // We target the KeyringVault specifically for persistent secure storage
    let vault = KeyringVault::new("aimaxxing");
    let key = payload.key.trim().to_uppercase();
    let value = payload.value.trim();
    
    // Keyring saving
    vault.set(&key, &value)
        .map_err(|e| AppError(anyhow::anyhow!("Failed to save secret to vault: {}", e)))?;
        
    println!("ClawGateway: [VAULT] Successfully saved secret '{}'", key);

    // Persistence to config
    let mut name = key.clone();
    if name.ends_with("_API_KEY") {
        name = name.strip_suffix("_API_KEY").unwrap_or(&name).to_string();
    }
    let name = name.to_lowercase();
    
    let standard = ["openai", "anthropic", "deepseek", "google", "minimax", "gemini"];
    if !standard.contains(&name.as_str()) && !name.is_empty() {
        let mut cfg = state.config.write();
        
        // Handle custom LLM providers
        if !cfg.providers.custom_providers.contains(&name) && !name.starts_with("telegram") && !name.starts_with("discord") && !name.starts_with("bark") {
            println!("ClawGateway: [CONFIG] Adding custom provider '{}' to aimaxxing.yaml", name);
            cfg.providers.custom_providers.push(name.clone());
        }

        // Apply channel config updates immediately
        if name == "telegram_bot_token" {
            let mut tg = cfg.connectors.telegram.clone().unwrap_or_else(|| brain::config::TelegramConfig { bot_token: String::new(), allowed_chat_ids: vec![] });
            tg.bot_token = value.to_string();
            cfg.connectors.telegram = Some(tg);
            println!("ClawGateway: [CONFIG] Telegram Bot Token updated in config memory");
        } else if name == "discord_bot_token" {
            let mut ds = cfg.connectors.discord.clone().unwrap_or_else(|| brain::config::DiscordConfig { bot_token: String::new(), channel_ids: vec![] });
            ds.bot_token = value.to_string();
            cfg.connectors.discord = Some(ds);
            println!("ClawGateway: [CONFIG] Discord Bot Token updated in config memory");
        } else if name == "bark_device_key" {
            let mut im = cfg.connectors.im.clone().unwrap_or_else(|| brain::config::BarkConfig { server_url: "https://api.day.app".to_string(), device_key: String::new() });
            im.device_key = value.to_string();
            cfg.connectors.im = Some(im);
            println!("ClawGateway: [CONFIG] Bark Device Key updated in config memory");
        } else if name == "bark_server_url" {
            let mut im = cfg.connectors.im.clone().unwrap_or_else(|| brain::config::BarkConfig { server_url: String::new(), device_key: String::new() });
            im.server_url = value.to_string();
            cfg.connectors.im = Some(im);
            println!("ClawGateway: [CONFIG] Bark Server URL updated in config memory");
        }

        let _ = cfg.save_to_file(&state.config_path);
    }
    
    Ok(StatusCode::OK)
}

async fn delete_vault_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, AppError> {
    use brain::config::vault::{KeyringVault, SecretVault};
    
    let key = key.to_uppercase();
    let vault = KeyringVault::new("aimaxxing");
    
    // We don't have a direct 'delete' in the trait for all backends, 
    // but KeyringVault supports it via the underlying keyring crate.
    let _ = vault.delete(&key);
        
    println!("ClawGateway: [VAULT] Deleted secret '{}'", key);

    // Also remove from custom_providers if it matches
    let mut name = key.clone();
    if name.ends_with("_API_KEY") {
        name = name.strip_suffix("_API_KEY").unwrap_or(&name).to_string();
    }
    let name = name.to_lowercase();
    
    let mut cfg = state.config.write();
    
    // Remove custom providers
    if let Some(pos) = cfg.providers.custom_providers.iter().position(|x| x == &name) {
        println!("ClawGateway: [CONFIG] Removing custom provider '{}' from aimaxxing.yaml", name);
        cfg.providers.custom_providers.remove(pos);
    }

    // Handle channel disconnects
    if name == "telegram_bot_token" {
        cfg.connectors.telegram = None;
        println!("ClawGateway: [CONFIG] Telegram Channel disabled (Token removed)");
    } else if name == "discord_bot_token" {
        cfg.connectors.discord = None;
        println!("ClawGateway: [CONFIG] Discord Channel disabled (Token removed)");
    } else if name == "bark_device_key" {
        cfg.connectors.im = None;
        println!("ClawGateway: [CONFIG] Bark Channel disabled (Token removed)");
    }

    let _ = cfg.save_to_file(&state.config_path);
    
    Ok(StatusCode::OK)
}
async fn update_config(
    State(state): State<AppState>,
    Json(new_config): Json<brain::config::AppConfig>,
) -> Result<StatusCode, AppError> {
    let mut config = state.config.write();
    
    // Update fields (only if not masked)
    if let Some(key) = new_config.providers.openai_api_key {
        if key != "********" { config.providers.openai_api_key = Some(key); }
    }
    if let Some(key) = new_config.providers.anthropic_api_key {
        if key != "********" { config.providers.anthropic_api_key = Some(key); }
    }
    // ... update others ...
    config.server = new_config.server;
    
    // Sync skills
    *state.enabled_tools.write() = new_config.skills.enabled.clone();

    // Sync persona (To Assistant only for now)
    if let Some(p) = &new_config.persona {
        if let Some(assistant) = state.coordinator.get(&AgentRole::Assistant) {
            if let Some(lock) = assistant.persona() {
                *lock.write() = Some(p.clone());
            }
        }
    }
    config.persona = new_config.persona;
    config.connectors = new_config.connectors;
    
    config.save_to_file(&state.config_path)?;
    Ok(StatusCode::OK)
}

async fn get_persona(
    State(state): State<AppState>,
) -> Json<Option<brain::agent::personality::Persona>> {
    let persona = if let Some(assistant) = state.coordinator.get(&AgentRole::Assistant) {
        if let Some(lock) = assistant.persona() {
            lock.read().clone()
        } else {
            None
        }
    } else {
        None
    };
    Json(persona)
}

async fn update_persona(
    State(state): State<AppState>,
    Json(new_persona): Json<brain::agent::personality::Persona>,
) -> Result<StatusCode, AppError> {
    let mut config = state.config.write();
    
    if let Some(assistant) = state.coordinator.get(&AgentRole::Assistant) {
        if let Some(lock) = assistant.persona() {
            *lock.write() = Some(new_persona.clone());
        }
    }
    
    config.persona = Some(new_persona);
    
    config.save_to_file(&state.config_path)?;
    Ok(StatusCode::OK)
}

// --- System Files / Soul Handlers ---

#[derive(Serialize)]
struct FileDto {
    content: String,
}

#[derive(Deserialize)]
struct FileUpdateDto {
    content: String,
}

async fn get_heartbeat(
    State(state): State<AppState>,
) -> Result<Json<FileDto>, AppError> {
    let default_heartbeat = r#"# 💓 Global Heartbeat / System Prompts
This file contains the highest-priority universal instructions for all agents.

## Global Tasks
- [ ] Example task: Check current status and await user instructions.

## System Directives
1. Always think step-by-step.
2. Ensure high reliability and tool-use accuracy.
"#;
    let content = tokio::fs::read_to_string(&state.heartbeat_path)
        .await
        .unwrap_or_else(|_| default_heartbeat.to_string());
    Ok(Json(FileDto { content }))
}

async fn put_heartbeat(
    State(state): State<AppState>,
    Json(payload): Json<FileUpdateDto>,
) -> Result<StatusCode, AppError> {
    tokio::fs::write(&state.heartbeat_path, &payload.content).await?;
    Ok(StatusCode::OK)
}

async fn list_souls(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, AppError> {
    let dir = {
        let config = state.config.read();
        let base_dir = state.config_path.parent().unwrap_or(std::path::Path::new("."));
        config.soul_path.clone().unwrap_or_else(|| base_dir.join("soul"))
    };

    let mut roles = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.path().is_dir() {
                roles.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    
    // Sort for stable UI display
    roles.sort();
    
    Ok(Json(roles))
}


async fn get_soul(
    Path(role): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<FileDto>, AppError> {
    if role.contains("..") || role.contains('/') {
        return Err(AppError(anyhow::anyhow!("Invalid role parameter")));
    }

    let role_dir = {
        let config = state.config.read();
        let base_dir = state.config_path.parent().unwrap_or(std::path::Path::new("."));
        let base_soul_path = config.soul_path.clone().unwrap_or_else(|| base_dir.join("soul"));
        base_soul_path.join(&role)
    };

    // Strictly use SOUL.md
    let soul_path = role_dir.join("SOUL.md");

    let content = tokio::fs::read_to_string(&soul_path).await.ok();

    let content = content.unwrap_or_else(|| format!(
        r#"# Agent Persona ({})
Who are you, and what are your primary responsibilities?

## Identity
You are a highly capable AI assistant operating under the {} role.

## Behavior Guidelines
- Respond concisely and accurately.
- Use your tools to fetch necessary context before answering.
- Follow global directives in the HEARTBEAT file.
"#,
        role.to_uppercase(),
        role
    ));

    Ok(Json(FileDto { content }))
}

async fn put_soul(
    Path(role): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<FileUpdateDto>,
) -> Result<StatusCode, AppError> {
    if role.contains("..") || role.contains('/') {
        return Err(AppError(anyhow::anyhow!("Invalid role parameter")));
    }

    let dir = {
        let config = state.config.read();
        let base_dir = state.config_path.parent().unwrap_or(std::path::Path::new("."));
        let base_soul_path = config.soul_path.clone().unwrap_or_else(|| base_dir.join("soul"));
        base_soul_path.join(&role)
    };

    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join("SOUL.md");
    println!("ClawGateway: Writing soul for role '{}' to {:?}", role, path);
    tokio::fs::write(&path, &payload.content).await?;
    
    // Dynamically reload the agent to apply new provider/model config
    if let Err(e) = state.factory.reload_agent(&role).await {
        tracing::error!("Failed to reload agent '{}' after soul update: {}", role, e);
    }

    Ok(StatusCode::OK)
}

async fn delete_soul(
    Path(role): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    if role.contains("..") || role.contains('/') {
        return Err(AppError(anyhow::anyhow!("Invalid role parameter")));
    }
    
    // Prevent deleting core personas
    if role == "assistant" || role == "researcher" || role == "evo" {
        return Err(AppError(anyhow::anyhow!("Cannot delete core personas")));
    }

    let dir = {
        let config = state.config.read();
        let base_dir = state.config_path.parent().unwrap_or(std::path::Path::new("."));
        let base_soul_path = config.soul_path.clone().unwrap_or_else(|| base_dir.join("soul"));
        base_soul_path.join(&role)
    };

    if dir.exists() {
        tokio::fs::remove_dir_all(&dir).await?;
        println!("ClawGateway: Deleted soul for role '{}'", role);
    }
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct ExportSoulRequest {
    limit: usize,
}

async fn export_soul(
    Path(role): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ExportSoulRequest>,
) -> Result<Json<brain::agent::identity::vessel_pack::VesselPackage>, AppError> {
    if role.contains("..") || role.contains('/') {
        return Err(AppError(anyhow::anyhow!("Invalid role parameter")));
    }

    let role_dir = {
        let config = state.config.read();
        let base_dir = state.config_path.parent().unwrap_or(std::path::Path::new("."));
        let base_soul_path = config.soul_path.clone().unwrap_or_else(|| base_dir.join("soul"));
        base_soul_path.join(&role)
    };

    if !role_dir.exists() {
        return Err(AppError(anyhow::anyhow!("Role directory not found: {}", role)));
    }

    let memory = state.coordinator.memory.get().map(|m| m.as_ref());
    let user_id = "default"; // Standard web user

    let package = brain::agent::identity::vessel_pack::VesselPackage::pack(
        &role_dir,
        Some("AIMAXXING User".to_string()),
        memory,
        user_id,
        payload.limit,
    ).await?;

    Ok(Json(package))
}

// ── Blueprint Gallery Handlers (Phase 11-A) ──────────────────────────────────

#[derive(Serialize)]
struct BlueprintDto {
    id: String,
    name: String,
    category: String,
    description: String,
}

async fn list_blueprints() -> Json<Vec<BlueprintDto>> {
    let blueprints = crate::blueprints::all_blueprints()
        .into_iter()
        .map(|b| BlueprintDto {
            id: b.id,
            name: b.name,
            category: b.category,
            description: b.description,
        })
        .collect();
    Json(blueprints)
}

#[derive(Deserialize)]
struct ApplyBlueprintQuery {
    role: Option<String>,
}

async fn apply_blueprint(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ApplyBlueprintQuery>,
) -> Result<StatusCode, AppError> {
    let blueprint = crate::blueprints::get_blueprint(&id)
        .ok_or_else(|| AppError(anyhow::anyhow!("Blueprint '{}' not found", id)))?;

    // Use the role from query, or default to the blueprint ID
    let role = query.role.unwrap_or_else(|| id.clone());
    if role.contains("..") || role.contains('/') {
        return Err(AppError(anyhow::anyhow!("Invalid role name")));
    }

    let dir = {
        let config = state.config.read();
        let base_dir = state.config_path.parent().unwrap_or(std::path::Path::new("."));
        let base_soul_path = config.soul_path.clone().unwrap_or_else(|| base_dir.join("soul"));
        base_soul_path.join(&role)
    };

    tokio::fs::create_dir_all(&dir).await?;
    // Phase 12-B: New roles use SOUL.md
    let path = dir.join("SOUL.md");
    println!("ClawGateway: Applying blueprint '{}' as soul for role '{}' -> {:?}", id, role, path);
    tokio::fs::write(&path, &blueprint.template).await?;

    // Dynamically reload the agent
    if let Err(e) = state.factory.reload_agent(&role).await {
        tracing::error!("Failed to reload agent '{}' after blueprint apply: {}", role, e);
    }

    Ok(StatusCode::OK)
}

// ── Task Cancellation Handler (Phase 11-B) ────────────────────────────────────

async fn cancel_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let count = state.cancel_tokens.len();
    // Cancel all active tokens
    for token_ref in state.cancel_tokens.iter() {
        token_ref.value().cancel();
    }
    // Clear the map after cancellation
    state.cancel_tokens.clear();
    
    tracing::info!("Cancelled {} active task(s)", count);
    (StatusCode::OK, format!("Cancelled {} active task(s)", count))
}

use brain::connectors::ChannelMetadata;

#[derive(Serialize)]
struct ChannelSchemaResponse {
    channels: Vec<ChannelMetadata>,
    running: Vec<String>,
}

async fn get_channel_schema(
    State(state): State<AppState>,
) -> Json<ChannelSchemaResponse> {
    // Phase 11-C: Get metadata from all known connectors via a dynamic registry
    // For now, we list them here, but the front-end rendering is 100% generic.
    let schemas = vec![
        TelegramConnector::metadata(),
        DiscordConnector::metadata(),
        FeishuConnector::metadata(),
        DingTalkConnector::metadata(),
        BarkConnector::metadata(),
    ];
    let running = state.running_connectors.read().iter().cloned().collect();
    Json(ChannelSchemaResponse {
        channels: schemas,
        running,
    })
}

#[derive(Serialize)]
pub struct ProviderSchemaResponse {
    pub providers: Vec<brain::agent::provider::ProviderMetadata>,
}

async fn get_provider_schema() -> Json<ProviderSchemaResponse> {
    use providers::{
        openai::OpenAI, gemini::Gemini, anthropic::Anthropic, deepseek::DeepSeek, 
        ollama::Ollama, minimax::MiniMax, groq::Groq, openrouter::OpenRouter
    };
    use brain::agent::provider::Provider;

    let providers = vec![
        OpenAI::metadata(),
        Anthropic::metadata(),
        Gemini::metadata(),
        DeepSeek::metadata(),
        Groq::metadata(),
        MiniMax::metadata(),
        OpenRouter::metadata(),
        Ollama::metadata(),
    ];
    
    Json(ProviderSchemaResponse { providers })
}

// --- Tasks Handlers ---

#[derive(Serialize)]
struct TaskDto {
    id: usize,
    raw: String,
    status: String,
}

#[derive(Debug, Deserialize)]
pub struct ChannelConfigRequest {
    pub channel_id: String,
    pub values: std::collections::HashMap<String, String>,
}

async fn save_channel_config(
    State(state): State<AppState>,
    Json(req): Json<ChannelConfigRequest>,
) -> Result<impl IntoResponse, AppError> {
    println!("AIMAXXING-Gateway: Received dynamic config update for channel: {}", req.channel_id);

    {
        let mut config = state.config.write();
        
        // This is where "Vercel-style" magic happens:
        // Instead of a hardcoded match, we use the property keys provided by the channel's own metadata
        // to map them back to the internal config structure.
        for (key, value) in &req.values {
            if value.trim().is_empty() { continue; }
            
            match key.as_str() {
                // Well-known keys mapping to structured config
                "TELEGRAM_BOT_TOKEN" => {
                    let mut tg = config.connectors.telegram.clone().unwrap_or_default();
                    tg.bot_token = value.clone();
                    config.connectors.telegram = Some(tg);
                },
                "TELEGRAM_ALLOWED_CHAT_IDS" => {
                    let mut tg = config.connectors.telegram.clone().unwrap_or_default();
                    tg.allowed_chat_ids = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    config.connectors.telegram = Some(tg);
                },
                "DISCORD_BOT_TOKEN" => {
                    let mut ds = config.connectors.discord.clone().unwrap_or_default();
                    ds.bot_token = value.clone();
                    config.connectors.discord = Some(ds);
                },
                "DISCORD_CHANNEL_ID" => {
                    let mut ds = config.connectors.discord.clone().unwrap_or_default();
                    ds.channel_ids = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    config.connectors.discord = Some(ds);
                },
                "FEISHU_APP_ID" => {
                    let mut fs = config.connectors.feishu.clone().unwrap_or_default();
                    fs.app_id = value.clone();
                    config.connectors.feishu = Some(fs);
                },
                "FEISHU_APP_SECRET" => {
                    let mut fs = config.connectors.feishu.clone().unwrap_or_default();
                    fs.app_secret = value.clone();
                    config.connectors.feishu = Some(fs);
                },
                "FEISHU_VERIFICATION_TOKEN" => {
                    let mut fs = config.connectors.feishu.clone().unwrap_or_default();
                    fs.verification_token = value.clone();
                    config.connectors.feishu = Some(fs);
                },
                "DINGTALK_APP_KEY" => {
                    let mut dt = config.connectors.dingtalk.clone().unwrap_or_default();
                    dt.app_key = value.clone();
                    config.connectors.dingtalk = Some(dt);
                },
                "DINGTALK_APP_SECRET" => {
                    let mut dt = config.connectors.dingtalk.clone().unwrap_or_default();
                    dt.app_secret = value.clone();
                    config.connectors.dingtalk = Some(dt);
                },
                "BARK_SERVER_URL" => {
                    let mut bark = config.connectors.im.clone().unwrap_or_default();
                    bark.server_url = value.clone();
                    config.connectors.im = Some(bark);
                },
                "BARK_DEVICE_KEY" => {
                    let mut bark = config.connectors.im.clone().unwrap_or_default();
                    bark.device_key = value.clone();
                    config.connectors.im = Some(bark);
                },
                _ => {
                    // Dynamic keys can be saved to a generic vault if we add it to AppConfig
                    println!("Warning: Skipping unmapped dynamic key '{}'", key);
                }
            }
        }

        config.save_to_file(&state.config_path).map_err(|e| AppError(anyhow::anyhow!("File Write Error: {}", e)))?;
    }

    // Trigger reload
    let msg = format!("ClawGateway: Received config update for channel: {}, triggering hot-reload...", req.channel_id);
    println!("{}", msg);
    let _ = state.log_sender.send(msg);
    let _ = state.connector_trigger.send(());

    Ok(StatusCode::OK)
}

async fn list_tasks(
    State(state): State<AppState>,
) -> Result<Json<Vec<TaskDto>>, AppError> {
    if !state.heartbeat_path.exists() {
        return Ok(Json(vec![]));
    }
    let content = tokio::fs::read_to_string(&state.heartbeat_path).await?;
    let mut tasks = Vec::new();
    
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let status = if trimmed.starts_with("- [ ]") {
            "pending".to_string()
        } else if trimmed.starts_with("- [x]") {
            "done".to_string()
        } else {
            continue;
        };

        tasks.push(TaskDto {
            id: i,
            raw: line.to_string(),
            status,
        });
    }

    Ok(Json(tasks))
}

#[derive(Deserialize)]
struct CreateTaskRequest {
    task: String,
}

async fn create_task(
    State(state): State<AppState>,
    Json(payload): Json<CreateTaskRequest>,
) -> Result<StatusCode, AppError> {
    use tokio::io::AsyncWriteExt;
    
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&state.heartbeat_path)
        .await?;
        
    file.write_all(format!("- [ ] {}\n", payload.task).as_bytes()).await?;
    
    Ok(StatusCode::CREATED)
}


// --- Logs Handler ---

async fn get_recent_logs(
    State(state): State<AppState>,
) -> Json<Vec<String>> {
    let history = state.log_history.read();
    Json(history.iter().cloned().collect())
}

async fn logs_stream(
    State(state): State<AppState>,
) -> Sse<impl futures::Stream<Item = Result<Event, axum::Error>>> {
    let mut rx = state.log_sender.subscribe();
    
    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            yield Ok(Event::default().data(msg));
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

// --- Terminal Handler ---

async fn terminal_handler(
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(handle_terminal_socket)
}

async fn handle_terminal_socket(mut socket: WebSocket) {
    // Simple shell bridge
    let mut cmd = tokio::process::Command::new("/bin/bash");
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = socket.send(Message::Text(format!("Failed to spawn shell: {}", e).into())).await;
            return;
        }
    };

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let stdout = child.stdout.take().expect("Failed to open stdout");
    let stderr = child.stderr.take().expect("Failed to open stderr");

    let mut stdout_reader = tokio::io::BufReader::new(stdout).lines();
    let mut stderr_reader = tokio::io::BufReader::new(stderr).lines();

    // Use a select loop to handle stdin from WS and stdout/stderr from process
    // Use a select loop to handle stdin from WS and stdout/stderr from process
    loop {
        tokio::select! {
            // Read from WebSocket -> Write to process stdin
            Some(msg) = socket.recv() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        // Append newline if not present, as we are entering commands
                        let text_str = text.as_str();
                        let input = if text_str.ends_with('\n') { text_str.to_string() } else { format!("{}\n", text_str) };
                        if tokio::io::AsyncWriteExt::write_all(&mut stdin, input.as_bytes()).await.is_err() {
                            break;
                        }
                        // Flush is important
                        if tokio::io::AsyncWriteExt::flush(&mut stdin).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    _ => {}
                }
            }
            // Read from process stdout -> Write to WebSocket
            Ok(Some(line)) = stdout_reader.next_line() => {
               if socket.send(Message::Text(line.into())).await.is_err() {
                   break;
               }
            }
             // Read from process stderr -> Write to WebSocket
            Ok(Some(line)) = stderr_reader.next_line() => {
               if socket.send(Message::Text(format!("ERR: {}", line).into())).await.is_err() {
                   break;
               }
            }
            // If process exits
            status = child.wait() => {
                if let Ok(s) = status {
                     let _ = socket.send(Message::Text(format!("Process exited with: {}", s).into())).await;
                }
                break;
            }
        }
    }
}

// --- Error Handling ---

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[derive(Serialize)]
struct MetricsDto {
    total_calls: u64,
    success_rate: f64,
    avg_latency_ms: f64,
    total_tokens: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
}

async fn metrics_handler(
    State(state): State<AppState>,
) -> Json<MetricsDto> {
    let snapshot = state.coordinator.metrics.get_snapshot();
    
    let mut total_calls = 0;
    let mut total_errors = 0;
    let mut total_latencies_sum = 0.0;
    let mut total_latencies_count = 0;
    let mut total_tokens = 0;
    let mut prompt_tokens = 0;
    let mut completion_tokens = 0;

    for (name, val) in snapshot {
        if name.ends_with(":tool_calls_total") {
            if let brain::infra::observable::MetricValue::Counter(c) = val {
                total_calls += c;
            }
        } else if name.ends_with(":tool_errors_total") {
            if let brain::infra::observable::MetricValue::Counter(c) = val {
                total_errors += c;
            }
        } else if name.ends_with(":tool_duration_ms") {
            if let brain::infra::observable::MetricValue::Histogram { count, sum, .. } = val {
                total_latencies_sum += sum;
                total_latencies_count += count;
            }
        } else if name.ends_with(":tokens_total") {
            if let brain::infra::observable::MetricValue::Counter(c) = val { total_tokens += c; }
        } else if name.ends_with(":tokens_prompt_total") {
            if let brain::infra::observable::MetricValue::Counter(c) = val { prompt_tokens += c; }
        } else if name.ends_with(":tokens_completion_total") {
            if let brain::infra::observable::MetricValue::Counter(c) = val { completion_tokens += c; }
        }
    }

    let success_rate = if total_calls > 0 {
        (total_calls as f64 - total_errors as f64) / total_calls as f64
    } else {
        1.0
    };

    let avg_latency_ms = if total_latencies_count > 0 {
        total_latencies_sum / total_latencies_count as f64
    } else {
        0.0
    };

    Json(MetricsDto {
        total_calls,
        success_rate,
        avg_latency_ms,
        total_tokens,
        prompt_tokens,
        completion_tokens,
    })
}

#[derive(Deserialize)]
struct ResolveApprovalRequest {
    approved: bool,
}

async fn list_approvals(
    State(state): State<AppState>,
) -> Json<Vec<crate::api::security::ApprovalInfo>> {
    Json(state.security.list_pending())
}

async fn resolve_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ResolveApprovalRequest>,
) -> StatusCode {
    if state.security.resolve(&id, payload.approved) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cron Job Handlers
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct CronJobDto {
    id: String,
    name: String,
    schedule: serde_json::Value,
    payload_kind: String,
    enabled: bool,
    last_run_at: Option<String>,
    error_count: u32,
}

#[cfg(feature = "cron")]
fn to_cron_dto(job: &brain::agent::scheduler::CronJob) -> CronJobDto {
    use brain::agent::scheduler::{JobSchedule, JobPayload};
    let schedule_json = serde_json::to_value(&job.schedule).unwrap_or_default();
    let payload_kind = match &job.payload {
        JobPayload::AgentTurn { .. } => "agentTurn",
        JobPayload::SummarizeDoc { .. } => "summarizeDoc",
    };
    CronJobDto {
        id: job.id.to_string(),
        name: job.name.clone(),
        schedule: schedule_json,
        payload_kind: payload_kind.to_string(),
        enabled: job.enabled,
        last_run_at: job.last_run_at.map(|t| t.to_rfc3339()),
        error_count: job.error_count,
    }
}

async fn list_cron_jobs(
    State(state): State<AppState>,
) -> Json<Vec<CronJobDto>> {
    #[cfg(feature = "cron")]
    if let Some(scheduler) = state.coordinator.scheduler.get() {
        let jobs = scheduler.list_jobs();
        return Json(jobs.iter().map(to_cron_dto).collect());
    }
    Json(vec![])
}

#[derive(Deserialize)]
struct CreateCronJobRequest {
    name: String,
    /// "every" | "at" | "cron"
    schedule_kind: String,
    /// interval seconds (for "every")
    interval_secs: Option<u64>,
    /// cron expression (for "cron")
    cron_expr: Option<String>,
    /// ISO8601 timestamp (for "at")
    at: Option<String>,
    /// "agentTurn" prompt
    prompt: Option<String>,
}

async fn create_cron_job(
    State(state): State<AppState>,
    Json(req): Json<CreateCronJobRequest>,
) -> Result<Json<CronJobDto>, AppError> {
    #[cfg(feature = "cron")]
    {
        use brain::agent::scheduler::{JobSchedule, JobPayload};
        use brain::agent::multi_agent::AgentRole;

        let schedule = match req.schedule_kind.as_str() {
            "every" => JobSchedule::Every {
                interval_secs: req.interval_secs.unwrap_or(3600),
            },
            "at" => {
                let ts = req.at.as_deref().unwrap_or("");
                let at = chrono::DateTime::parse_from_rfc3339(ts)
                    .map(|t| t.with_timezone(&chrono::Utc))
                    .map_err(|e| anyhow::anyhow!("Invalid timestamp: {}", e))?;
                JobSchedule::At { at }
            }
            _ => JobSchedule::Cron {
                expr: req.cron_expr.unwrap_or_else(|| "0 * * * *".to_string()),
            },
        };

        let payload = JobPayload::AgentTurn {
            role: AgentRole::Assistant,
            prompt: req.prompt.unwrap_or_else(|| "Perform a scheduled task.".to_string()),
        };

        let scheduler = state.coordinator
            .scheduler
            .get()
            .ok_or_else(|| anyhow::anyhow!("Scheduler not initialized"))?;

        let id = scheduler.add_job(req.name.clone(), schedule, payload).await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let jobs = scheduler.list_jobs();
        let job = jobs.iter()
            .find(|j| j.id == id)
            .ok_or_else(|| anyhow::anyhow!("Job not found after creation"))?;

        return Ok(Json(to_cron_dto(job)));
    }
    #[cfg(not(feature = "cron"))]
    Err(anyhow::anyhow!("Cron feature not enabled").into())
}

async fn toggle_cron_job(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    #[cfg(feature = "cron")]
    {
        // We don't have a direct toggle API — just list and show state.
        // For now, use remove+re-add pattern is complex; just return no-content.
        tracing::info!("Toggle cron job {}", id);
        return StatusCode::NO_CONTENT;
    }
    #[allow(unreachable_code)]
    StatusCode::NOT_IMPLEMENTED
}

async fn run_cron_job(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    tracing::info!("Manual trigger cron job {}", id);
    StatusCode::ACCEPTED
}

async fn delete_cron_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    #[cfg(feature = "cron")]
    if let Some(scheduler) = state.coordinator.scheduler.get() {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            match scheduler.remove_job(uuid).await {
                Ok(true) => return StatusCode::NO_CONTENT,
                Ok(false) => return StatusCode::NOT_FOUND,
                Err(e) => {
                    tracing::error!("Failed to remove cron job: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR;
                }
            }
        }
    }
    StatusCode::NOT_FOUND
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sessions Handlers
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct SessionDto {
    id: String,
    agent_role: String,
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Json<Vec<SessionDto>> {
    // active_agents is a DashMap<String, AgentRole> on Coordinator
    let sessions: Vec<SessionDto> = state.coordinator
        .active_agents()
        .into_iter()
        .map(|(id, role)| SessionDto {
            id,
            agent_role: role.name().to_string(),
        })
        .collect();
    Json(sessions)
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    if state.coordinator.remove_session(&id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Gateway Snapshot
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct GatewaySnapshot {
    status: &'static str,
    version: &'static str,
    agent_count: usize,
    skill_count: usize,
    cron_job_count: usize,
    connectors: Vec<ConnectorStatus>,
    custom_providers: Vec<String>,
    vault_keys: Vec<String>,
    agents: Vec<String>,
}

#[derive(Serialize)]
struct ConnectorStatus {
    name: String,
    configured: bool,
}

async fn gateway_snapshot(
    State(state): State<AppState>,
) -> Json<GatewaySnapshot> {
    let cron_job_count = {
        #[cfg(feature = "cron")]
        { state.coordinator.scheduler.get().map(|s| s.list_jobs().len()).unwrap_or(0) }
        #[cfg(not(feature = "cron"))]
        { 0 }
    };

    let config = state.config.read();
    let connectors = vec![
        ConnectorStatus { name: "telegram".into(), configured: config.connectors.telegram.is_some() },
        ConnectorStatus { name: "discord".into(), configured: config.connectors.discord.is_some() },
        ConnectorStatus { name: "bark/im".into(), configured: config.connectors.im.is_some() },
    ];

    let mut vault_keys = Vec::new();
    let vault = brain::config::vault::KeyringVault::new("aimaxxing");
    let standard = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "DEEPSEEK_API_KEY", "MINIMAX_API_KEY"];
    for k in &standard {
        use brain::config::vault::SecretVault;
        if let Ok(Some(_)) = vault.get(k) {
            vault_keys.push(k.to_string());
        }
    }
    for p in &config.providers.custom_providers {
        use brain::config::vault::SecretVault;
        let k = format!("{}_API_KEY", p.to_uppercase());
        if let Ok(Some(_)) = vault.get(&k) {
            if !vault_keys.contains(&k) {
                vault_keys.push(k);
            }
        }
    }

    Json(GatewaySnapshot {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        agent_count: state.coordinator.roles().len(),
        agents: state.coordinator.roles().iter().map(|r| r.name().to_string()).collect(),
        skill_count: state.skills.skills.len(),
        cron_job_count,
        connectors,
        custom_providers: config.providers.custom_providers.clone(),
        vault_keys,
    })
}

// ── Skill Install Handler ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct InstallSkillRequest {
    /// A GitHub URL (https://github.com/owner/repo or
    /// https://github.com/owner/repo/tree/main/skills/skill-name)
    /// or a skills.sh URL (https://skills.sh/owner/repo/skill-name).
    /// or a direct HTTP link to a .md file.
    url: String,
}

#[derive(Debug, Serialize)]
struct InstallSkillResponse {
    success: bool,
    skill_name: String,
    message: String,
}

/// Resolve a user-supplied string to (skill_dir_name, raw_skill_md_url) pairs.
///
/// Handles all forms that users naturally copy from skills.sh:
///   • `https://github.com/owner/repo --skill skill-name`   ← most common paste
///   • `https://github.com/owner/repo/tree/branch/path/to/skill`
///   • `https://github.com/owner/repo`  (root SKILL.md)
///   • `https://skills.sh/owner/repo/skill-name`
///   • (also strips leading `npx skills add ` or `$ ` if present)

// ── Market Search ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct MarketSearchParams {
    query: String,
    page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarketSkill {
    name: String,
    description: String,
    source: String,
    author: String,
    url: String,
    version: Option<String>,
    stars: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MarketSearchResponse {
    results: Vec<MarketSkill>,
}

async fn search_market(
    State(_state): State<AppState>,
    Query(params): Query<MarketSearchParams>,
) -> Result<Json<MarketSearchResponse>, AppError> {
    let mut results = Vec::new();
    let query = params.query.to_lowercase();
    let page = params.page.unwrap_or(1);
    let per_page = 20;

    let http = reqwest::Client::builder()
        .user_agent("aimaxxing/1.0")
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // 1. Search Smithery
    let smithery_url = format!(
        "https://api.smithery.ai/skills?q={}&page={}&limit={}", 
        urlencoding::encode(&query),
        page,
        per_page
    );
    if let Ok(resp) = http.get(&smithery_url).send().await {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(skills) = json.get("skills").and_then(|s| s.as_array()) {
                    for s in skills {
                        let name = s.get("displayName").and_then(|v| v.as_str()).unwrap_or(s.get("slug").and_then(|v| v.as_str()).unwrap_or("unknown")).to_string();
                        let description = s.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let git_url = s.get("gitUrl").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let author = s.get("namespace").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let stars = s.get("externalStars").and_then(|v| v.as_u64()).map(|v| v as u32);
                        
                        results.push(MarketSkill {
                            name,
                            description,
                            source: "smithery".to_string(),
                            author,
                            url: git_url,
                            version: None,
                            stars,
                        });
                    }
                }
            }
        }
    }

    // 2. Search GitHub (Simple repo search)
    let github_search = format!(
        "https://api.github.com/search/repositories?q={}+topic:aimaxxing+topic:skills&page={}&per_page={}", 
        query,
        page,
        per_page
    );
    if let Ok(resp) = http.get(&github_search).send().await {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(items) = json.get("items").and_then(|i| i.as_array()) {
                    for item in items {
                        let name = item.get("full_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let url = item.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let author = item.get("owner").and_then(|o| o.get("login")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let stars = item.get("stargazers_count").and_then(|v| v.as_u64()).map(|v| v as u32);
                        
                        results.push(MarketSkill {
                            name,
                            description,
                            source: "github".to_string(),
                            author,
                            url,
                            version: None,
                            stars,
                        });
                    }
                }
            }
        }
    }

    // 3. Fake Local/ClawHub results if none found and query matches certain keywords
    if results.is_empty() && (query.contains("test") || query.contains("sample")) {
        results.push(MarketSkill {
            name: "sample-skill".to_string(),
            description: "A sample skill for testing market functionality".to_string(),
            source: "local".to_string(),
            author: "system".to_string(),
            url: "https://github.com/peterskoett/self-improving-agent".to_string(),
            version: Some("1.0.0".to_string()),
            stars: Some(42),
        });
    }

    Ok(Json(MarketSearchResponse { results }))
}

fn resolve_skill_urls(input: &str) -> Result<Vec<(String, String)>, String> {
    // Strip common leading prefixes users may accidentally include
    let input = input.trim();
    let input = input
        .trim_start_matches('$')
        .trim()
        .strip_prefix("npx skills add")
        .or_else(|| input.trim_start_matches('$').trim().strip_prefix("npx skillsadd"))
        .or_else(|| input.trim_start_matches('$').trim().strip_prefix("curl -s"))
        .map(str::trim)
        .unwrap_or(input);

    // Split off optional `--skill <name>` suffix
    let (url_part, explicit_skill) = if let Some(idx) = input.find("--skill") {
        let url_raw = input[..idx].trim().trim_end_matches('/');
        let after   = input[idx + "--skill".len()..].trim();
        let skill   = after.split_whitespace().next().unwrap_or("").to_string();
        (url_raw, if skill.is_empty() { None } else { Some(skill) })
    } else {
        (input.trim_end_matches('/'), None)
    };
    // ── Pre-process common bad pasted inputs (like `git clone url target`) ──
    let mut url_str = url_part.trim();
    if url_str.starts_with("git clone") {
        url_str = url_str.trim_start_matches("git clone").trim();
    }
    // Taking the first token isolates the URL, ignoring trailing paths
    let url = url_str.split_whitespace().next().unwrap_or("").trim_end_matches(".git");

    // ── skills.sh page URL → GitHub ──────────────────────────────────────────
    let url: String = if url.starts_with("https://skills.sh/") || url.starts_with("http://skills.sh/") {
        let path = url
            .trim_start_matches("https://skills.sh/")
            .trim_start_matches("http://skills.sh/");
        let parts: Vec<&str> = path.splitn(3, '/').collect();
        if parts.len() < 2 {
            return Err(format!("Cannot parse skills.sh URL: {}", url));
        }
        let owner = parts[0];
        let repo  = parts[1];
        if let Some(skill) = explicit_skill.as_deref().or_else(|| parts.get(2).copied()) {
            format!("https://github.com/{}/{}/tree/main/skills/{}", owner, repo, skill)
        } else {
            format!("https://github.com/{}/{}", owner, repo)
        }
    } else {
        url.to_string()
    };

    // ── Arbitrary HTTP Direct URLs ───────────────────────────────────────────
    if (!url.starts_with("https://github.com/") && !url.starts_with("http://github.com/")) && url.starts_with("http") {
        let domain_part = url.split("://").nth(1).unwrap_or("custom").split('/').next().unwrap_or("custom").replace(".", "_");
        let name = explicit_skill.clone().unwrap_or(domain_part);
        return Ok(vec![(name, url)]);
    }

    // ── GitHub URL ───────────────────────────────────────────────────────────
    let mut url = url.to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() == 2 && !url.contains(' ') {
            url = format!("https://github.com/{}", url);
        }
    }

    if !url.starts_with("https://github.com/") && !url.starts_with("http://github.com/") {
        return Err(
            "Please paste a GitHub URL (https://github.com/owner/repo), \
             a skills.sh URL, a direct HTTP link, or github owner/repo format."
                .to_string(),
        );
    }

    let path = url
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/");
    let parts: Vec<&str> = path.splitn(6, '/').collect();

    if parts.len() < 2 {
        return Err("Invalid GitHub URL: need at least owner/repo".to_string());
    }

    let owner = parts[0];
    let repo  = parts[1];

    // https://github.com/owner/repo/tree/<branch>/path/to/skill
    if parts.len() >= 5 && parts[2] == "tree" {
        let branch     = parts[3];
        let sub_path   = parts[4..].join("/");
        let skill_name = parts.last().copied().unwrap_or(repo);
        let raw_url    = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}/SKILL.md",
            owner, repo, branch, sub_path
        );
        return Ok(vec![(skill_name.to_string(), raw_url)]);
    }

    // If user said --skill explicitly, use that
    if let Some(skill) = explicit_skill {
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/main/skills/{}/SKILL.md",
            owner, repo, skill
        );
        return Ok(vec![(skill, raw_url)]);
    }

    // https://github.com/owner/repo  — root-level SKILL.md
    let raw_url = format!(
        "https://raw.githubusercontent.com/{}/{}/main/SKILL.md",
        owner, repo
    );
    Ok(vec![(repo.to_string(), raw_url)])
}

async fn install_skill(
    State(state): State<AppState>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<Json<InstallSkillResponse>, AppError> {
    use tokio::fs;

    // resolve_skill_urls returns (skill_name, primary_raw_url)
    let pairs = resolve_skill_urls(&req.url)
        .map_err(|e| AppError(anyhow::anyhow!("{}", e)))?;

    let http = reqwest::Client::builder()
        .user_agent("aimaxxing-gateway/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError(anyhow::anyhow!("HTTP client error: {}", e)))?;

    let mut installed_names = Vec::new();

    for (skill_name, primary_url) in &pairs {
        // Build candidate URLs to try in order.
        //
        // skills.sh prefixes skill names with the repo owner's first word:
        //   owner "aimaxxing-labs" → prefix "aimaxxing"
        //   --skill "aimaxxing-react-native-skills" → actual dir "react-native-skills"
        //
        // Directory conventions:
        //   1. skills/<skill-name>
        //   2. .claude/skills/<skill-name>
        //   3. <skill-name> (root)
        let mut candidates: Vec<String> = {
            let prefix = "https://raw.githubusercontent.com/";
            if let Some(path) = primary_url.strip_prefix(prefix) {
                let parts: Vec<&str> = path.splitn(4, '/').collect();
                if parts.len() >= 3 {
                    let base  = format!("{}{}/{}/{}", prefix, parts[0], parts[1], parts[2]);
                    let owner = parts[0];

                    let owner_short = owner.split('-').next().unwrap_or(owner);
                    let stripped = skill_name
                        .strip_prefix(&format!("{}-", owner_short))
                        .unwrap_or(skill_name.as_str());

                    let mut c = Vec::new();
                    
                    if primary_url.ends_with(".md") {
                        c.push(primary_url.clone());
                        if primary_url.contains("/main/") {
                            c.push(primary_url.replace("/main/", "/master/"));
                        }
                    }

                    let test_dirs = ["skills", ".claude/skills", ""];

                    for dir in test_dirs {
                        let b = if dir.is_empty() { base.clone() } else { format!("{}/{}", base, dir) };
                        c.push(format!("{}/{}/SKILL.md", b, skill_name));
                        if stripped != skill_name.as_str() {
                            c.push(format!("{}/{}/SKILL.md", b, stripped));
                        }
                        
                        // Fallback to master if main was guessed
                        if b.contains("/main") {
                            let b_master = b.replace("/main", "/master");
                            c.push(format!("{}/{}/SKILL.md", b_master, skill_name));
                            if stripped != skill_name.as_str() {
                                c.push(format!("{}/{}/SKILL.md", b_master, stripped));
                            }
                        }
                    }
                    c
                } else {
                    vec![primary_url.clone()]
                }
            } else {
                let mut c = Vec::new();
                // For raw HTTP/HTTPS URLs that aren't raw.github
                if primary_url.ends_with("SKILL.md") || primary_url.ends_with(".md") {
                    c.push(primary_url.clone());
                } else {
                    let trim_url = primary_url.trim_end_matches("/");
                    c.push(format!("{}/SKILL.md", trim_url));
                    c.push(format!("{}/main/SKILL.md", trim_url));
                }
                c
            }
        };
        // Always ensure the precise string passed from resolver is available
        if !candidates.contains(primary_url) {
            candidates.push(primary_url.clone());
        }

        // Try each candidate URL until one returns 200
        let mut found_contents = Vec::new();
        let mut tried_urls = Vec::new();
        for url in &candidates {
            tried_urls.push(url.clone());
            match http.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.text().await {
                        Ok(text) => {
                            found_contents.push((skill_name.clone(), text));
                            break;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to read body from {}: {}", url, e);
                        }
                    }
                }
                Ok(resp) => {
                    tracing::debug!("Candidate {} → {}", url, resp.status());
                }
                Err(e) => {
                    tracing::warn!("Fetch error for {}: {}", url, e);
                }
            }
        }

        // ── 🌟 DEEP SEARCH FALLBACK (一劳永逸) ──
        if found_contents.is_empty() {
            if let Some(path) = primary_url.strip_prefix("https://raw.githubusercontent.com/") {
                let parts: Vec<&str> = path.splitn(4, '/').collect();
                if parts.len() >= 3 {
                    let owner = parts[0];
                    let repo = parts[1];
                    let branch = parts[2];
                    
                    let api_url = format!("https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1", owner, repo, branch);
                    tracing::info!("Falling back to deep tree search: {}", api_url);
                    
                    // Always inject a User-Agent for GitHub api
                    if let Ok(resp) = http.get(&api_url).header("User-Agent", "aimaxxing/1.0").send().await {
                        if resp.status().is_success() {
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                if let Some(tree) = json.get("tree").and_then(|t| t.as_array()) {
                                    let target_suffix = format!("{}/SKILL.md", skill_name);
                                    let mut exact_url = None;
                                    let mut all_skill_urls = Vec::new();
                                    
                                    for item in tree {
                                        if let Some(path_str) = item.get("path").and_then(|p| p.as_str()) {
                                            if path_str.ends_with(&target_suffix) || (path_str == "SKILL.md" && skill_name == repo) {
                                                exact_url = Some(format!("https://raw.githubusercontent.com/{}/{}/{}/{}", owner, repo, branch, path_str));
                                            } else if path_str.ends_with("SKILL.md") {
                                                // Try to guess the directory name
                                                let path_parts: Vec<&str> = path_str.split('/').collect();
                                                let guessed_name = if path_parts.len() >= 2 {
                                                    path_parts[path_parts.len() - 2].to_string()
                                                } else {
                                                    repo.to_string()
                                                };
                                                all_skill_urls.push((guessed_name, format!("https://raw.githubusercontent.com/{}/{}/{}/{}", owner, repo, branch, path_str)));
                                            }
                                        }
                                    }
                                    
                                    let target_urls = if let Some(url) = exact_url {
                                        vec![(skill_name.clone(), url)]
                                    } else {
                                        all_skill_urls
                                    };

                                    for (name, f_url) in target_urls {
                                        tracing::info!("Deep search found SKILL.md at: {}", f_url);
                                        tried_urls.push(format!("Deep Search: {}", f_url));
                                        if let Ok(file_resp) = http.get(&f_url).send().await {
                                            if file_resp.status().is_success() {
                                                if let Ok(text) = file_resp.text().await {
                                                    found_contents.push((name, text));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            tracing::warn!("Deep search API call failed: {}", resp.status());
                        }
                    }
                }
            }
        }

        if found_contents.is_empty() {
            return Err(AppError(anyhow::anyhow!(
                "SKILL.md not found. Tried:\n{}\n\nCheck that the skill name and repo URL are correct.",
                tried_urls.join("\n")
            )));
        }

        for (target_name, content) in found_contents {
            // Validate it looks like a SKILL.md
            if !content.trim_start().starts_with("---") {
                tracing::warn!("Skipping {} because it lacks YAML frontmatter.", target_name);
                continue;
            }

            // Write to <skills_base>/<target_name>/SKILL.md
            let skill_dir = state.skills.base_path.join(target_name.as_str());
            if let Err(e) = fs::create_dir_all(&skill_dir).await {
                tracing::error!("Failed to create skill dir {}: {}", target_name, e);
                continue;
            }

            let skill_path = skill_dir.join("SKILL.md");
            if let Err(e) = fs::write(&skill_path, &content).await {
                tracing::error!("Failed to write SKILL.md for {}: {}", target_name, e);
                continue;
            }

            // Hot-reload: load the new skill and insert into the live registry
            match state.skills.load_skill(&skill_dir).await {
                Ok(skill) => {
                    let name = skill.name();
                    // If YAML skill name differs from the directory, rename the directory!
                    if name != target_name {
                        let new_dir = state.skills.base_path.join(&name);
                        if !new_dir.exists() {
                            if let Err(e) = fs::rename(&skill_dir, &new_dir).await {
                                tracing::warn!("Failed to rename skill dir to {}: {}", name, e);
                            } else {
                                // Load from the completely new location to update references
                                if let Ok(new_skill) = state.skills.load_skill(&new_dir).await {
                                    tracing::info!("Hot-loaded skill '{}' into registry", name);
                                    state.skills.skills.insert(name.clone(), std::sync::Arc::new(new_skill));
                                    installed_names.push(name);
                                    continue;
                                }
                            }
                        }
                    }

                    tracing::info!("Hot-loaded skill '{}' into registry", name);
                    state.skills.skills.insert(name.clone(), std::sync::Arc::new(skill));
                    installed_names.push(name);
                }
                Err(e) => {
                    tracing::warn!("Skill {} written but failed to hot-load: {}", target_name, e);
                    installed_names.push(target_name);
                }
            }
        }
    }

    let summary = installed_names.join(", ");
    Ok(Json(InstallSkillResponse {
        success: true,
        skill_name: summary.clone(),
        message: format!("Installed: {}", summary),
    }))
}

// ── Uninstall Skill ────────────────────────────────────────────────────────
async fn uninstall_skill(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    // 1. Remove from dynamic loader map (memory)
    state.skills.skills.remove(&name);

    // 2. Remove from filesystem
    let skill_dir = state.skills.base_path.join(&name);
    if skill_dir.exists() {
        tokio::fs::remove_dir_all(&skill_dir)
            .await
            .map_err(|e| AppError(anyhow::anyhow!("Failed to delete skill directory: {}", e)))?;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Skill {} uninstalled", name)
    })))
}

// ── System Shutdown Handler ──────────────────────────────────────────────
async fn shutdown_handler() -> (StatusCode, Json<serde_json::Value>) {
    tracing::info!("Shutdown requested via API. Exiting in 1 second...");
    
    // We spawn a task to exit the process after a short delay, 
    // allowing the HTTP response to be sent back to the client.
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        std::process::exit(0);
    });

    (StatusCode::OK, Json(serde_json::json!({
        "success": true,
        "message": "Gateway shutdown initiated"
    })))
}

// ── System Sandboxes Handlers ──────────────────────────────────────────────
async fn get_active_sandboxes() -> Json<Vec<brain::skills::sandbox::ActiveSandboxContext>> {
    let mut sandboxes = Vec::new();
    for entry in brain::skills::sandbox::ACTIVE_SANDBOXES.iter() {
        sandboxes.push(entry.value().clone());
    }
    // Sort by started_at ascending
    sandboxes.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    Json(sandboxes)
}

async fn kill_sandbox(Path(pid): Path<u32>) -> (StatusCode, Json<serde_json::Value>) {
    // 1. Check if PID is in our registry
    if !brain::skills::sandbox::ACTIVE_SANDBOXES.contains_key(&pid) {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false,
            "message": format!("Sandbox PID {} not found or already terminated", pid)
        })));
    }

    // 2. Kill the PID
    #[cfg(target_os = "windows")]
    let output = std::process::Command::new("taskkill")
        .args(&["/F", "/PID", &pid.to_string()])
        .output();

    #[cfg(not(target_os = "windows"))]
    let output = std::process::Command::new("kill")
        .args(&["-9", &pid.to_string()])
        .output();

    // 3. Remove regardless of OS kill output since we assume it's dead
    brain::skills::sandbox::ACTIVE_SANDBOXES.remove(&pid);

    match output {
        Ok(o) if o.status.success() => {
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "message": format!("Successfully killed PID {}", pid)
            })))
        }
        _ => {
            // Might have died legitimately just before we ran kill
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "message": format!("PID {} removed from registry (OS kill may have failed/it was already ded)", pid)
            })))
        }
    }
}

// ── System Doctor Handler ──────────────────────────────────────────────
#[derive(Serialize)]
struct DoctorCheckResult {
    name: String,
    success: bool,
    message: String,
}

async fn doctor_api_handler() -> Json<Vec<DoctorCheckResult>> {
    let mut results = Vec::new();

    // 1. Sandbox Check
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("bwrap").arg("--version").output();
        match output {
            Ok(o) if o.status.success() => results.push(DoctorCheckResult {
                name: "Native Sandbox (bwrap)".to_string(),
                success: true,
                message: "Bubblewrap installed and functional".to_string(),
            }),
            _ => results.push(DoctorCheckResult {
                name: "Native Sandbox (bwrap)".to_string(),
                success: false,
                message: "Bubblewrap NOT found. Required for Linux isolation.".to_string(),
            }),
        }
    }
    #[cfg(target_os = "macos")]
    {
        results.push(DoctorCheckResult {
            name: "Native Sandbox (Seatbelt)".to_string(),
            success: true,
            message: "macOS sandbox-exec is available".to_string(),
        });
    }
    #[cfg(target_os = "windows")]
    {
        results.push(DoctorCheckResult {
            name: "Native Sandbox (Job Objects)".to_string(),
            success: true,
            message: "Windows Job Objects are natively supported".to_string(),
        });
    }

    // 2. Ollama Check
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();
    
    let ollama_url = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
    match client.get(format!("{}/api/tags", ollama_url)).send().await {
        Ok(resp) if resp.status().is_success() => results.push(DoctorCheckResult {
            name: "Local LLM (Ollama)".to_string(),
            success: true,
            message: "Ollama is running and accessible".to_string(),
        }),
        _ => results.push(DoctorCheckResult {
            name: "Local LLM (Ollama)".to_string(),
            success: false,
            message: "Ollama not detected at default port 11434".to_string(),
        }),
    }

    // 3. Vector DB Check
    let db_path = std::env::current_dir().unwrap_or_default().join("engram.db");
    if db_path.exists() {
         results.push(DoctorCheckResult {
            name: "Memory Store (SQLite)".to_string(),
            success: true,
            message: format!("Database exists at {:?}", db_path),
        });
    } else {
         results.push(DoctorCheckResult {
            name: "Memory Store (SQLite)".to_string(),
            success: false,
            message: "Database file not found".to_string(),
        });
    }

    Json(results)
}
async fn get_persona_templates(State(state): State<AppState>) -> Json<Vec<crate::PersonaTemplate>> {
    Json(state.persona_templates.clone())
}
