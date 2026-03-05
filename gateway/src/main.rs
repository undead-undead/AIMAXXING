use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use aimaxxing_core::skills::SkillLoader;
use aimaxxing_core::prelude::Tool;
use std::sync::Arc;


use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

use aimaxxing_gateway::mcp;
use aimaxxing_gateway::api;
use aimaxxing_core::agent::multi_agent::{Coordinator, AgentRole};
use aimaxxing_engram::{HybridSearchEngine, HybridSearchConfig, HierarchicalRetriever};


// Providers and agent loading are now handled by AgentFactory in aimaxxing-gateway/src/api/factory.rs

use aimaxxing_gateway::{PersonaTemplate, PersonaConfig};

/// On first run, seed each agent role directory with a default SOUL.md template.
/// Only writes if SOUL.md does not exist — never overwrites user edits.
fn seed_default_personas(base_soul_path: &std::path::Path, config: &PersonaConfig) {
    const YAML_HEADER_COMMENT: &str = "---";

    for p in &config.personas {
        let role_dir = base_soul_path.join(&p.name);
        if !role_dir.exists() {
            let _ = std::fs::create_dir_all(&role_dir);
        }
        // Phase 12-B: Seed SOUL.md if it doesn't exist
        let soul_file = role_dir.join("SOUL.md");
        if !soul_file.exists() {
            let tools_yaml = p.tools.iter().map(|t| format!("  - {}", t)).collect::<Vec<_>>().join("\n");
            let content = format!(
                "{}\nprovider: {}\nmodel: {}\ntemperature: {}\ntools:\n{}\n# base_url: https://your-custom-endpoint.com/v1\n---\n{}",
                YAML_HEADER_COMMENT, p.provider, p.model, p.temperature, tools_yaml, p.body
            );
            let _ = std::fs::write(&soul_file, content);
            info!("[first-run] Created default soul: {}", soul_file.display());
        }
    }
}

mod doctor;
mod onboard;

#[derive(Parser)]
#[command(name = "aimaxxing-gw")]
#[command(about = "AIMAXXING AI Gateway - Lightweight tool execution engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available skills
    List,
    /// Run diagnostic checks
    Doctor,
    /// Run onboarding wizard
    Onboard,
    /// Run a specific skill
    Run {
        /// Name of the skill to run
        name: String,
        /// JSON arguments for the skill
        #[arg(default_value = "{}")]
        args: String,
    },
    /// Start the gateway server (MCP)
    Serve,
    /// Start the HTTP API server
    Web {
        /// Port to listen on
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
        /// Explicitly choose the LLM provider (openai, anthropic, gemini, deepseek, minimax)
        #[arg(long)]
        provider: Option<String>,
        /// Explicitly choose the model name
        #[arg(long)]
        model: Option<String>,
    },
}

// Log Writer for Broadcast Channel
struct ChannelWriter(broadcast::Sender<String>);
impl std::io::Write for ChannelWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let msg = String::from_utf8_lossy(buf).to_string();
        let _ = self.0.send(msg);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Default paths
    let base_dir = std::env::current_dir()?;
    let logs_dir = base_dir.join("logs");
    if !logs_dir.exists() { let _ = std::fs::create_dir_all(&logs_dir); }

    // Initialize Logging with Broadcast capability
    let (log_tx, _) = broadcast::channel(100);
    let log_tx_clone = log_tx.clone();

    // 1. Line-Capped File Logger (10,000 entries)
    // We implement a custom writer that counts lines to ensure disk safety
    struct LineCappedWriter {
        path: std::path::PathBuf,
        old_path: std::path::PathBuf,
        file: std::fs::File,
        count: usize,
        max_lines: usize,
    }

    impl LineCappedWriter {
        fn new(path: std::path::PathBuf, max_lines: usize) -> Self {
            let old_path = path.with_extension("log.old");
            // Count existing lines on start
            let count = std::fs::read_to_string(&path)
                .map(|s| s.lines().count())
                .unwrap_or(0);

            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("Failed to open log file");

            Self { path, old_path, file, count, max_lines }
        }
    }

    impl std::io::Write for LineCappedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            // Basic line counting
            let new_lines = buf.iter().filter(|&&b| b == b'\n').count();
            self.count += new_lines;

            if self.count >= self.max_lines {
                // Rotate: close, rename, reopen
                let _ = std::fs::rename(&self.path, &self.old_path);
                self.file = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&self.path)?;
                self.count = 0;
            }
            self.file.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> { self.file.flush() }
    }

    let log_writer = LineCappedWriter::new(logs_dir.join("gateway.log"), 10000);
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(log_writer);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_file)
        .with_ansi(false)
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

    // 2. Stdout Logger
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);
    
    // 3. Broadcast Layer for frontend streaming
    let channel_layer = tracing_subscriber::fmt::layer()
        .with_writer(move || ChannelWriter(log_tx_clone.clone()))
        .json()
        .flatten_event(true)
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .with(channel_layer)
        .init();

    info!("Logging initialized. Logs are stored in: {}", logs_dir.display());

    let cli = Cli::parse();

    // Default paths
    let skills_path = base_dir.join("skills");
    let envs_path = base_dir.join(".aimaxxing_envs");

    let env_manager = Arc::new(aimaxxing_core::env::EnvManager::new(envs_path));
    let loader = Arc::new(SkillLoader::new(skills_path).with_env_manager(env_manager));
    let loader_init = {
        let l = Arc::clone(&loader);
        tokio::spawn(async move {
            l.load_all().await
        })
    };

    match cli.command {
        Commands::List => {
            loader_init.await??;
            println!("Available Skills:");
            for skill in loader.skills.iter() {
                println!("- {}: {}", skill.key(), skill.value().metadata().description);
            }
        }
        Commands::Doctor => {
            if let Err(e) = doctor::run_doctor().await {
                eprintln!("Doctor check failed: {}", e);
            }
        }
        Commands::Onboard => {
            if let Err(e) = onboard::run_onboard().await {
                eprintln!("Onboarding failed: {}", e);
            }
        }
        Commands::Run { name, args } => {
            loader_init.await??;
            if let Some(skill) = loader.skills.get(&name) {
                info!("Running skill: {}", name);
                let result = skill.call(&args).await?;
                println!("{}", result);
            } else {
                eprintln!("Skill '{}' not found", name);
            }
        }
        Commands::Serve => {
            loader_init.await??;
            info!("Starting AIMAXXING Gateway MCP Server...");
            let server = mcp::McpServer::new(Arc::clone(&loader));
            server.run().await?;
        }
        Commands::Web { port, provider: _, model: _ } => {
            let config_path = base_dir.join("aimaxxing.yaml");
            let mut app_config = aimaxxing_core::config::AppConfig::load_from_file(&config_path)?;
            
            // CLI arg overrides config
            if port != 3000 || app_config.server.port == 0 {
                app_config.server.port = port;
            }

            use aimaxxing_core::auth::{OAuthManager, FileTokenStore};

            // Initialize OAuth manager
            let token_store = Arc::new(FileTokenStore::new(base_dir.join(".aimaxxing_tokens.json")));
            let mut oauth_manager = OAuthManager::new(token_store);
            
            // Register OAuth aimaxxing_providers (Google example)
            let google_id = std::env::var("GOOGLE_CLIENT_ID").ok();
            let google_secret = std::env::var("GOOGLE_CLIENT_SECRET").ok();
            if let (Some(id), Some(secret)) = (google_id, google_secret) {
                let config = aimaxxing_core::auth::OAuthConfig {
                    client_id: id,
                    client_secret: secret,
                    auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                    token_url: "https://oauth2.googleapis.com/token".to_string(),
                    redirect_url: format!("http://localhost:{}/api/auth/google/callback", app_config.server.port),
                    scopes: vec!["openid".to_string(), "email".to_string(), "profile".to_string()],
                };
                let _ = oauth_manager.register_provider("google", config);
            }

            let shared_config = Arc::new(parking_lot::RwLock::new(app_config.clone()));
            let enabled_tools = {
                let mut config = shared_config.write();
                if config.skills.enabled.is_empty() {
                    info!("First run detected. Enabling default skills...");
                    config.skills.enabled.insert("browser_browse".to_string());
                    let _ = config.save_to_file(&config_path);
                }
                Arc::new(parking_lot::RwLock::new(config.skills.enabled.clone()))
            };

            let oauth_manager = Arc::new(oauth_manager);
            
            let data_dir = base_dir.join("data");
            let engram_config = HybridSearchConfig {
                db_path: data_dir.join("aimaxxing_engram.db"),
                ..Default::default()
            };
            
            info!("Initializing search engine in background...");
            let engram_init = tokio::task::spawn_blocking(move || {
                HybridSearchEngine::new(engram_config)
            });

            // Wait for both Skill Loader and Search Engine
            let (loader_res, engram_res) = tokio::join!(loader_init, engram_init);
            loader_res??;
            let knowledge_engine = Arc::new(engram_res??);
            
            let retriever = Arc::new(HierarchicalRetriever::new(knowledge_engine.clone()));

            // Swarm & Agents Initialization
            let coordinator = Arc::new(Coordinator::new());
            let base_soul_path = app_config.soul_path.clone().unwrap_or_else(|| base_dir.join("soul"));
            let heartbeat_path = app_config.heartbeat_path.clone().unwrap_or_else(|| base_dir.join("HEARTBEAT.md"));
            
            if !base_soul_path.exists() { let _ = std::fs::create_dir_all(&base_soul_path); }

            // Load persona templates from file or use defaults
            let persona_config = {
                let p_yaml = base_dir.join("personas.yaml");
                if p_yaml.exists() {
                    match std::fs::read_to_string(&p_yaml) {
                        Ok(content) => {
                            match serde_yaml_ng::from_str::<PersonaConfig>(&content) {
                                Ok(cfg) => cfg,
                                Err(e) => {
                                    tracing::error!("Failed to parse personas.yaml: {}. Using internal defaults.", e);
                                    get_default_personas()
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to read personas.yaml: {}. Using internal defaults.", e);
                            get_default_personas()
                        }
                    }
                } else {
                    get_default_personas()
                }
            };

            // First-run seeding: create default SOUL.md for each role if missing
            seed_default_personas(&base_soul_path, &persona_config);

            // Initialize Agent Factory
            let factory = Arc::new(aimaxxing_gateway::api::factory::AgentFactory::new(
                shared_config.clone(),
                loader.clone(),
                coordinator.clone(),
                retriever.clone(),
                base_dir.clone(),
                enabled_tools.clone(),
            ));

            // Load all agents from the soul directory in parallel
            if let Ok(mut entries) = tokio::fs::read_dir(&base_soul_path).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if entry.path().is_dir() {
                        let role_name = entry.file_name().to_string_lossy().to_string();
                        let factory_clone = factory.clone();
                        tokio::spawn(async move {
                            if let Err(e) = factory_clone.reload_agent(&role_name).await {
                                tracing::error!("Failed to load agent '{}': {}", role_name, e);
                            }
                        });
                    }
                }
            }

            // Start Heartbeat Watcher
            if let Some(hb_agent) = coordinator.get(&AgentRole::Assistant) {
                let hb_path_clone = heartbeat_path.clone();
                tokio::spawn(async move {
                    let watcher = aimaxxing_core::agent::heartbeat::HeartbeatWatcher::new(hb_agent, hb_path_clone, 30);
                    watcher.run().await;
                });
            } else {
                tracing::warn!("Assistant agent not found in coordinator, HeartbeatWatcher disabled.");
            }

            api::server::start_server(
                Arc::clone(&loader), coordinator, oauth_manager, shared_config,
                enabled_tools, config_path, heartbeat_path, log_tx,
                knowledge_engine, retriever, factory, persona_config.personas,
            ).await?;
        }
    }

    Ok(())
}

fn get_default_personas() -> PersonaConfig {
    PersonaConfig {
        personas: vec![
            PersonaTemplate {
                name: "assistant".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                temperature: 0.7,
                tools: vec!["fs".to_string(), "knowledge".to_string(), "git".to_string(), "data".to_string(), "notify".to_string()],
                body: "\n# Assistant\n\nYou are the primary conversational agent. Be precise, technical, and concise.\nNo filler phrases. Ask for clarification when needed rather than guessing.\n".to_string(),
            },
            PersonaTemplate {
                name: "researcher".to_string(),
                provider: "deepseek".to_string(),
                model: "deepseek-chat".to_string(),
                temperature: 0.3,
                tools: vec!["fs".to_string(), "ocr".to_string(), "data".to_string()],
                body: "\n# Researcher\n\nYou are an information retrieval and analysis specialist.\nOnly make claims supported by searched sources. Annotate uncertainty.\nOutput in the form: Conclusion -> Evidence -> Source.\n".to_string(),
            },
            PersonaTemplate {
                name: "evo".to_string(),
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                temperature: 0.9,
                tools: vec!["fs".to_string(), "chart".to_string(), "crypto".to_string()],
                body: "\n# Evo\n\nYou are the creative and divergent-thinking engine.\nAlways generate multiple solution candidates before evaluating.\nBe bold, self-questioning, and explicit about your assumptions.\n".to_string(),
            },
        ],
    }
}
