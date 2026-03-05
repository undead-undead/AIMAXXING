use crate::error::Result;
use crate::skills::{SkillMetadata, SkillExecutionConfig};
use crate::env::EnvManager;
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

// Sub-modules
#[cfg(feature = "wasm")]
pub mod wasm;
pub mod quickjs;
pub mod micropython;
pub mod node;
pub mod python_utils;

#[cfg(feature = "wasm")]
pub use wasm::WasmRuntime;
pub use quickjs::QuickJSRuntime;
pub use micropython::MicroPythonRuntime;
pub use node::SmartNodeRuntime;

/// The unified execution abstraction for all skill runtimes.
#[async_trait]
pub trait SkillRuntime: Send + Sync {
    async fn execute(
        &self,
        metadata: &SkillMetadata,
        arguments: &str,
        base_dir: &Path,
        config: &SkillExecutionConfig,
        env_manager: Option<&Arc<EnvManager>>,
    ) -> Result<std::process::Output>;
}
