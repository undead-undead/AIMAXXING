use std::sync::Arc;
use std::path::PathBuf;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
#[cfg(feature = "http")]
use crate::skills::compiler::GithubCompiler;
use crate::skills::{SkillLoader, DynamicSkill, SkillMetadata};
use crate::skills::tool::{Tool, ToolDefinition, ToolSet};

/// Tool to forge new skills at runtime
pub struct ForgeSkill {
    loader: Arc<SkillLoader>,
    toolset: ToolSet,
    base_dir: PathBuf,
    #[cfg(feature = "http")]
    compiler: Option<GithubCompiler>,
}

impl ForgeSkill {
    #[cfg(feature = "http")]
    pub fn new(loader: Arc<SkillLoader>, toolset: ToolSet, base_dir: PathBuf, compiler: Option<GithubCompiler>) -> Self {
        Self { loader, toolset, base_dir, compiler }
    }

    #[cfg(not(feature = "http"))]
    pub fn new(loader: Arc<SkillLoader>, toolset: ToolSet, base_dir: PathBuf, _compiler: Option<()>) -> Self {
        Self { loader, toolset, base_dir }
    }

    async fn finalize_forge(&self, args: ForgeSkillArgs, skill_dir: PathBuf) -> anyhow::Result<String> {
        // 3. Write SKILL.md
        let metadata = SkillMetadata {
            name: args.name.clone(),
            description: args.description.clone(),
            homepage: None,
            parameters: None,
            interface: args.interface.clone(),
            script: Some(args.filename.clone()),
            runtime: Some(if args.runtime == "rust" { "wasm".to_string() } else { args.runtime.clone() }),
            metadata: json!({}),
            kind: "tool".to_string(),
            usage_guidelines: None,
            dependencies: Vec::new(),
            use_browser: false,
            models: Vec::new(),
        };

        let yaml = serde_yaml_ng::to_string(&metadata)?;
        let skill_md = format!("---\n{}---\n\n{}", yaml, args.instructions);
        tokio::fs::write(skill_dir.join("SKILL.md"), skill_md).await?;

        // 4. Load into memory
        let skill = DynamicSkill::new(metadata, args.instructions, skill_dir);
        let skill_arc = Arc::new(skill);
        
        // Add to loader registry
        self.loader.skills.insert(args.name.clone(), Arc::clone(&skill_arc));
        
        // Add to active toolset
        self.toolset.add_shared(skill_arc);

        Ok(format!("SUCCESS: Skill '{}' forged and loaded. You can now use it by calling '{}'.", args.name, args.name))
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ForgeSkillArgs {
    /// Technical name of the skill (snake_case)
    pub name: String,
    /// Short description of what the skill does
    pub description: String,
    /// Detailed instructions/guide for the agent on how to use it
    pub instructions: String,
    /// The source code for the skill
    pub script: String,
    /// The runtime/language (python3, node, bash)
    pub runtime: String,
    /// Filename for the script (e.g. "my_tool.py")
    pub filename: String,
    /// TypeScript interface for the parameters
    pub interface: Option<String>,
}

#[async_trait]
impl Tool for ForgeSkill {
    fn name(&self) -> String {
        "forge_skill".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Forge a new skill by providing its code, metadata, and instructions. The skill will be immediately available for use.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "instructions": { "type": "string" },
                    "script": { "type": "string" },
                    "runtime": { "type": "string", "enum": ["python3", "node", "bash", "rust"] },
                    "filename": { "type": "string" },
                    "interface": { "type": "string" }
                },
                "required": ["name", "description", "instructions", "script", "runtime", "filename"]
            }),
            parameters_ts: Some("interface ForgeSkillArgs {\n  name: string;\n  description: string;\n  instructions: string;\n  script: string;\n  runtime: string;\n  filename: string;\n  interface?: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this to create NEW capabilities that do not yet exist in your toolkit. Analyze the requirements carefully before generating code.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: ForgeSkillArgs = serde_json::from_str(arguments)?;
        
        // 1. Prepare directory
        let skill_dir = self.base_dir.join(&args.name);
        let scripts_dir = skill_dir.join("scripts");
        tokio::fs::create_dir_all(&scripts_dir).await?;

        // 2. Write script
        let script_path = scripts_dir.join(&args.filename);
        
        if args.runtime == "rust" {
            #[cfg(feature = "http")]
            {
                let compiler = self.compiler.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("GitHub compiler not configured. Cannot forge Rust skills.")
                })?;
                
                let wasm_binary = compiler.compile(&args.name, &args.script).await?;
                let wasm_filename = format!("{}.wasm", args.name);
                let wasm_path = scripts_dir.join(&wasm_filename);
                tokio::fs::write(&wasm_path, wasm_binary).await?;
                
                let mut final_args = args;
                final_args.filename = wasm_filename;
                return self.finalize_forge(final_args, skill_dir).await;
            }
            #[cfg(not(feature = "http"))]
            {
                return Err(anyhow::anyhow!("Rust skill forging requires 'http' feature (for GitHub Compiler)."));
            }
        }

        tokio::fs::write(&script_path, &args.script).await?;
        
        // Ensure script is executable for bash etc
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&script_path).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&script_path, perms).await?;
        }

        self.finalize_forge(args, skill_dir).await
    }
}
