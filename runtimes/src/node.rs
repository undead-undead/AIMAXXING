use brain::error::{Error, Result};
use crate::{QuickJSRuntime, SkillRuntime, SkillMetadata, SkillExecutionConfig};
use security::sandbox::NativeShellRuntime;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info, warn};

/// SmartNodeRuntime — handles `runtime: node` or `runtime: js` skills.
/// 
/// 5-Tiered execution logic:
/// 1. Level 0: QuickJS (Internal, Lightning fast, memory-locked)
/// 2. Level 1: System Bun (Native performance)
/// 3. Level 2: Pixi Bun (Automated high-performance provisioning)
/// 4. Level 3: System Node (Compatibility)
/// 5. Level 4: Pixi Node (Ultimate fallback)
pub struct SmartNodeRuntime;

impl SmartNodeRuntime {
    pub fn new() -> Self {
        Self
    }

    async fn find_system_interpreter(name: &str) -> Option<PathBuf> {
        which::which(name).ok()
    }
}

#[async_trait]
impl SkillRuntime for SmartNodeRuntime {
    async fn execute(
        &self,
        metadata: &crate::SkillMetadata,
        arguments: &str,
        base_dir: &Path,
        config: &SkillExecutionConfig,
        env_manager: Option<&std::sync::Arc<brain::env::EnvManager>>,
    ) -> Result<std::process::Output> {
        // --- LEVEL 0: QuickJS Trial Run (For simple logic) ---
        // We only attempt QuickJS if there are no declared dependencies and no browser requirement.
        if metadata.dependencies.is_empty() && !metadata.use_browser {
            let qjs = QuickJSRuntime::new();
            match qjs.execute(metadata, arguments, base_dir, config, env_manager).await {
                Ok(output) if output.status.success() => {
                    debug!(skill = %metadata.name, "Execution succeeded via internal QuickJS");
                    return Ok(output);
                }
                _ => {
                    debug!(skill = %metadata.name, "QuickJS trial failed (likely complex Node APIs). Falling back to OS processes...");
                }
            }
        }

        // --- LEVEL 1: Pixi + Bun (Preferred Modern Runtime) ---
        if let Some(em) = env_manager {
            let mut bun_deps = metadata.dependencies.clone();
            if !bun_deps.iter().any(|d| d == "bun" || d == "ts" || d == "typescript") {
                bun_deps.push("bun".to_string());
            }

            match em.provision(&metadata.name, &bun_deps, metadata.use_browser).await {
                Ok(env_prefix) => {
                    let bun_bin = if cfg!(target_os = "windows") { env_prefix.join("bun.exe") } else { env_prefix.join("bin").join("bun") };
                    if bun_bin.exists() {
                        debug!(skill = %metadata.name, "Using Pixi-provisioned Bun environment");
                        let mut mod_meta = metadata.clone();
                        mod_meta.runtime = Some(bun_bin.to_string_lossy().to_string());
                        return NativeShellRuntime::new().execute(&mod_meta, arguments, base_dir, config, env_manager).await;
                    }
                }
                Err(e) => {
                    warn!(skill = %metadata.name, "Level 1 (Pixi Bun) fallback failed: {}. Moving to Level 2...", e);
                }
            }
        }

        // --- LEVEL 2: System Bun ---
        if let Some(bun_path) = Self::find_system_interpreter("bun").await {
            info!(skill = %metadata.name, "Level 2: Using system Bun runtime");
            let mut mod_meta = metadata.clone();
            mod_meta.runtime = Some(bun_path.to_string_lossy().to_string());
            return NativeShellRuntime::new().execute(&mod_meta, arguments, base_dir, config, env_manager).await;
        }

        // --- LEVEL 3: Pixi + Node.js (Compatibility) ---
        if let Some(em) = env_manager {
            let mut node_deps = metadata.dependencies.clone();
            if !node_deps.iter().any(|d| d == "nodejs" || d == "node") {
                node_deps.push("nodejs".to_string());
            }

            match em.provision(&metadata.name, &node_deps, metadata.use_browser).await {
                Ok(env_prefix) => {
                    let node_bin = if cfg!(target_os = "windows") { env_prefix.join("node.exe") } else { env_prefix.join("bin").join("node") };
                    if node_bin.exists() {
                        debug!(skill = %metadata.name, "Using Pixi-provisioned Node.js environment");
                        let mut mod_meta = metadata.clone();
                        mod_meta.runtime = Some(node_bin.to_string_lossy().to_string());
                        return NativeShellRuntime::new().execute(&mod_meta, arguments, base_dir, config, env_manager).await;
                    }
                }
                Err(e) => {
                    warn!(skill = %metadata.name, "Level 3 (Pixi Node) fallback failed: {}. Moving to Level 4...", e);
                }
            }
        }

        // --- LEVEL 4: System Node.js ---
        if let Some(node_path) = Self::find_system_interpreter("node").await {
            info!(skill = %metadata.name, "Level 4: Using system Node.js runtime");
            let mut mod_meta = metadata.clone();
            mod_meta.runtime = Some(node_path.to_string_lossy().to_string());
            return NativeShellRuntime::new().execute(&mod_meta, arguments, base_dir, config, env_manager).await;
        }

        // --- LEVEL 5: PNPM Fallback (Manual Dependency Resolution) ---
        if let Ok(pnpm_path) = which::which("pnpm") {
            info!(skill = %metadata.name, "Level 5: Attempting pnpm install fallback...");
            let install_status = Command::new(pnpm_path)
                .arg("install")
                .current_dir(base_dir)
                .status()
                .await;

            if let Ok(status) = install_status {
                if status.success() {
                    // Try to run with node again after pnpm install
                    if let Some(node_path) = Self::find_system_interpreter("node").await {
                        let mut mod_meta = metadata.clone();
                        mod_meta.runtime = Some(node_path.to_string_lossy().to_string());
                        return NativeShellRuntime::new().execute(&mod_meta, arguments, base_dir, config, env_manager).await;
                    }
                }
            }
        }

        Err(Error::ToolExecution {
            tool_name: "SmartNodeRuntime".into(),
            message: "All JS/TS runtime levels failed (QuickJS, Pixi-Bun, System-Bun, Pixi-Node, System-Node, pnpm).".to_string(),
        })
    }
}
