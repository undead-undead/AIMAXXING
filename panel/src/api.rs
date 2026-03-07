 //! HTTP client for communicating with a running aimaxxing-gw instance.
//! Supports both local (localhost:3000) and remote (Tailscale) endpoints.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

/// A single skill as returned by /api/skills
#[derive(Debug, Clone, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub runtime: Option<String>,
    pub homepage: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub dependencies: Vec<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelField {
    pub key: String,         
    pub label: String,       
    pub field_type: String,  
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelMetadata {
    pub id: String,          
    pub name: String,        
    pub description: String,
    pub icon: String,        
    pub fields: Vec<ChannelField>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderField {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub fields: Vec<ProviderField>,
    pub capabilities: Vec<String>,
    pub preferred_models: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSchemaResponse {
    pub providers: Vec<ProviderMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelSchemaResponse {
    pub channels: Vec<ChannelMetadata>,
    pub running: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActiveSandboxContext {
    pub pid: u32,
    pub tool_name: String,
    pub interpreter: String,
    pub started_at: std::time::SystemTime,
}

/// Health check response
#[derive(Debug, Clone, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub agent_count: Option<usize>,
}

/// Vault secret write request
#[derive(Debug, Serialize)]
pub struct VaultSecretRequest {
    pub key: String,
    pub value: String,
}

/// Metrics response
#[derive(Debug, Clone, Deserialize)]
pub struct Metrics {
    pub total_calls: Option<u64>,
    pub success_rate: Option<f64>,
    pub avg_latency_ms: Option<f64>,
    pub total_tokens: Option<u64>,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
}

/// Result of a single doctor check
#[derive(Debug, Clone, Deserialize)]
pub struct DoctorCheckResult {
    pub name: String,
    pub success: bool,
    pub message: String,
}

/// A skill from the AIMAXXING marketplace
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketSkill {
    pub name: String,
    pub description: String,
    pub source: String,
    pub author: String,
    pub url: String,
    pub version: Option<String>,
    pub stars: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketSearchResponse {
    pub results: Vec<MarketSkill>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PersonaTemplate {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub tools: Vec<String>,
    pub body: String,
}

/// The gateway API client.
#[derive(Clone)]
pub struct GatewayClient {
    pub base_url: String,
    client: Client,
}

impl GatewayClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        #[allow(unused_mut)]
        let mut builder = Client::builder();
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            builder = builder.timeout(Duration::from_secs(10));
        }

        let client = builder.build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url.into(),
            client,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Check if the gateway is reachable.
    pub async fn health(&self) -> Result<HealthStatus> {
        let url = format!("{}/health", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<HealthStatus>().await?)
    }

    /// Fetch channel metadata schema
    pub async fn get_channel_schema(&self) -> Result<ChannelSchemaResponse> {
        let url = format!("{}/api/channels/schema", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<ChannelSchemaResponse>().await?)
    }

    /// Fetch LLM provider metadata schema
    pub async fn get_provider_schema(&self) -> Result<ProviderSchemaResponse> {
        let url = format!("{}/api/providers/schema", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<ProviderSchemaResponse>().await?)
    }

    /// List all skills.
    pub async fn list_skills(&self) -> Result<Vec<SkillInfo>> {
        let url = format!("{}/api/skills", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<Vec<SkillInfo>>().await?)
    }

    /// Toggle a skill on/off.
    pub async fn toggle_skill(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/skills/{}/toggle", self.base_url, name);
        self.client.post(&url).send().await?;
        Ok(())
    }

    /// Uninstall a skill.
    pub async fn uninstall_skill(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/skills/{}", self.base_url, name);
        self.client.delete(&url).send().await?;
        Ok(())
    }


    /// Save a secret to the vault.
    pub async fn save_vault_secret(&self, key: &str, value: &str) -> Result<()> {
        let url = format!("{}/api/config/vault", self.base_url);
        self.client.post(&url)
            .json(&serde_json::json!({ "key": key, "value": value }))
            .send()
            .await?;
        Ok(())
    }

    pub async fn save_channel_config(&self, channel_id: &str, values: std::collections::HashMap<String, String>) -> Result<()> {
        let url = format!("{}/api/channels/config", self.base_url);
        self.client.post(&url)
            .json(&serde_json::json!({ "channel_id": channel_id, "values": values }))
            .send()
            .await?;
        Ok(())
    }

    /// Delete a secret from the vault.
    pub async fn delete_vault_secret(&self, key: &str) -> Result<()> {
        let url = format!("{}/api/config/vault/{}", self.base_url, key);
        self.client.delete(&url).send().await?;
        Ok(())
    }

    /// Fetch current metrics.
    pub async fn metrics(&self) -> Result<Metrics> {
        let url = format!("{}/api/metrics", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<Metrics>().await?)
    }

    /// Fetch recent log lines via SSE (one-shot poll, not streaming).
    /// Returns up to `limit` recent lines by reading the SSE stream for a moment.
    pub async fn poll_logs(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/logs/recent", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<Vec<String>>().await?)
    }

    /// Fetch persona templates from gateway
    pub async fn get_persona_templates(&self) -> Result<Vec<PersonaTemplate>> {
        let url = format!("{}/api/system/persona/templates", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<Vec<PersonaTemplate>>().await?)
    }

    /// Fetch full gateway configuration as JSON
    pub async fn get_config(&self) -> Result<serde_json::Value> {
        let url = format!("{}/api/config", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<serde_json::Value>().await?)
    }

    /// Update gateway configuration
    pub async fn update_config(&self, config: &serde_json::Value) -> Result<()> {
        let url = format!("{}/api/config", self.base_url);
        let resp = self.client.post(&url).json(config).send().await?;
        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            anyhow::bail!("Config update failed: {}", msg);
        }
        Ok(())
    }
}

/// A standalone HTTP client for fetching from ClawHub marketplace.
/// Base URL: https://clawhub.ai/api (hypothetical)
pub struct MarketClient {
    client: Client,
    base_url: String,
}

impl Default for MarketClient {
    fn default() -> Self {
        let client = Client::builder()
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: "https://api.github.com/repos/openclaw/skills/contents".to_string(),
        }
    }
}

impl MarketClient {
    /// Fetch featured/available skills from ClawHub GitHub repo index.
    pub async fn list_skills(&self) -> Result<Vec<MarketSkill>> {
        // Uses GitHub API to list SKILL.md files from the aimaxxing skills repository
        let url = format!("{}", self.base_url);
        let resp = self.client
            .get(&url)
            .header("User-Agent", "clawhub-panel/0.3")
            .send()
            .await?;
        
        // GitHub returns a JSON array of file/dir entries
        #[derive(Deserialize)]
        struct GhEntry {
            name: String,
            #[serde(rename = "type")]
            entry_type: String,
            html_url: Option<String>,
        }

        let entries: Vec<GhEntry> = resp.json().await?;
        
        // Convert directory entries to MarketSkill (each dir = one skill)
        let skills = entries
            .into_iter()
            .filter(|e| e.entry_type == "dir")
            .map(|e| MarketSkill {
                name: e.name.clone(),
                description: "Available from GitHub repository".to_string(),
                source: "github".to_string(),
                author: "openclaw".to_string(),
                url: e.html_url.unwrap_or_default(),
                version: Some("latest".to_string()),
                stars: None,
            })
            .collect();
        
        Ok(skills)
    }
}

// ── New data types for Cron / Sessions / Snapshot ────────────────────────────

/// A scheduled cron job
#[derive(Debug, Clone, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: serde_json::Value,
    pub payload_kind: String,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub error_count: u32,
}

/// Request body for creating a new cron job
#[derive(Debug, Serialize)]
pub struct CreateCronJobRequest {
    pub name: String,
    pub schedule_kind: String,
    pub interval_secs: Option<u64>,
    pub cron_expr: Option<String>,
    pub at: Option<String>,
    pub prompt: Option<String>,
}

/// An active session
#[derive(Debug, Clone, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub agent_role: String,
}

/// A connector's configured status
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorStatus {
    pub name: String,
    pub configured: bool,
}

/// Gateway snapshot
#[derive(Debug, Clone, Deserialize)]
pub struct GatewaySnapshot {
    pub status: String,
    pub version: String,
    pub agent_count: usize,
    pub skill_count: usize,
    pub cron_job_count: usize,
    pub connectors: Vec<ConnectorStatus>,
    #[serde(default)]
    pub custom_providers: Vec<String>,
    #[serde(default)]
    pub vault_keys: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    // Model Pooling (Phase 3.5)
    pub model_ram_usage_mb: usize,
    pub model_vram_usage_mb: usize,
    pub model_ram_limit_gb: u32,
    pub model_vram_limit_gb: u32,
}

/// A single log entry with level/subsystem/message (parsed from JSON line)
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LogEntry {
    pub level: Option<String>,
    pub message: Option<String>,
    /// tracing subsystem / target
    pub target: Option<String>,
    /// raw line if not parseable as JSON
    #[serde(skip)]
    pub raw: String,
}

impl LogEntry {
    pub fn from_raw(line: &str) -> Self {
        if let Ok(mut e) = serde_json::from_str::<LogEntry>(line) {
            e.raw = line.to_string();
            e
        } else {
            LogEntry { raw: line.to_string(), ..Default::default() }
        }
    }

    pub fn display_level(&self) -> &str {
        self.level.as_deref().unwrap_or("info")
    }

    pub fn display_message(&self) -> &str {
        if !self.raw.is_empty() && self.message.is_none() {
            &self.raw
        } else {
            self.message.as_deref().unwrap_or("")
        }
    }
}

impl GatewayClient {
    /// List cron jobs
    pub async fn list_cron_jobs(&self) -> Result<Vec<CronJob>> {
        let url = format!("{}/api/cron/jobs", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<Vec<CronJob>>().await?)
    }

    /// Create a cron job
    pub async fn create_cron_job(&self, req: CreateCronJobRequest) -> Result<CronJob> {
        let url = format!("{}/api/cron/jobs", self.base_url);
        let resp = self.client.post(&url).json(&req).send().await?;
        Ok(resp.json::<CronJob>().await?)
    }

    /// Delete a cron job
    pub async fn delete_cron_job(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/cron/jobs/{}", self.base_url, id);
        self.client.delete(&url).send().await?;
        Ok(())
    }

    /// Manually run a cron job
    pub async fn run_cron_job(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/cron/jobs/{}/run", self.base_url, id);
        self.client.post(&url).send().await?;
        Ok(())
    }

    /// List active sessions
    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let url = format!("{}/api/sessions", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<Vec<SessionInfo>>().await?)
    }

    /// Delete a session
    pub async fn delete_session(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/sessions/{}", self.base_url, url_encode(id));
        self.client.delete(&url).send().await?;
        Ok(())
    }

    /// Get gateway snapshot
    pub async fn get_snapshot(&self) -> Result<GatewaySnapshot> {
        let url = format!("{}/api/snapshot", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<GatewaySnapshot>().await?)
    }

    /// Install a skill from a GitHub or skills.sh URL
    pub async fn install_skill(&self, url: &str) -> Result<InstallSkillResponse> {
        let endpoint = format!("{}/api/skills/install", self.base_url);
        let body = serde_json::json!({ "url": url });
        let resp = self.client.post(&endpoint).json(&body).send().await?;
        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            anyhow::bail!("Install failed: {}", msg);
        }
        Ok(resp.json::<InstallSkillResponse>().await?)
    }

    pub async fn get_heartbeat(&self) -> Result<String> {
        let url = format!("{}/api/system/heartbeat", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let dto: FileDto = resp.json().await?;
        Ok(dto.content)
    }

    pub async fn search_market(&self, query: &str, page: u32) -> Result<Vec<MarketSkill>> {
        let url = format!("{}/api/market/search?query={}&page={}", self.base_url, url_encode(query), page);
        let resp = self.client.get(&url).send().await?;
        let res: MarketSearchResponse = resp.json().await?;
        Ok(res.results)
    }

    pub async fn put_heartbeat(&self, content: String) -> Result<()> {
        let url = format!("{}/api/system/heartbeat", self.base_url);
        let payload = FileDto { content };
        self.client.put(&url).json(&payload).send().await?;
        Ok(())
    }

    pub async fn get_soul(&self, role: &str) -> Result<String> {
        let url = format!("{}/api/system/soul/{}", self.base_url, url_encode(role));
        let resp = self.client.get(&url).send().await?;
        let dto: FileDto = resp.json().await?;
        Ok(dto.content)
    }

    pub async fn put_soul(&self, role: &str, content: String) -> Result<()> {
        let url = format!("{}/api/system/soul/{}", self.base_url, url_encode(role));
        let payload = FileDto { content };
        self.client.put(&url).json(&payload).send().await?;
        Ok(())
    }


    pub async fn delete_soul(&self, role: &str) -> Result<()> {
        let url = format!("{}/api/system/soul/{}", self.base_url, url_encode(role));
        self.client.delete(&url).send().await?;
        Ok(())
    }

    pub async fn list_souls(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/system/souls", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let roles: Vec<String> = resp.json().await?;
        Ok(roles)
    }

    pub async fn export_soul(&self, role: &str, limit: usize) -> Result<String> {
        let url = format!("{}/api/system/soul/{}/export", self.base_url, url_encode(role));
        let payload = serde_json::json!({ "limit": limit });
        let resp = self.client.post(&url).json(&payload).send().await?;
        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            anyhow::bail!("Export failed: {}", msg);
        }
        Ok(resp.text().await?)
    }

    pub async fn chat(&self, message: String, role: Option<String>, session_id: Option<String>) -> Result<String> {
        let url = format!("{}/api/chat", self.base_url);
        let req = ChatRequest {
            message,
            session_id,
            _model: None,
            role,
        };
        let resp = self.client.post(&url).json(&req).send().await?;
        let data: ChatResponse = resp.json().await?;
        Ok(data.response)
    }

    /// Run diagnostic checks on the gateway.
    pub async fn doctor_check(&self) -> Result<Vec<DoctorCheckResult>> {
        let url = format!("{}/api/system/doctor", self.base_url);
        let resp = self.client.get(&url).send().await?;
        if resp.status().is_success() {
            let res = resp.json().await?;
            Ok(res)
        } else {
            let text = resp.text().await?;
            Err(anyhow::anyhow!("Doctor check failed: {}", text))
        }
    }

    pub async fn get_active_sandboxes(&self) -> Result<Vec<ActiveSandboxContext>> {
        let url = format!("{}/api/system/sandboxes", self.base_url);
        let resp = self.client.get(&url).send().await?;
        if resp.status().is_success() {
            let res = resp.json().await?;
            Ok(res)
        } else {
            let text = resp.text().await?;
            Err(anyhow::anyhow!("Failed to fetch sandboxes: {}", text))
        }
    }

    pub async fn kill_sandbox(&self, pid: u32) -> Result<()> {
        let url = format!("{}/api/system/sandboxes/{}/kill", self.base_url, pid);
        let resp = self.client.post(&url).send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let text = resp.text().await?;
            Err(anyhow::anyhow!("Failed to kill sandbox {}: {}", pid, text))
        }
    }

    /// Request the gateway to shut down.
    pub async fn shutdown_gateway(&self) -> Result<()> {
        let url = format!("{}/api/system/shutdown", self.base_url);
        self.client.post(&url).send().await?;
        Ok(())
    }

    // ── Blueprint Gallery API (Phase 11-A) ───────────────────────────

    /// List all available blueprint templates.
    pub async fn list_blueprints(&self) -> Result<Vec<BlueprintInfo>> {
        let url = format!("{}/api/blueprints", self.base_url);
        let resp = self.client.get(&url).send().await?;
        Ok(resp.json::<Vec<BlueprintInfo>>().await?)
    }

    /// Apply a blueprint template to create/override a persona role.
    pub async fn apply_blueprint(&self, blueprint_id: &str, role: &str) -> Result<()> {
        let url = format!("{}/api/blueprints/{}/apply?role={}", self.base_url, url_encode(blueprint_id), url_encode(role));
        let resp = self.client.post(&url).send().await?;
        if !resp.status().is_success() {
            let msg = resp.text().await.unwrap_or_default();
            anyhow::bail!("Blueprint apply failed: {}", msg);
        }
        Ok(())
    }

    // ── Task Cancellation API (Phase 11-B) ───────────────────────────

    /// Send a cancel signal to abort all active agent tasks.
    pub async fn cancel_task(&self) -> Result<()> {
        let url = format!("{}/api/cancel", self.base_url);
        self.client.post(&url).send().await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub _model: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    pub response: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileDto {
    pub content: String,
}

/// Response from POST /api/skills/install
#[derive(Debug, Clone, Deserialize)]
pub struct InstallSkillResponse {
    pub success: bool,
    pub skill_name: String,
    pub message: String,
}

/// A blueprint template from the gallery (Phase 11-A)
#[derive(Debug, Clone, Deserialize)]
pub struct BlueprintInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
}

// ... unchanged urlencoding ...
fn url_encode(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { vec![c] }
            else {
                let encoded = format!("%{:02X}", c as u32);
                encoded.chars().collect::<Vec<_>>()
            }
        })
        .collect()
}
