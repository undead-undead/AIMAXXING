use crate::error::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Abstraction for an execution environment (Native, Sandbox, Wasm, etc.)
#[async_trait]
pub trait RuntimeAdapter: Send + Sync {
    /// Execute a command in the environment
    async fn execute(&self, cmd: &str, args: &[String]) -> Result<String>;

    /// Write a file into the environment
    async fn write_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Read a file from the environment
    async fn read_file(&self, path: &Path) -> Result<String>;

    /// Prepare the environment (e.g., provision dependencies)
    async fn prepare(&self) -> Result<()>;

    /// Get the working directory path within the environment
    fn workdir(&self) -> PathBuf;
}
