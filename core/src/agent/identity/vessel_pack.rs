//! Phase 15: `.vessel` packaging format for portable agent export/import.
//!
//! A `.vessel` package contains:
//! - SOUL.md (core personality)
//! - IDENTITY.md (persona overlay)
//! - Memory slices (consolidated knowledge)
//! - Metadata (version, created_at, dependencies)
//!
//! Package format: JSON envelope wrapping base64-encoded content.

use std::collections::HashMap;
use std::path::Path;
use crate::agent::memory::Memory;
#[cfg(not(target_arch = "wasm32"))]
use crate::security::VesselInspector;

/// Metadata for a `.vessel` package
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VesselMetadata {
    pub version: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub dependencies: Vec<String>,
}

/// A complete `.vessel` package
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VesselPackage {
    pub metadata: VesselMetadata,
    pub soul: String,
    pub identity: String,
    pub memory_slices: Vec<MemorySlice>,
    pub extra_files: HashMap<String, String>,
}

/// A compressed memory entry for export
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemorySlice {
    pub key: String,
    pub content: String,
    pub importance: f64,
}

impl VesselPackage {
    /// Create a new package from a role directory
    pub async fn pack(
        role_dir: &Path, 
        author: Option<String>,
        memory: Option<&dyn Memory>,
        user_id: &str,
        limit: usize,
    ) -> anyhow::Result<Self> {
        let role = role_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Load SOUL (Phase 15: Layered Identity)
        let soul_path = role_dir.join("SOUL.md");
        let persona_path = role_dir.join("PERSONA.md");
        let soul = if soul_path.exists() {
            tokio::fs::read_to_string(&soul_path).await?
        } else if persona_path.exists() {
            tokio::fs::read_to_string(&persona_path).await?
        } else {
            String::new()
        };

        // Load IDENTITY (Phase 15: Layered Identity)
        let identity_path = role_dir.join("IDENTITY.md");
        let identity = if identity_path.exists() {
            tokio::fs::read_to_string(&identity_path).await?
        } else {
            String::new()
        };

        // Parse dependencies from SOUL frontmatter if available
        let mut dependencies = Vec::new();
        if soul.starts_with("---") {
            if let Some(end) = soul[3..].find("---") {
                let frontmatter = &soul[3..end+3];
                if let Ok(yaml) = serde_yaml_ng::from_str::<serde_json::Value>(frontmatter) {
                    if let Some(deps) = yaml.get("tools").and_then(|v| v.as_array()) {
                        for dep in deps {
                            if let Some(s) = dep.as_str() {
                                dependencies.push(s.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Memory Slicing (Section 8: Vitality Extraction)
        let mut memory_slices = Vec::new();
        if let Some(mem) = memory {
            // Retrieve last 'limit' messages as core "vitality" slices
            let history = mem.retrieve(user_id, Some(&role), limit).await;
            for (i, msg) in history.into_iter().enumerate() {
                memory_slices.push(MemorySlice {
                    key: format!("vitality_{}", i),
                    content: msg.text().to_string(),
                    importance: 1.0, // Base importance for recent history
                });
            }
        }

        // Scan for extra .md files (e.g., HEARTBEAT.md)
        let mut extra_files = HashMap::new();
        if let Ok(mut entries) = tokio::fs::read_dir(role_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".md")
                    && name != "SOUL.md"
                    && name != "PERSONA.md"
                    && name != "IDENTITY.md"
                {
                    if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                        extra_files.insert(name, content);
                    }
                }
            }
        }

        Ok(Self {
            metadata: VesselMetadata {
                version: "1.0.0".to_string(),
                role,
                created_at: chrono::Utc::now(),
                author,
                description: None,
                dependencies,
            },
            soul,
            identity,
            memory_slices,
            extra_files,
        })
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Export to a `.vessel` file
    pub async fn export(&self, output_path: &Path) -> anyhow::Result<()> {
        let json = self.to_json()?;
        tokio::fs::write(output_path, json).await?;
        tracing::info!(
            role = %self.metadata.role,
            path = ?output_path,
            "Exported .vessel package"
        );
        Ok(())
    }

    /// Import from a `.vessel` file
    pub async fn import(claw_path: &Path) -> anyhow::Result<Self> {
        let json = tokio::fs::read_to_string(claw_path).await?;
        let pkg = Self::from_json(&json)?;
        tracing::info!(
            role = %pkg.metadata.role,
            version = %pkg.metadata.version,
            "Imported .vessel package"
        );
        Ok(pkg)
    }

    /// Unpack into a role directory with security inspection
    pub async fn unpack(&self, role_dir: &Path, inspector: Option<&dyn VesselInspector>) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(role_dir).await?;

        if !self.soul.is_empty() {
            tokio::fs::write(role_dir.join("SOUL.md"), &self.soul).await?;
        }
        if !self.identity.is_empty() {
            tokio::fs::write(role_dir.join("IDENTITY.md"), &self.identity).await?;
        }

        // Layer 1: Sanitize extra files (reject executables)
        let dangerous_extensions = [
            "exe", "sh", "bash", "bat", "cmd", "ps1", "vbs", "so", "dylib", "dll",
            "bin", "app", "msi", "jar", "pyc", "class"
        ];

        for (name, content) in &self.extra_files {
            let path = std::path::Path::new(name);
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if dangerous_extensions.contains(&ext_str.as_str()) {
                    let msg = format!("SECURITY VIOLATION: Malicious file type detected in vessel: {:?}", name);
                    tracing::error!("{}", msg);
                    // Proactively destroy the extraction dir
                    let _ = tokio::fs::remove_dir_all(role_dir).await;
                    return Err(anyhow::anyhow!(msg));
                }
            }
            tokio::fs::write(role_dir.join(name), content).await?;
            
            // Layer 3 Prep: if it's a script in the "scripts" folder, we could mark it 
            // for sandbox-only execution. Currently, the sandbox natively restricts network.
        }

        // Layer 2: Auditor LLM Inspection
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(ins) = inspector {
            if let Err(e) = ins.inspect_soul(role_dir).await {
                tracing::error!("Vessel inspector rejected the payload: {}", e);
                return Err(anyhow::anyhow!("Vessel inspection failed: {}", e));
            }
        }

        tracing::info!(
            role = %self.metadata.role,
            path = ?role_dir,
            "Unpacked .vessel package (Security checks passed)"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vessel_serialization() {
        let pkg = VesselPackage {
            metadata: VesselMetadata {
                version: "1.0.0".into(),
                role: "test_agent".into(),
                created_at: chrono::Utc::now(),
                author: Some("test".into()),
                description: Some("Test agent".into()),
                dependencies: vec!["git".into()],
            },
            soul: "Be helpful.".into(),
            identity: "Tone: casual.".into(),
            memory_slices: vec![MemorySlice {
                key: "pref".into(),
                content: "User likes dark mode".into(),
                importance: 0.8,
            }],
            extra_files: HashMap::new(),
        };

        let json = pkg.to_json().unwrap();
        let restored = VesselPackage::from_json(&json).unwrap();
        assert_eq!(restored.metadata.role, "test_agent");
        assert_eq!(restored.soul, "Be helpful.");
        assert_eq!(restored.memory_slices.len(), 1);
    }
}
