pub mod vault;

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub providers: ProviderConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    #[cfg(not(target_arch = "wasm32"))]
    pub persona: Option<crate::agent::personality::Persona>,
    #[serde(default)]
    pub connectors: ConnectorsConfig,
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
    /// Path to a folder containing .md files for persona "soul" injection
    pub soul_path: Option<PathBuf>,
    /// Path to HEARTBEAT.md for autonomous tasks
    pub heartbeat_path: Option<PathBuf>,
    /// Per-agent specific overrides (e.g., model, provider)
    #[serde(default)]
    pub agents: std::collections::HashMap<String, AgentConfigOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfigOverrides {
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub tools: Option<Vec<String>>,
}

impl AgentConfigOverrides {
    pub fn parse_frontmatter(content: &str) -> (Self, String) {
        if content.starts_with("---\n") || content.starts_with("---\r\n") {
            if let Some(end_idx) = content[4..].find("\n---") {
                let end_full = end_idx + 4;
                let yaml_str = &content[4..end_full];
                if let Ok(config) = serde_yaml_ng::from_str::<Self>(yaml_str) {
                    let mut rest = content[end_full + 4..].to_string();
                    if rest.starts_with('\n') {
                        rest.remove(0);
                    }
                    return (config, rest);
                }
            }
        }
        (Self::default(), content.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectorsConfig {
    pub telegram: Option<TelegramConfig>,
    pub discord: Option<DiscordConfig>,
    pub feishu: Option<FeishuConfig>,
    pub dingtalk: Option<DingTalkConfig>,
    pub slack: Option<SlackConfig>,
    pub im: Option<BarkConfig>, // iMessage support via Bark (iOS)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BarkConfig {
    pub server_url: String, // e.g., https://api.day.app
    pub device_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub allowed_chat_ids: Vec<String>, // Whitelist for security
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    pub bot_token: String,
    pub channel_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeishuConfig {
    pub app_id: String,
    pub app_secret: String,
    pub verification_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DingTalkConfig {
    pub app_key: String,
    pub app_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlackConfig {
    pub bot_token: String,
    pub verification_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsConfig {
    #[serde(default)]
    pub enabled: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    pub enable_vector: bool,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            enable_vector: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub active_provider: Option<String>,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
    pub deepseek_api_key: Option<String>,
    pub minimax_api_key: Option<String>,
    #[serde(default)]
    pub custom_providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "0.0.0.0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig {
    pub data_dir: Option<PathBuf>,
}

impl AppConfig {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut content = std::fs::read_to_string(path)?;

        // Phase 1: Basic environment variable expansion for ${VAR} pattern
        // This handles the primitive case mentioned in the example.
        content = expand_env_vars(&content);

        let mut config: Self = serde_yaml_ng::from_str(&content)
            .map_err(|e| crate::error::Error::Internal(format!("Failed to parse config: {}", e)))?;

        // Phase 2: Resolve vault:// references using system keychain/env
        let vault = vault::CompositeVault::default_system();
        config.resolve_secrets(&vault)?;

        Ok(config)
    }

    pub fn resolve_secrets(&mut self, vault: &dyn vault::SecretVault) -> Result<()> {
        // Resolve provider keys
        self.providers.openai_api_key = resolve_one(self.providers.openai_api_key.take(), vault)?;
        self.providers.anthropic_api_key =
            resolve_one(self.providers.anthropic_api_key.take(), vault)?;
        self.providers.gemini_api_key = resolve_one(self.providers.gemini_api_key.take(), vault)?;
        self.providers.deepseek_api_key =
            resolve_one(self.providers.deepseek_api_key.take(), vault)?;
        self.providers.minimax_api_key = resolve_one(self.providers.minimax_api_key.take(), vault)?;

        // Resolve connector tokens
        if let Some(tg) = &mut self.connectors.telegram {
            tg.bot_token = resolve_one(Some(tg.bot_token.clone()), vault)?.unwrap_or_default();
        }
        if let Some(ds) = &mut self.connectors.discord {
            ds.bot_token = resolve_one(Some(ds.bot_token.clone()), vault)?.unwrap_or_default();
        }
        if let Some(sl) = &mut self.connectors.slack {
            sl.bot_token = resolve_one(Some(sl.bot_token.clone()), vault)?.unwrap_or_default();
        }

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_yaml_ng::to_string(self).map_err(|e| {
            crate::error::Error::Internal(format!("Failed to serialize config: {}", e))
        })?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::vault::SecretVault;

    struct MockVault;
    impl SecretVault for MockVault {
        fn get(&self, key: &str) -> Result<Option<String>> {
            if key == "SECRET_API_KEY" {
                Ok(Some("mocked-secret-key".to_string()))
            } else {
                Ok(None)
            }
        }
    }

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_VAR", "hello-world");
        let content = "api_key = \"${TEST_VAR}\"";
        let expanded = expand_env_vars(content);
        assert_eq!(expanded, "api_key = \"hello-world\"");
    }

    #[test]
    fn test_resolve_secrets() {
        let mut config = AppConfig::default();
        config.providers.openai_api_key = Some("vault://SECRET_API_KEY".to_string());

        config.resolve_secrets(&MockVault).unwrap();

        assert_eq!(
            config.providers.openai_api_key.unwrap(),
            "mocked-secret-key"
        );
    }
}

fn resolve_one(value: Option<String>, vault: &dyn vault::SecretVault) -> Result<Option<String>> {
    match value {
        Some(s) if s.starts_with("vault://") => {
            let key = &s[8..];
            match vault.get(key)? {
                Some(secret) => Ok(Some(secret)),
                None => {
                    tracing::warn!("Vault key '{}' not found, using literal reference", key);
                    Ok(Some(s))
                }
            }
        }
        _ => Ok(value),
    }
}

fn expand_env_vars(content: &str) -> String {
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    re.replace_all(content, |caps: &regex::Captures| {
        let key = &caps[1];
        std::env::var(key).unwrap_or_else(|_| format!("${{{}}}", key))
    })
    .to_string()
}
