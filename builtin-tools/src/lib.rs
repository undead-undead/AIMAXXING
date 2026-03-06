pub mod capabilities;
pub mod compiler;
// sandbox module moved to 'security' crate
pub mod tool;

use dashmap::DashMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::info;

use brain::agent::context::ContextInjector;
use brain::agent::message::Message;
use brain::error::{Error, Result};
use brain::skills::tool::{Tool, ToolDefinition};

use runtimes::{SkillMetadata, SkillExecutionConfig, ModelSpec, SkillRuntime};

/// A skill that executes an external script
pub struct DynamicSkill {
    metadata: SkillMetadata,
    instructions: String,
    base_dir: PathBuf,
    execution_config: SkillExecutionConfig,
    env_manager: Option<Arc<brain::env::EnvManager>>,
    runtime: Option<Arc<dyn SkillRuntime>>,
}

impl DynamicSkill {
    /// Create a new dynamic skill
    pub fn new(metadata: SkillMetadata, instructions: String, base_dir: PathBuf) -> Self {
        let execution_config = SkillExecutionConfig {
            use_browser: metadata.use_browser,
            ..Default::default()
        };

        Self {
            metadata,
            instructions,
            base_dir,
            execution_config,
            env_manager: None,
            runtime: None,
        }
    }

    pub fn with_runtime(mut self, runtime: Arc<dyn SkillRuntime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// Set custom execution configuration
    pub fn with_execution_config(mut self, config: SkillExecutionConfig) -> Self {
        self.execution_config = config;
        self
    }

    /// Set an environment manager for auto-provisioning
    pub fn with_env_manager(mut self, env_manager: Arc<brain::env::EnvManager>) -> Self {
        self.env_manager = Some(env_manager);
        self
    }

    /// Access metadata
    pub fn metadata(&self) -> &SkillMetadata {
        &self.metadata
    }
}

#[async_trait]
impl Tool for DynamicSkill {
    fn name(&self) -> String {
        self.metadata.name.clone()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.metadata.name.clone(),
            description: self.metadata.description.clone(),
            parameters: self.metadata.parameters.clone().unwrap_or(json!({})),
            parameters_ts: self.metadata.interface.clone(),
            is_binary: self.metadata.runtime.as_deref() == Some("wasm"),
            is_verified: false, // Default to unverified
            usage_guidelines: self.metadata.usage_guidelines.clone(),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        use runtimes::{QuickJSRuntime, SkillRuntime, MicroPythonRuntime, SmartNodeRuntime};
        let runtime_type = self.metadata.runtime.as_deref().unwrap_or("python3");

        info!(tool = %self.name(), runtime = %runtime_type, "Dispatching skill execution");

        let runtime = if let Some(ref r) = self.runtime {
            Arc::clone(r)
        } else {
            // Fallback: create a temporary runtime based on type
            match runtime_type {
                "qjs" | "quickjs" => Arc::new(QuickJSRuntime::new()) as Arc<dyn SkillRuntime>,
                "js" | "javascript" | "node" => Arc::new(SmartNodeRuntime::new()),
                "python" | "python3" => Arc::new(MicroPythonRuntime::with_skill_context(
                    self.metadata.name.clone(),
                    self.metadata.dependencies.clone(),
                )),
                // Other types fall back to direct shell execution
                _ => Arc::new(security::sandbox::NativeShellRuntime::new()),
            }
        };

        let output = runtime.execute(
            &self.metadata,
            arguments,
            &self.base_dir,
            &self.execution_config,
            self.env_manager.as_ref(),
        ).await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(Error::ToolExecution {
                tool_name: self.name(),
                message: format!(
                    "Script error (exit code {}): {}\nStderr: {}",
                    output.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                ),
            }
            .into());
        }

        Ok(stdout)
    }
}

/// Registry and loader for dynamic skills
pub struct SkillLoader {
    pub skills: DashMap<String, Arc<DynamicSkill>>,
    pub base_path: PathBuf,
    pub(crate) env_manager: Option<Arc<brain::env::EnvManager>>,
    #[cfg(feature = "wasm")]
    pub(crate) wasm_runtime: Arc<dyn runtimes::SkillRuntime>,
}

impl SkillLoader {
    /// Create a new registry
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            skills: DashMap::new(),
            base_path: base_path.into(),
            env_manager: None,
            #[cfg(feature = "wasm")]
            wasm_runtime: Arc::new(runtimes::WasmRuntime::new()),
        }
    }

    /// Set an environment manager for all loaded skills
    pub fn with_env_manager(mut self, env_manager: Arc<brain::env::EnvManager>) -> Self {
        self.env_manager = Some(env_manager);
        self
    }

    /// Load all skills from the base directory
    pub async fn load_all(&self) -> Result<()> {
        if !self.base_path.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&self.base_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(mut skill) = self.load_skill(&path).await {
                    if let Some(ref em) = self.env_manager {
                        skill = skill.with_env_manager(Arc::clone(em));
                    }
                    
                    // Assign runtime based on metadata
                    use runtimes::{QuickJSRuntime, SkillRuntime, MicroPythonRuntime, SmartNodeRuntime};
                    let runtime_type = skill.metadata().runtime.as_deref().unwrap_or("bash");
                    let runtime: Arc<dyn SkillRuntime> = match runtime_type {
                        "qjs" | "quickjs" => Arc::new(QuickJSRuntime::new()),
                        "js" | "javascript" | "node" => Arc::new(SmartNodeRuntime::new()),
                        "python" | "python3" => Arc::new(MicroPythonRuntime::with_skill_context(
                            skill.name(),
                            skill.metadata().dependencies.clone(),
                        )),
                        #[cfg(feature = "wasm")]
                        "wasm" => Arc::clone(&self.wasm_runtime) as Arc<dyn SkillRuntime>,
                        // Fallback
                        _ => Arc::new(security::sandbox::NativeShellRuntime::new()),
                    };
                    
                    skill = skill.with_runtime(runtime);

                    info!("Loaded dynamic skill: {}", skill.name());
                    self.skills.insert(skill.name(), Arc::new(skill));
                }
            }
        }
        Ok(())
    }

    pub async fn load_skill(&self, path: &Path) -> Result<DynamicSkill> {
        let (metadata, instructions) = crate::compiler::SkillParser::parse_file(path).await?;

        Ok(DynamicSkill::new(
            metadata,
            instructions,
            path.to_path_buf(),
        ))
    }
}

#[async_trait::async_trait]
impl ContextInjector for SkillLoader {
    async fn inject(&self, _history: &[Message]) -> Result<Vec<Message>> {
        if self.skills.is_empty() {
            return Ok(Vec::new());
        }

        let mut content = String::from("## Available Skills\n\n");
        content.push_str("You have the following skills available via `read_skill_manual`:\n\n");

        for skill_ref in self.skills.iter() {
            let skill = skill_ref.value();
            content.push_str(&format!(
                "- **{}**: {}\n",
                skill.name(),
                skill.metadata.description
            ));
        }

        content.push_str(
            "\nUse `read_skill_manual(skill_name)` to see full instructions for any skill.\n",
        );

        Ok(vec![Message::system(content)])
    }
}

/// Tool to read the full SKILL.md guide for a specific skill
pub struct ReadSkillDoc {
    loader: Arc<SkillLoader>,
}

impl ReadSkillDoc {
    pub fn new(loader: Arc<SkillLoader>) -> Self {
        Self { loader }
    }
}

#[async_trait]
impl Tool for ReadSkillDoc {
    fn name(&self) -> String {
        "read_skill_manual".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Read the full SKILL.md manual for a specific skill to understand its parameters and usage examples.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "The name of the skill to read documentation for"
                    }
                },
                "required": ["skill_name"]
            }),
            parameters_ts: Some("interface ReadSkillArgs {\n  skill_name: string; // The name of the skill to read manual for\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            skill_name: String,
        }
        let args: Args = serde_json::from_str(arguments)?;

        if let Some(skill) = self.loader.skills.get(&args.skill_name) {
            Ok(format!(
                "# Skill: {}\n\n{}",
                skill.name(),
                skill.instructions
            ))
        } else {
            Err(anyhow::anyhow!(
                "Skill '{}' not found in registry",
                args.skill_name
            ))
        }
    }
}
/// Tool to search and install skills from Smithery using CLI (npm/pnpm/bun)
#[cfg(feature = "http")]
pub struct SmitheryTool {
    loader: Arc<SkillLoader>,
}

#[cfg(feature = "http")]
impl SmitheryTool {
    pub fn new(loader: Arc<SkillLoader>) -> Self {
        Self { loader }
    }
}

#[cfg(feature = "http")]
#[async_trait]
impl Tool for SmitheryTool {
    fn name(&self) -> String {
        "smithery_manager".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Search and install new skills from the Smithery.ai registry. Supports 'search' to find skills and 'install' to add them to your environment.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["search", "install"],
                        "description": "The action to perform"
                    },
                    "query": {
                        "type": "string",
                        "description": "Search query or skill slug to install"
                    },
                    "manager": {
                        "type": "string",
                        "enum": ["npm", "pnpm", "bun"],
                        "description": "The package manager to use (default: npm)"
                    }
                },
                "required": ["action", "query"]
            }),
            parameters_ts: Some("interface ClawHubArgs {\n  action: 'search' | 'install';\n  query: string; // Search query or skill slug\n  manager?: 'npm' | 'pnpm' | 'bun'; // Package manager (default: npm)\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args {
            action: String,
            query: String,
            manager: Option<String>,
        }
        let args: Args = serde_json::from_str(arguments)?;

        let manager = args.manager.as_deref().unwrap_or({
            if cfg!(windows) { "bun" } else { "npm" }
        });
        let (cmd, base_args) = match manager {
            "pnpm" => ("pnpm", vec!["dlx", "smithery@latest"]),
            "bun" => ("bunx", vec!["smithery@latest"]),
            "pixi" => ("pixi", vec!["run", "bunx", "smithery@latest"]),
            _ => ("npx", vec!["smithery@latest"]),
        };

        match args.action.as_str() {
            "search" => {
                info!(
                    "Searching Smithery registry for: {} (via {})",
                    args.query, manager
                );
                let output = tokio::process::Command::new(cmd)
                    .args(&base_args)
                    .arg("search")
                    .arg(&args.query)
                    .output()
                    .await?;

                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
            "install" => {
                info!(
                    "Installing skill from Smithery: {} (via {})",
                    args.query, manager
                );
                let output = tokio::process::Command::new(cmd)
                    .args(&base_args)
                    .arg("install")
                    .arg(&args.query)
                    .output()
                    .await?;

                if output.status.success() {
                    // Refresh the loader to pick up the new skill
                    info!(
                        "Skill {} installed successfully, refreshing registry...",
                        args.query
                    );
                    self.loader.load_all().await?;
                    Ok(format!(
                        "Successfully installed '{}'. It is now available for use.",
                        args.query
                    ))
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    Err(anyhow::anyhow!("Failed to install skill: {}", err))
                }
            }
            _ => Err(anyhow::anyhow!("Unknown action: {}", args.action)),
        }
    }
}
