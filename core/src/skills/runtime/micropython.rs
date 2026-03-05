use crate::error::{Error, Result};
use crate::skills::runtime::SkillRuntime;
use crate::skills::SkillExecutionConfig;
use crate::skills::sandbox::NativeShellRuntime;
use crate::skills::runtime::python_utils;
use async_trait::async_trait;
use std::path::{Path};
use tracing::info;

/// MicroPythonRuntime — handles `runtime: python3` skills.
///
/// Complete execution flow:
///
/// ```text
/// 宿主机有 Python?
///   ├─YES─► 直接用系统 Python
///   └─NO──► uv 静默下载 Python 3.11 到 ~/.aimaxxing/runtimes/python/
///
/// SKILL.md 有 dependencies?
///   ├─YES─► 优先使用 uv/venv。如果包含非 Python 依赖（如 c, nodejs, browser），由 EnvManager (Pixi) 处理。
///   └─NO──► 直接用 base Python 执行
///
/// 用解析出的 Python 二进制，交给 NativeShellRuntime 执行
/// (自动裹上 bwrap / sandbox-exec 沙箱)
/// ```
pub struct MicroPythonRuntime {
    /// Skill name for venv isolation (set by caller)
    pub skill_name: String,
    /// Declared pip dependencies from SKILL.md
    pub dependencies: Vec<String>,
}

impl MicroPythonRuntime {
    pub fn new() -> Self {
        Self {
            skill_name: "unknown".to_string(),
            dependencies: vec![],
        }
    }

    pub fn with_skill_context(skill_name: impl Into<String>, dependencies: Vec<String>) -> Self {
        Self {
            skill_name: skill_name.into(),
            dependencies,
        }
    }
}

#[async_trait]
impl SkillRuntime for MicroPythonRuntime {
    async fn execute(
        &self,
        metadata: &crate::skills::SkillMetadata,
        arguments: &str,
        base_dir: &Path,
        config: &SkillExecutionConfig,
        env_manager: Option<&std::sync::Arc<crate::env::EnvManager>>,
    ) -> Result<std::process::Output> {
        let _script_file = metadata.script.as_ref().ok_or_else(|| {
            Error::ToolExecution {
                tool_name: metadata.name.clone(),
                message: "No script defined for this skill".to_string(),
            }
        })?;
        // 1. Determine whether to use Pixi (for binary/system deps) or uv (pure python)
        let has_binary_deps = metadata.use_browser || self.dependencies.iter().any(|d| {
            let dl = d.to_lowercase();
            dl == "c" || dl == "gcc" || dl == "nodejs" || dl.contains(".so") || dl.contains(".dll")
        });

        let mut python_bin = None;

        // TIER 1: Try uv/venv Flow (Fastest for pure Python)
        if !has_binary_deps {
            match python_utils::find_python().await {
                Some(p) => {
                    if let Ok(venv_p) = python_utils::ensure_venv(&p, &self.skill_name, &self.dependencies).await {
                        python_bin = Some(venv_p);
                    }
                }
                None => {
                    // Try to provision uv
                    if let Ok(p) = python_utils::provision_python_via_uv().await {
                        if let Ok(venv_p) = python_utils::ensure_venv(&p, &self.skill_name, &self.dependencies).await {
                            python_bin = Some(venv_p);
                        }
                    }
                }
            }
        }

        // TIER 2: Fallback to Pixi (If TIER 1 failed or binary deps detected)
        if python_bin.is_none() && env_manager.is_some() {
            let em = env_manager.unwrap();
            info!(skill = %self.skill_name, "Provisioning via EnvManager (Pixi) - TIER 2 fallback");
            let mut deps = self.dependencies.clone();
            if !deps.iter().any(|d| d == "python") {
                deps.push("python".to_string());
            }
            if let Ok(env_prefix) = em.provision(&self.skill_name, &deps, metadata.use_browser).await {
                let bin = if cfg!(target_os = "windows") {
                    env_prefix.join("python.exe")
                } else {
                    env_prefix.join("bin").join("python3")
                };
                if bin.exists() {
                    python_bin = Some(bin);
                }
            }
        }

        let python_bin = python_bin.ok_or_else(|| Error::ToolExecution {
            tool_name: self.skill_name.clone(),
            message: "Failed to resolve Python runtime via all tiers (uv, Pixi).".to_string(),
        })?;

        let python_interpreter = python_bin.to_string_lossy().to_string();
        info!(
            skill = %self.skill_name,
            interpreter = %python_interpreter,
            "Executing Python skill"
        );

        // 3. Delegate to the native OS sandbox
        let mut modified_metadata = metadata.clone();
        modified_metadata.runtime = Some(python_interpreter);

        let native = NativeShellRuntime::new();
        native
            .execute(
                &modified_metadata,
                arguments,
                base_dir,
                config,
                env_manager,
            )
            .await
    }
}
