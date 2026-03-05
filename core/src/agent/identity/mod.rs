//! Phase 15: Identity Layering — SOUL + IDENTITY dual-layer architecture.
//!
//! Separates agent identity into two layers:
//! - SOUL.md: Core personality, immutable values, system defense directives
//! - IDENTITY.md: Visual aesthetics, communication tone, scenario settings
//!
//! Also provides the `.vessel` packaging format for portable agent export.

pub mod vessel_pack;

use std::path::{Path, PathBuf};

/// Represents the dual-layer identity of an agent
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayeredIdentity {
    /// Role name
    pub role: String,
    /// SOUL layer: core personality and values (immutable foundation)
    pub soul: String,
    /// IDENTITY layer: tone, aesthetics, scenario (mutable overlay)
    pub identity: String,
}

impl LayeredIdentity {
    /// Load a layered identity from a role directory
    pub async fn load(role_dir: &Path) -> anyhow::Result<Self> {
        let role = role_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Load SOUL: try SOUL.md first, fall back to PERSONA.md
        let soul_path = role_dir.join("SOUL.md");
        let persona_path = role_dir.join("PERSONA.md");
        let soul = if soul_path.exists() {
            tokio::fs::read_to_string(&soul_path).await?
        } else if persona_path.exists() {
            tokio::fs::read_to_string(&persona_path).await?
        } else {
            String::new()
        };

        // Load IDENTITY
        let identity_path = role_dir.join("IDENTITY.md");
        let identity = if identity_path.exists() {
            tokio::fs::read_to_string(&identity_path).await?
        } else {
            String::new()
        };

        Ok(Self { role, soul, identity })
    }

    /// Save the layered identity to a role directory
    pub async fn save(&self, role_dir: &Path) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(role_dir).await?;

        if !self.soul.is_empty() {
            tokio::fs::write(role_dir.join("SOUL.md"), &self.soul).await?;
        }
        if !self.identity.is_empty() {
            tokio::fs::write(role_dir.join("IDENTITY.md"), &self.identity).await?;
        }

        Ok(())
    }

    /// Compose the full system prompt from both layers
    pub fn compose_system_prompt(&self) -> String {
        let mut prompt = String::new();

        if !self.soul.is_empty() {
            prompt.push_str("## SOUL (Core Personality)\n\n");
            prompt.push_str(&self.soul);
            prompt.push_str("\n\n");
        }

        if !self.identity.is_empty() {
            prompt.push_str("## IDENTITY (Current Persona)\n\n");
            prompt.push_str(&self.identity);
        }

        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_system_prompt() {
        let identity = LayeredIdentity {
            role: "assistant".into(),
            soul: "Be helpful, precise, and concise.".into(),
            identity: "Tone: professional. Style: minimal.".into(),
        };
        let prompt = identity.compose_system_prompt();
        assert!(prompt.contains("SOUL"));
        assert!(prompt.contains("IDENTITY"));
        assert!(prompt.contains("Be helpful"));
        assert!(prompt.contains("professional"));
    }

    #[test]
    fn test_compose_empty_identity() {
        let identity = LayeredIdentity {
            role: "test".into(),
            soul: "Core values.".into(),
            identity: String::new(),
        };
        let prompt = identity.compose_system_prompt();
        assert!(prompt.contains("Core values"));
        assert!(!prompt.contains("IDENTITY"));
    }
}
