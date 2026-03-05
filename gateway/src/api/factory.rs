use std::sync::Arc;
use std::path::PathBuf;
use anyhow::{Result, anyhow};
use tracing::info;

use aimaxxing_core::agent::Agent;
use aimaxxing_core::agent::multi_agent::{AgentRole, Coordinator};
use aimaxxing_core::agent::provider::Provider;
use aimaxxing_core::config::{AppConfig, AgentConfigOverrides};
use aimaxxing_core::config::vault::{KeyringVault, SecretVault};
use aimaxxing_providers;
use aimaxxing_core::skills::tool::filesystem::{ReadFileTool, WriteFileTool, ListDirTool, EditFileTool};
use aimaxxing_core::skills::tool::{
    GitOpsTool, ChartTool, MailerTool, DataTransformTool, 
    NotifierTool, CipherTool, TextExtractTool, TranscribeTool, SpeakTool
};
use aimaxxing_engram::KnowledgeSearchTool;
use aimaxxing_engram::HierarchicalRetriever;

/// Factory for creating agents based on soul configurations
pub struct AgentFactory {
    pub config: Arc<parking_lot::RwLock<AppConfig>>,
    pub loader: Arc<aimaxxing_core::skills::SkillLoader>,
    pub coordinator: Arc<Coordinator>,
    pub retriever: Arc<HierarchicalRetriever>,
    pub base_dir: PathBuf,
    pub enabled_tools: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
}

impl AgentFactory {
    pub fn new(
        config: Arc<parking_lot::RwLock<AppConfig>>,
        loader: Arc<aimaxxing_core::skills::SkillLoader>,
        coordinator: Arc<Coordinator>,
        retriever: Arc<HierarchicalRetriever>,
        base_dir: PathBuf,
        enabled_tools: Arc<parking_lot::RwLock<std::collections::HashSet<String>>>,
    ) -> Self {
        Self {
            config,
            loader,
            coordinator,
            retriever,
            base_dir,
            enabled_tools,
        }
    }

    /// Rebuild and re-register an agent for a given role
    pub async fn reload_agent(&self, role_name: &str) -> Result<()> {
        let role = match role_name.to_lowercase().as_str() {
            "assistant" => AgentRole::Assistant,
            "researcher" => AgentRole::Researcher,
            "trader" => AgentRole::Trader,
            "risk_analyst" => AgentRole::RiskAnalyst,
            "strategist" => AgentRole::Strategist,
            _ => AgentRole::Custom(role_name.to_string()),
        };

        let soul_path = self.config.read().soul_path.clone()
            .unwrap_or_else(|| self.base_dir.join("soul"))
            .join(role_name);

        let overrides = self.read_soul_overrides(&soul_path);
        let agent = self.build_agent(role, soul_path, overrides).await?;
        
        self.coordinator.register(Arc::new(agent));
        info!("Reloaded agent for role: {}", role_name);
        
        Ok(())
    }

    fn read_soul_overrides(&self, soul_path: &std::path::Path) -> AgentConfigOverrides {
        let soul_file = soul_path.join("SOUL.md");
        
        if soul_file.exists() {
            match std::fs::read_to_string(&soul_file) {
                Ok(content) => {
                    let (ovr, _) = AgentConfigOverrides::parse_frontmatter(&content);
                    ovr
                }
                Err(_) => AgentConfigOverrides::default(),
            }
        } else {
            AgentConfigOverrides::default()
        }
    }

    async fn build_agent(
        &self,
        role: AgentRole,
        soul_path: PathBuf,
        ovr: AgentConfigOverrides,
    ) -> Result<Agent<Arc<dyn Provider>>> {
        let app_cfg = self.config.read();
        
        // 1. Resolve Provider
        let provider_name = ovr.provider.clone()
            .or_else(|| app_cfg.aimaxxing_providers.active_provider.clone())
            .unwrap_or_else(|| "openai".to_string());

        let vault = KeyringVault::new("aimaxxing");
        
        // Try to get API key from vault, then env, then config
        let api_key = match vault.get(&format!("{}_API_KEY", provider_name.to_uppercase())) {
            Ok(Some(key)) => Some(key),
            _ => {
                let from_cfg = match provider_name.as_str() {
                    "openai" => app_cfg.aimaxxing_providers.openai_api_key.clone(),
                    "anthropic" => app_cfg.aimaxxing_providers.anthropic_api_key.clone(),
                    "gemini" => app_cfg.aimaxxing_providers.gemini_api_key.clone(),
                    "deepseek" => app_cfg.aimaxxing_providers.deepseek_api_key.clone(),
                    "minimax" => app_cfg.aimaxxing_providers.minimax_api_key.clone(),
                    _ => None,
                };
                from_cfg.or_else(|| std::env::var(format!("{}_API_KEY", provider_name.to_uppercase())).ok())
            }
        };

        let provider = aimaxxing_providers::create_provider(
            &provider_name,
            ovr.base_url.clone(),
            api_key.clone(),
        ).map_err(|e| anyhow!("Failed to create provider '{}': {}", provider_name, e))?;

        // 2. Build Agent
        let mut builder = Agent::builder(provider.clone())
            .role(role)
            .with_delegation(self.coordinator.clone())
            .with_handover(self.coordinator.clone())
            .with_dynamic_skills(self.loader.clone())?
            .with_enabled_tools(self.enabled_tools.clone())
            .soul_path(soul_path);

        // 3. Add Core Tools based on overrides or defaults
        let configured_tools = ovr.tools.clone().unwrap_or_else(|| {
            vec!["fs".to_string()]
        });

        for tool_name in configured_tools {
            match tool_name.as_str() {
                "fs" => {
                    builder = builder
                        .tool(ReadFileTool::new(self.base_dir.clone()))
                        .tool(WriteFileTool::new(self.base_dir.clone()))
                        .tool(ListDirTool::new(self.base_dir.clone()))
                        .tool(EditFileTool::new(self.base_dir.clone()));
                }
                "knowledge" => {
                    builder = builder.tool(KnowledgeSearchTool::new(self.retriever.clone()));
                }
                "git" => {
                    builder = builder.tool(GitOpsTool);
                }
                "chart" => {
                    builder = builder.tool(ChartTool);
                }
                "mailer" => {
                    builder = builder.tool(MailerTool);
                }
                "data" => {
                    builder = builder.tool(DataTransformTool);
                }
                "ocr" => {
                    builder = builder.tool(TextExtractTool::new(Some(Arc::clone(&provider)), ovr.model.clone()));
                }
                "crypto" => {
                    builder = builder.tool(CipherTool);
                }
                "notify" => {
                    builder = builder.tool(NotifierTool);
                }
                "voice" => {
                    if let Some(ref key) = api_key {
                        builder = builder
                            .tool(TranscribeTool::new(key.clone(), ovr.base_url.clone()))
                            .tool(SpeakTool::new(key.clone(), ovr.base_url.clone(), self.base_dir.clone()));
                    }
                }
                _ => {}
            }
        }

        // 4. Overrides for Model/Temp
        if let Some(m) = ovr.model {
            builder = builder.model(m);
        } else if provider_name == "ollama" {
            // Default model for Ollama if not specified
            builder = builder.model("llama3".to_string());
        }

        if let Some(t) = ovr.temperature {
            builder = builder.temperature(t as f64);
        }

        builder.build().map_err(anyhow::Error::from)
    }
}
