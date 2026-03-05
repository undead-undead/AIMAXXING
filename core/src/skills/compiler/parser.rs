use crate::error::{Error, Result};
use crate::skills::SkillMetadata;
use serde_yaml_ng;
use std::path::Path;

pub struct SkillParser;

impl SkillParser {
    pub async fn parse_file(path: &Path) -> Result<(SkillMetadata, String)> {
        let manifest_path = path.join("SKILL.md");
        if !manifest_path.exists() {
            return Err(Error::Internal(format!("No SKILL.md found at {:?}", path)));
        }

        let content = tokio::fs::read_to_string(&manifest_path).await?;
        Self::parse_str(&content, path)
    }

    pub fn parse_str(content: &str, base_path: &Path) -> Result<(SkillMetadata, String)> {
        // Find frontmatter delimiters
        let start_delimiter = "---\n";
        let end_delimiter = "\n---";

        let yaml_str;
        let instructions;

        // Ensure file starts with YAML frontmatter
        if content.starts_with(start_delimiter) || content.starts_with("---\r\n") {
            // Find end of frontmatter
            if let Some(end_idx) = content[4..].find(end_delimiter) {
                let actual_end_idx = end_idx + 4; // Add back the initial offset
                yaml_str = &content[4..actual_end_idx]; 

                let rest_start = actual_end_idx + 4;
                if rest_start < content.len() {
                    instructions = content[rest_start..].trim().to_string();
                } else {
                    instructions = String::new();
                }
            } else {
                return Err(Error::Internal(
                    "SKILL.md frontmatter unclosed (missing closing ---)".to_string(),
                ));
            }
        } else {
            return Err(Error::Internal("SKILL.md must start with ---".to_string()));
        }

        let mut metadata: SkillMetadata = serde_yaml_ng::from_str(yaml_str)
            .map_err(|e| Error::Internal(format!("Failed to parse Skill YAML: {}", e)))?;

        // --- Compatibility Fixes: Inference ---
        if metadata.script.is_none() || metadata.runtime.is_none() {
            if metadata.script.is_none() {
                let scripts_dir = base_path.join("scripts");
                if scripts_dir.exists() {
                    if let Ok(mut entries) = std::fs::read_dir(scripts_dir) {
                        if let Some(Ok(first_entry)) = entries.next() {
                            let filename = first_entry.file_name().to_string_lossy().to_string();
                            metadata.script = Some(filename.clone());

                            if metadata.runtime.is_none() {
                                if filename.ends_with(".py") {
                                    metadata.runtime = Some("python3".into());
                                } else if filename.ends_with(".js") {
                                    metadata.runtime = Some("node".into());
                                } else if filename.ends_with(".sh") {
                                    metadata.runtime = Some("bash".into());
                                }
                            }
                        }
                    }
                }
            }

            if metadata.runtime.is_none() {
                if instructions.contains("python3") {
                    metadata.runtime = Some("python3".into());
                } else if instructions.contains("node") {
                    metadata.runtime = Some("node".into());
                } else if instructions.contains("bash") || instructions.contains("sh ") {
                    metadata.runtime = Some("bash".into());
                }
            }
        }

        Ok((metadata, instructions))
    }
}
