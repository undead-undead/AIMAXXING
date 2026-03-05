use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use crate::skills::SkillLoader;
use crate::skills::tool::{Tool, ToolDefinition};

/// Tool to refine/edit existing skills
pub struct RefineSkill {
    loader: Arc<SkillLoader>,
}

impl RefineSkill {
    pub fn new(loader: Arc<SkillLoader>) -> Self {
        Self { loader }
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RefineSkillArgs {
    /// Name of the skill to refine
    pub skill_name: String,
    /// The new source code for the script
    pub new_script: String,
    /// Reason for the refinement (for logging/audit)
    pub reason: String,
}

#[async_trait]
impl Tool for RefineSkill {
    fn name(&self) -> String {
        "refine_skill".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Refine an existing skill by updating its script code. Use this to fix bugs, add features, or optimize performance of your tools.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "skill_name": { "type": "string" },
                    "new_script": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["skill_name", "new_script", "reason"]
            }),
            parameters_ts: Some("interface RefineSkillArgs {\n  skill_name: string;\n  new_script: string;\n  reason: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this to IMPROVE existing tools. Read the tool's code first using `read_skill_manual` or by checking the file, then provide the COMPLETE new script content.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: RefineSkillArgs = serde_json::from_str(arguments)?;
        
        // 1. Check if skill exists
        let skill = self.loader.skills.get(&args.skill_name)
            .ok_or_else(|| anyhow::anyhow!("Skill '{}' not found", args.skill_name))?;
            
        // 2. Identify script path
        let metadata = &skill.metadata;
        let script_filename = metadata.script.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Skill '{}' has no associated script file", args.skill_name))?;
            
        let skill_dir = self.loader.base_path.join(&args.skill_name);
        let script_path = skill_dir.join("scripts").join(script_filename);
        
        if !script_path.exists() {
             return Err(anyhow::anyhow!("Script file not found at {:?}", script_path));
        }

        // 3. Backup existing script (simple versioning)
        let backup_path = script_path.with_extension(format!("bak.{}", chrono::Utc::now().timestamp()));
        tokio::fs::copy(&script_path, &backup_path).await?;
        
        // 4. Write new content
        tokio::fs::write(&script_path, &args.new_script).await?;
        
        // 5. Reload skill
        drop(skill); // Drop ref before reloading
        
        // We need to implement a reload method on SkillLoader or use load_skill directly
        // Since load_skill is properly implemented, we can re-use logic.
        // However, we need to update the DashMap entry.
        
        let new_skill = self.loader.load_skill(&skill_dir).await?;
        
        // Re-apply shared resources (same logic as load_all)
        // Note: This matches the logic we saw in load_all

        if let Some(em) = &self.loader.env_manager {
             let new_skill_with_env = new_skill.with_env_manager(em.clone());
             self.loader.skills.insert(args.skill_name.clone(), Arc::new(new_skill_with_env));
        } else {
             self.loader.skills.insert(args.skill_name.clone(), Arc::new(new_skill));
        }

        let backup_filename = backup_path.file_name()
            .ok_or_else(|| anyhow::anyhow!("Failed to get backup filename"))?;
        Ok(format!("SUCCESS: Skill '{}' refined. Backup saved to {:?}. New version loaded and ready.", args.skill_name, backup_filename))
    }
}
