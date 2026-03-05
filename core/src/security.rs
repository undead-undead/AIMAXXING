use async_trait::async_trait;
use std::path::Path;
use crate::error::Result;
use serde::{Serialize, Deserialize};

/// Trait for inspecting .vessel packages for security violations.
#[async_trait]
pub trait VesselInspector: Send + Sync {
    /// Inspect the unpacked soul and identity files.
    async fn inspect_soul(&self, extract_to: &Path) -> Result<()>;
}

/// Output of a security input check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedOutput {
    pub content: String,
    pub warnings: Vec<String>,
    pub was_modified: bool,
}

/// A detection of a potential secret leak.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakDetection {
    pub pattern_name: String,
    pub redacted_value: String,
}

/// Trait for enforcing security policies on inputs and outputs.
pub trait SecurityHandler: Send + Sync {
    /// Scan input text for potential threats (e.g., prompt injection).
    fn check_input(&self, text: &str) -> SanitizedOutput;
    /// Scan output text for potential leaks (e.g., API keys).
    fn check_output(&self, text: &str) -> (String, Vec<LeakDetection>);
}
