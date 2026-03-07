//! Application state shared across all egui panels.

use crate::i18n::Language;

use crate::api::{
    BlueprintInfo, CronJob, GatewayClient, GatewaySnapshot, InstallSkillResponse, LogEntry,
    MarketClient, MarketSkill, SessionInfo, SkillInfo,
};
use poll_promise::Promise;

/// Which tab is currently active.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveTab {
    Skills,
    Api,
    Logs,
    Store,
    Sessions,
    Cron,
    Persona,
    Connection,
    Chat,
    Dashboard,
    System,
    Channels,
}

impl Default for ActiveTab {
    fn default() -> Self {
        Self::Skills
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkillsSubTab {
    Installed,
    Market,
    Manual,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PersonaSubTab {
    Editor,
    Gallery,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiSubTab {
    Keys,
    Voice,
    Comm,
}

/// A vault entry being entered by the user.
#[derive(Default, Clone)]
pub struct VaultEntry {
    pub key: String,
    pub value: String,
    pub saved: bool,
    pub error: Option<String>,
}

/// The full panel application state.
pub struct AppState {
    /// Current active tab
    pub tab: ActiveTab,
    pub skills_subtab: SkillsSubTab,
    pub persona_subtab: PersonaSubTab,
    pub api_subtab: ApiSubTab,

    /// The gateway endpoint URL (editable in Connection tab)
    pub gateway_url: String,

    /// API client (recreated when URL changes)
    pub client: GatewayClient,

    /// Async load of skills
    pub skills_promise: Option<Promise<Result<Vec<SkillInfo>, String>>>,

    /// Cached skills list (after successful load)
    pub skills: Vec<SkillInfo>,

    /// Vault entries being edited
    pub vault_entries: Vec<VaultEntry>,
    pub new_vault_key: String,
    pub new_vault_value: String,
    pub vault_show_value: bool,

    /// Connection status
    pub connected: Option<bool>,
    pub gateway_version: Option<String>,

    /// Log buffer (filled via periodic polling)
    pub log_lines: Vec<String>,

    /// Currently expanded skill (for detail popup)
    pub expanded_skill: Option<String>,

    /// Skills market data
    pub market_skills: Vec<MarketSkill>,
    pub market_loading: bool,
    pub market_error: Option<String>,
    pub market_search_query: String,
    pub market_search_promise: Option<Promise<Result<Vec<MarketSkill>, String>>>,
    pub market_install_promise: Option<Promise<Result<InstallSkillResponse, String>>>,
    pub market_installing_url: Option<String>,
    pub market_install_error: Option<String>,
    pub market_install_success: Option<String>,
    pub market_page: u32,

    /// Timer tracking (egui time = seconds since startup, works on WASM too)
    pub last_log_poll_time: f64,
    pub last_skill_refresh_time: f64,
    /// Whether auto-refresh is enabled for logs
    pub auto_log_poll: bool,
    /// Pending one-shot log fetch promise
    pub pending_log_promise: Option<Promise<Vec<String>>>,

    /// Last error/status message (displayed in footer)
    pub status_msg: Option<(String, bool)>, // (message, is_error)

    // ── Cron state ───────────────────────────────────────────────────────────
    pub cron_jobs: Vec<CronJob>,
    pub cron_loading: bool,
    pub cron_error: Option<String>,
    pub last_cron_refresh_time: f64,
    pub pending_cron_promise: Option<Promise<Result<Vec<CronJob>, String>>>,
    pub pending_cron_action_promise: Option<Promise<Result<String, String>>>,

    // New job form
    pub cron_form_name: String,
    pub cron_form_schedule: String, // "every" | "cron" | "at"
    pub cron_form_interval: String, // seconds for "every"
    pub cron_form_expr: String,     // cron expr for "cron"
    pub cron_form_prompt: String,

    // ── Sessions state ───────────────────────────────────────────────────────
    pub sessions: Vec<SessionInfo>,
    pub sessions_loading: bool,
    pub sessions_error: Option<String>,
    pub last_sessions_refresh_time: f64,
    pub pending_sessions_promise: Option<Promise<Result<Vec<SessionInfo>, String>>>,

    // ── Snapshot / Overview state ─────────────────────────────────────────
    pub snapshot: Option<GatewaySnapshot>,
    pub last_snapshot_refresh_time: f64,
    pub pending_snapshot_promise: Option<Promise<Result<GatewaySnapshot, String>>>,
    /// Keys the user has explicitly deleted this session — prevents snapshot from re-adding them
    pub deleted_vault_keys: std::collections::HashSet<String>,

    // ── Voice / TTS state ──────────────────────────────────────────────────
    pub voice_tts_provider: String,
    pub voice_tts_model: String,
    pub voice_tts_voice: String,
    pub voice_local_tts_enabled: bool,
    pub voice_local_tts_path: String,
    pub whisper_model: String,
    pub whisper_language: String,
    pub whisper_status: Option<String>,
    pub piper_voice: String,
    pub piper_status: Option<String>,

    // ── Structured log entries ────────────────────────────────────────────
    pub log_entries: Vec<LogEntry>,
    pub log_filter_text: String,
    pub log_level_filter: LogLevelFilter,

    // ── Store tab (Browse & Install) ──────────────────────────────────────
    pub store_install_url: String,
    pub store_installing: bool,
    pub store_install_error: Option<String>,
    pub store_install_success: Option<String>,
    pub pending_install_promise: Option<Promise<Result<InstallSkillResponse, String>>>,

    // ── Channels (Integrations) state ──────────────────────────────────────
    pub channels: Vec<crate::api::ChannelMetadata>,
    pub channels_loading: bool,
    pub channels_error: Option<String>,
    pub pending_channels_promise: Option<Promise<Result<Vec<crate::api::ChannelMetadata>, String>>>,
    pub active_channel_id: Option<String>,
    pub channel_form_values: std::collections::HashMap<String, String>,

    // ── LLM Providers state ───────────────────────────────────────────────
    pub provider_metadata: Vec<crate::api::ProviderMetadata>,
    pub provider_loading: bool,
    pub provider_error: Option<String>,
    pub pending_provider_promise:
        Option<Promise<Result<crate::api::ProviderSchemaResponse, String>>>,

    // ── Persona / System Prompt state ─────────────────────────────────────
    pub persona_heartbeat_content: String,
    pub persona_heartbeat_dirty: bool,
    pub persona_heartbeat_promise: Option<Promise<Result<String, String>>>,
    pub persona_heartbeat_loaded: bool,
    pub persona_save_promise: Option<Promise<Result<(), String>>>,
    pub persona_export_limit: usize,
    pub persona_export_promise: Option<Promise<Result<String, String>>>,
    pub persona_export_json: Option<String>,

    pub persona_role_selected: String,
    pub is_adding_persona: bool,
    pub new_persona_name: String,
    pub custom_added_personas: std::collections::BTreeSet<String>,
    pub persona_role_content: String,
    pub persona_role_dirty: bool,
    pub persona_role_promise: Option<Promise<Result<String, String>>>,
    pub persona_role_loaded: bool,
    pub persona_role_provider: String,
    pub persona_role_base_url: String,
    pub persona_role_model: String,
    pub persona_role_temperature: String,
    pub persona_souls_promise: Option<Promise<Result<Vec<String>, String>>>,
    pub persona_souls: Vec<String>,
    pub persona_templates: Vec<crate::api::PersonaTemplate>,
    pub persona_templates_promise:
        Option<Promise<Result<Vec<crate::api::PersonaTemplate>, String>>>,

    // ── Chat state ──────────────────────────────────────────────────────────
    pub chat_histories: std::collections::BTreeMap<String, Vec<ChatMessage>>,
    pub chat_input: String,
    pub chat_selected_role: String,
    pub chat_loading: bool,
    pub chat_promise: Option<Promise<Result<String, String>>>,

    // ── Diagnostic / Doctor state ───────────────────────────────────────────
    pub doctor_loading: bool,
    pub doctor_error: Option<String>,
    pub doctor_results: Option<Vec<crate::api::DoctorCheckResult>>,
    pub pending_doctor_promise: Option<Promise<Result<Vec<crate::api::DoctorCheckResult>, String>>>,

    // ── Exit Dialog state ───────────────────────────────────────────────────
    pub show_exit_dialog: bool,
    pub exit_in_progress: bool,

    // ── Metrics Display state ───────────────────────────────────────────────
    pub metrics_loading: bool,
    pub metrics_error: Option<String>,
    pub last_metrics: Option<crate::api::Metrics>,
    pub pending_metrics_promise: Option<Promise<Result<crate::api::Metrics, String>>>,
    pub last_metrics_refresh_time: f64,
    pub metrics_history: Vec<(f64, u64)>, // (time, value)

    // ── Channels state ──────────────────────────────────────────────────────
    pub channel_metadata: Vec<crate::api::ChannelMetadata>,
    pub running_channels: Vec<String>,
    pub channel_metadata_promise:
        Option<Promise<Result<crate::api::ChannelSchemaResponse, String>>>,
    pub last_channel_refresh_time: f64,

    // ── Sandboxes state ─────────────────────────────────────────────────────
    pub sandboxes: Vec<crate::api::ActiveSandboxContext>,
    pub sandboxes_promise: Option<Promise<Result<Vec<crate::api::ActiveSandboxContext>, String>>>,
    pub kill_sandbox_promise: Option<Promise<Result<(), String>>>,
    pub last_sandboxes_refresh_time: f64,

    // ── Local Model Resource Management ─────────────────────────────────────
    pub model_vram_limit_gb: u32,
    pub model_ram_limit_gb: u32,

    // ── Blueprint Gallery state (Phase 11-A) ────────────────────────
    pub blueprints: Vec<BlueprintInfo>,
    pub blueprints_promise: Option<Promise<Result<Vec<BlueprintInfo>, String>>>,
    pub blueprint_apply_promise: Option<Promise<Result<(), String>>>,
    pub blueprint_apply_role: String,

    // ── Task Cancellation state (Phase 11-B) ───────────────────────
    pub cancel_promise: Option<Promise<Result<(), String>>>,

    pub night_mode: bool,

    // ── Reranker & Embedder state (Phase 7.2) ──────────────────────────────
    pub use_local_reranker: Option<bool>,
    pub bge_model_status: Option<String>,
    pub use_local_embed: Option<bool>,
    pub bert_model_status: Option<String>,
    pub use_local_ocr: Option<bool>,

    /// UI Language state
    pub language: Language,

    /// Tracked UI scale factor (based on window width). Updated every frame.
    /// Used to avoid re-setting text styles when scale hasn't changed.
    pub last_ui_scale: f32,

    /// Whether we have performed the initial 50% screen-size resize.
    pub initial_resize_done: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" or "agent"
    pub content: String,
    pub agent_name: Option<String>,
}

/// Level filter for logs
#[derive(Debug, Clone, Default)]
pub struct LogLevelFilter {
    pub show_trace: bool,
    pub show_debug: bool,
    pub show_info: bool,
    pub show_warn: bool,
    pub show_error: bool,
}

impl LogLevelFilter {
    pub fn all_on() -> Self {
        Self {
            show_trace: false, // off by default (too noisy)
            show_debug: false,
            show_info: true,
            show_warn: true,
            show_error: true,
        }
    }

    pub fn matches(&self, level: &str) -> bool {
        match level.to_lowercase().as_str() {
            "trace" => self.show_trace,
            "debug" => self.show_debug,
            "warn" => self.show_warn,
            "error" | "fatal" => self.show_error,
            _ => self.show_info, // info or unknown
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        let url = load_saved_url().unwrap_or_else(|| "http://localhost:3000".to_string());
        let saved = load_saved_config();
        let tab = saved.tab;
        let night_mode = saved.night_mode;
        let language = saved.language;
        let client = GatewayClient::new(url.clone());

        // Pre-populate vault with common key names
        let vault_entries = vec![
            VaultEntry {
                key: "OPENAI_API_KEY".to_string(),
                ..Default::default()
            },
            VaultEntry {
                key: "ANTHROPIC_API_KEY".to_string(),
                ..Default::default()
            },
            VaultEntry {
                key: "GEMINI_API_KEY".to_string(),
                ..Default::default()
            },
            VaultEntry {
                key: "DEEPSEEK_API_KEY".to_string(),
                ..Default::default()
            },
            VaultEntry {
                key: "MINIMAX_API_KEY".to_string(),
                ..Default::default()
            },
        ];

        Self {
            tab,
            skills_subtab: SkillsSubTab::Installed,
            persona_subtab: PersonaSubTab::Editor,
            api_subtab: ApiSubTab::Keys,
            gateway_url: url,
            client,
            skills_promise: None,
            skills: vec![],
            vault_entries,
            new_vault_key: String::new(),
            new_vault_value: String::new(),
            vault_show_value: false,
            connected: None,
            gateway_version: None,
            log_lines: vec![],
            expanded_skill: None,
            market_skills: vec![],
            market_loading: false,
            market_error: None,
            market_search_query: String::new(),
            market_search_promise: None,
            market_install_promise: None,
            market_installing_url: None,
            market_install_error: None,
            market_install_success: None,
            market_page: 1,
            last_log_poll_time: -999.0, // force immediate poll on first open
            last_skill_refresh_time: -999.0,
            auto_log_poll: true,
            pending_log_promise: None,
            status_msg: None,
            cron_jobs: vec![],
            cron_loading: false,
            cron_error: None,
            last_cron_refresh_time: -999.0,
            pending_cron_promise: None,
            pending_cron_action_promise: None,
            cron_form_name: String::new(),
            cron_form_schedule: "every".to_string(),
            cron_form_interval: "3600".to_string(),
            cron_form_expr: "0 * * * *".to_string(),
            cron_form_prompt: String::new(),
            sessions: vec![],
            sessions_loading: false,
            sessions_error: None,
            last_sessions_refresh_time: -999.0,
            pending_sessions_promise: None,
            snapshot: None,
            last_snapshot_refresh_time: -999.0,
            pending_snapshot_promise: None,
            deleted_vault_keys: std::collections::HashSet::new(),

            voice_tts_provider: "openai".to_string(),
            voice_tts_model: saved.voice_tts_model,
            voice_tts_voice: saved.voice_tts_voice,
            voice_local_tts_enabled: saved.voice_local_tts_enabled,
            voice_local_tts_path: saved.voice_local_tts_path,
            whisper_model: saved.whisper_model,
            whisper_language: saved.whisper_language,
            whisper_status: Some("Not Checked".to_string()),
            piper_voice: saved.piper_voice,
            piper_status: Some("Not Checked".to_string()),
            model_ram_limit_gb: saved.model_ram_limit_gb,
            model_vram_limit_gb: saved.model_vram_limit_gb,
            log_entries: vec![],
            log_filter_text: String::new(),
            log_level_filter: LogLevelFilter::all_on(),
            store_install_url: String::new(),
            store_installing: false,
            store_install_error: None,
            store_install_success: None,
            pending_install_promise: None,
            channels: vec![],
            channels_loading: false,
            channels_error: None,
            pending_channels_promise: None,
            active_channel_id: None,
            channel_form_values: std::collections::HashMap::new(),
            provider_metadata: Vec::new(),
            provider_loading: false,
            provider_error: None,
            pending_provider_promise: None,
            persona_heartbeat_content: String::new(),
            persona_heartbeat_dirty: false,
            persona_heartbeat_promise: None,
            persona_heartbeat_loaded: false,
            persona_save_promise: None,
            persona_export_limit: 50,
            persona_export_promise: None,
            persona_export_json: None,
            persona_role_selected: "assistant".to_string(),
            is_adding_persona: false,
            new_persona_name: String::new(),
            custom_added_personas: std::collections::BTreeSet::new(),
            persona_role_content: String::new(),
            persona_role_dirty: false,
            persona_role_promise: None,
            persona_role_loaded: false,
            persona_role_provider: "openai".to_string(),
            persona_role_base_url: String::new(),
            persona_role_model: "gpt-4o".to_string(),
            persona_role_temperature: "0.7".to_string(),
            persona_souls_promise: None,
            persona_souls: vec!["assistant".to_string()], // assistant is always the minimum default
            persona_templates: vec![],
            persona_templates_promise: None,
            chat_histories: std::collections::BTreeMap::new(),
            chat_input: String::new(),
            chat_selected_role: "assistant".to_string(),
            chat_loading: false,
            chat_promise: None,
            doctor_loading: false,
            doctor_error: None,
            doctor_results: None,
            pending_doctor_promise: None,
            show_exit_dialog: false,
            exit_in_progress: false,
            metrics_loading: false,
            metrics_error: None,
            last_metrics: None,
            pending_metrics_promise: None,
            last_metrics_refresh_time: 0.0,
            metrics_history: Vec::new(),

            channel_metadata: Vec::new(),
            running_channels: Vec::new(),
            channel_metadata_promise: None,
            last_channel_refresh_time: 0.0,

            sandboxes: Vec::new(),
            sandboxes_promise: None,
            kill_sandbox_promise: None,
            last_sandboxes_refresh_time: 0.0,

            blueprints: Vec::new(),
            blueprints_promise: None,
            blueprint_apply_promise: None,
            blueprint_apply_role: String::new(),

            cancel_promise: None,

            use_local_reranker: Some(false),
            bge_model_status: Some("Not Installed".to_string()),
            use_local_embed: Some(false),
            bert_model_status: Some("Not Installed".to_string()),
            use_local_ocr: Some(false),

            night_mode,
            language,
            last_ui_scale: 0.0,
            initial_resize_done: false,
        }
    }

    pub fn set_url(&mut self, url: String) {
        save_url(&url);
        self.client = GatewayClient::new(url.clone());
        self.gateway_url = url;
        self.connected = None;
        self.skills = vec![];
        self.skills_promise = None;
    }

    pub fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_msg = Some((msg.into(), is_error));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn config_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("aimaxxing-panel")
}

#[cfg(not(target_arch = "wasm32"))]
fn url_config_path() -> std::path::PathBuf {
    config_dir().join("gateway_url.txt")
}

#[cfg(not(target_arch = "wasm32"))]
fn load_saved_url() -> Option<String> {
    std::fs::read_to_string(url_config_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn save_url(url: &str) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(url_config_path(), url);
}

#[cfg(target_arch = "wasm32")]
fn load_saved_url() -> Option<String> {
    let storage = web_sys::window()?.local_storage().ok()??;
    storage.get_item("aimaxxing_gateway_url").ok()?
}

#[cfg(target_arch = "wasm32")]
fn save_url(url: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item("aimaxxing_gateway_url", url);
        }
    }
}

pub struct SavedConfig {
    pub tab: ActiveTab,
    pub night_mode: bool,
    pub language: Language,
    pub voice_tts_model: String,
    pub voice_tts_voice: String,
    pub voice_local_tts_enabled: bool,
    pub voice_local_tts_path: String,
    pub whisper_model: String,
    pub whisper_language: String,
    pub piper_voice: String,
    pub model_ram_limit_gb: u32,
    pub model_vram_limit_gb: u32,
}

pub fn load_saved_config() -> SavedConfig {
    let mut config = SavedConfig {
        tab: ActiveTab::Skills,
        night_mode: true,
        language: Language::En,
        voice_tts_model: "tts-1".to_string(),
        voice_tts_voice: "alloy".to_string(),
        voice_local_tts_enabled: false,
        voice_local_tts_path: String::new(),
        whisper_model: "ggml-tiny.en".to_string(),
        whisper_language: "en".to_string(),
        piper_voice: "en_US-lessac-medium".to_string(),
        model_ram_limit_gb: 4,
        model_vram_limit_gb: 0,
    };

    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(tab_str)) = storage.get_item("aimaxxing_active_tab") {
                    config.tab = match tab_str.as_str() {
                        "Skills" => ActiveTab::Skills,
                        "Api" | "Vault" => ActiveTab::Api,
                        "Logs" => ActiveTab::Logs,
                        "Store" => ActiveTab::Store,
                        "Sessions" => ActiveTab::Sessions,
                        "Cron" => ActiveTab::Cron,
                        "Persona" => ActiveTab::Persona,
                        "Connection" => ActiveTab::Connection,
                        "Dashboard" => ActiveTab::Dashboard,
                        "System" => ActiveTab::System,
                        "Channels" => ActiveTab::Channels,
                        _ => ActiveTab::Skills,
                    };
                }
                if let Ok(Some(mode_str)) = storage.get_item("aimaxxing_night_mode") {
                    config.night_mode = mode_str == "true";
                }
                if let Ok(Some(lang_str)) = storage.get_item("aimaxxing_language") {
                    config.language = if lang_str == "Zh" {
                        Language::Zh
                    } else {
                        Language::En
                    };
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_voice_model") {
                    config.voice_tts_model = v;
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_voice_persona") {
                    config.voice_tts_voice = v;
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_voice_local_enabled") {
                    config.voice_local_tts_enabled = v == "true";
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_voice_local_path") {
                    config.voice_local_tts_path = v;
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_whisper_model") {
                    config.whisper_model = v;
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_whisper_language") {
                    config.whisper_language = v;
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_piper_voice") {
                    config.piper_voice = v;
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_model_ram_limit_gb") {
                    config.model_ram_limit_gb = v.parse().unwrap_or(4);
                }
                if let Ok(Some(v)) = storage.get_item("aimaxxing_model_vram_limit_gb") {
                    config.model_vram_limit_gb = v.parse().unwrap_or(0);
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = config_dir().join("settings.json");
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(t_str) = val.get("tab").and_then(|t| t.as_str()) {
                    config.tab = match t_str {
                        "Skills" => ActiveTab::Skills,
                        "Api" | "Vault" => ActiveTab::Api,
                        "Logs" => ActiveTab::Logs,
                        "Store" => ActiveTab::Store,
                        "Sessions" => ActiveTab::Sessions,
                        "Cron" => ActiveTab::Cron,
                        "Persona" => ActiveTab::Persona,
                        "Connection" => ActiveTab::Connection,
                        "Dashboard" => ActiveTab::Dashboard,
                        "System" => ActiveTab::System,
                        "Channels" => ActiveTab::Channels,
                        _ => ActiveTab::Skills,
                    };
                }
                if let Some(m) = val.get("night_mode").and_then(|m| m.as_bool()) {
                    config.night_mode = m;
                }
                if let Some(l) = val.get("language").and_then(|l| l.as_str()) {
                    config.language = if l == "Zh" {
                        Language::Zh
                    } else {
                        Language::En
                    };
                }
                if let Some(v) = val.get("voice_model").and_then(|v| v.as_str()) {
                    config.voice_tts_model = v.to_string();
                }
                if let Some(v) = val.get("voice_persona").and_then(|v| v.as_str()) {
                    config.voice_tts_voice = v.to_string();
                }
                if let Some(v) = val.get("voice_local_enabled").and_then(|v| v.as_bool()) {
                    config.voice_local_tts_enabled = v;
                }
                if let Some(v) = val.get("voice_local_path").and_then(|v| v.as_str()) {
                    config.voice_local_tts_path = v.to_string();
                }
                if let Some(v) = val.get("whisper_model").and_then(|v| v.as_str()) {
                    config.whisper_model = v.to_string();
                }
                if let Some(v) = val.get("whisper_language").and_then(|v| v.as_str()) {
                    config.whisper_language = v.to_string();
                }
                if let Some(v) = val.get("piper_voice").and_then(|v| v.as_str()) {
                    config.piper_voice = v.to_string();
                }
                if let Some(v) = val.get("model_ram_limit_gb").and_then(|v| v.as_u64()) {
                    config.model_ram_limit_gb = v as u32;
                }
                if let Some(v) = val.get("model_vram_limit_gb").and_then(|v| v.as_u64()) {
                    config.model_vram_limit_gb = v as u32;
                }
            }
        }
    }

    config
}

pub fn save_config(state: &AppState) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item("aimaxxing_active_tab", &format!("{:?}", state.tab));
                let _ = storage.set_item(
                    "aimaxxing_night_mode",
                    if state.night_mode { "true" } else { "false" },
                );
                let _ = storage.set_item(
                    "aimaxxing_language",
                    if state.language == Language::Zh {
                        "Zh"
                    } else {
                        "En"
                    },
                );
                let _ = storage.set_item("aimaxxing_voice_model", &state.voice_tts_model);
                let _ = storage.set_item("aimaxxing_voice_persona", &state.voice_tts_voice);
                let _ = storage.set_item(
                    "aimaxxing_voice_local_enabled",
                    if state.voice_local_tts_enabled {
                        "true"
                    } else {
                        "false"
                    },
                );
                let _ = storage.set_item("aimaxxing_voice_local_path", &state.voice_local_tts_path);
                let _ = storage.set_item("aimaxxing_whisper_model", &state.whisper_model);
                let _ = storage.set_item("aimaxxing_whisper_language", &state.whisper_language);
                let _ = storage.set_item("aimaxxing_piper_voice", &state.piper_voice);
                let _ = storage.set_item(
                    "aimaxxing_model_ram_limit_gb",
                    &state.model_ram_limit_gb.to_string(),
                );
                let _ = storage.set_item(
                    "aimaxxing_model_vram_limit_gb",
                    &state.model_vram_limit_gb.to_string(),
                );
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = config_dir().join("settings.json");
        let dict = serde_json::json!({
            "tab": format!("{:?}", state.tab),
            "night_mode": state.night_mode,
            "language": if state.language == Language::Zh { "Zh" } else { "En" },
            "voice_model": state.voice_tts_model,
            "voice_persona": state.voice_tts_voice,
            "voice_local_enabled": state.voice_local_tts_enabled,
            "voice_local_path": state.voice_local_tts_path,
            "whisper_model": state.whisper_model,
            "whisper_language": state.whisper_language,
            "piper_voice": state.piper_voice,
            "model_ram_limit_gb": state.model_ram_limit_gb,
            "model_vram_limit_gb": state.model_vram_limit_gb,
        });
        if let Ok(content) = serde_json::to_string_pretty(&dict) {
            let _ = std::fs::create_dir_all(config_dir());
            let _ = std::fs::write(path, content);
        }
    }
}
