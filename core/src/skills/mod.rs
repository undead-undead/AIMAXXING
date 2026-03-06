// compiler and capabilities modules moved to 'builtin-tools' crate.
pub mod runtime;
// sandbox module moved to 'security' crate
pub mod tool;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Metadata extracted from a `SKILL.md` frontmatter
/// Specification for a model dependency required by a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Unique identifier / filename for the model (e.g., "whisper-tiny")
    pub name: String,
    /// Download URL (supports HTTPS, Hugging Face hub shorthand, or local path)
    pub source: String,
    /// Model format: "onnx", "gguf", "safetensors", "pytorch", "custom"
    #[serde(default = "default_model_format")]
    pub format: String,
    /// Expected size in MB (used for progress reporting and disk space checks)
    pub size_mb: Option<u64>,
    /// SHA256 checksum for integrity verification (hex string)
    pub sha256: Option<String>,
}

fn default_model_format() -> String {
    "onnx".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Name of the skill
    pub name: String,
    /// Short description
    pub description: String,
    /// Optional homepage URL
    pub homepage: Option<String>,
    /// Arguments schema (JSON Schema) - DEPRECATED: use parameters_ts
    pub parameters: Option<Value>,
    /// Arguments as TypeScript interface (Preferred)
    pub interface: Option<String>,
    /// Script to execute
    pub script: Option<String>,
    /// Language or runtime for the script
    pub runtime: Option<String>,
    /// Standard Smithery metadata object
    #[serde(default)]
    pub metadata: Value,
    /// Kind of skill (e.g., 'tool', 'knowledge', 'agent')
    #[serde(default = "default_skill_kind")]
    pub kind: String,
    /// Optional usage guidelines for LLM reasoning
    pub usage_guidelines: Option<String>,
    /// List of conda/pixi dependencies
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Whether this skill requires a browser
    #[serde(default)]
    pub use_browser: bool,
    /// Model dependencies for ML/AI skills
    #[serde(default)]
    pub models: Vec<ModelSpec>,
}

fn default_skill_kind() -> String {
    "tool".to_string()
}

/// Configuration for skill execution
#[derive(Debug, Clone)]
pub struct SkillExecutionConfig {
    /// Maximum execution time in seconds
    pub timeout_secs: u64,
    /// Maximum output size in bytes (to prevent memory exhaustion)
    pub max_output_bytes: usize,
    /// Whether to allow network access (future: implement via sandbox)
    pub allow_network: bool,
    /// Whether to provide a pre-configured headless browser
    pub use_browser: bool,
    /// Maximum memory in megabytes
    pub max_memory_mb: Option<usize>,
    /// Maximum CPU percentage (0-100)
    pub max_cpu_percent: Option<usize>,
    /// Custom environment variables
    pub env_vars: HashMap<String, String>,
}

impl Default for SkillExecutionConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            max_output_bytes: 1024 * 1024, // 1MB
            allow_network: false,
            use_browser: false,
            max_memory_mb: None,
            max_cpu_percent: None,
            env_vars: HashMap::new(),
        }
    }
}

// Heavy implementations (DynamicSkill, SkillLoader, etc.) moved to 'builtin-tools' crate.
