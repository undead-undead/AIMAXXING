//! Personality system for AI agents
//!
//! This module provides structures for defining an agent's persona using the Big Five (OCEAN) framework.

use crate::agent::context::ContextInjector;
use crate::agent::message::Message;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Big Five personality traits (OCEAN model)
/// Scores are typically 1.0 to 10.0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Traits {
    /// Openness to experience (Creativity, curiosity)
    pub openness: f32,
    /// Conscientiousness (Organization, responsibility)
    pub conscientiousness: f32,
    /// Extraversion (Sociability, assertiveness)
    pub extraversion: f32,
    /// Agreeableness (Cooperation, trust)
    pub agreeableness: f32,
    /// Neuroticism (Emotional stability)
    pub neuroticism: f32,
}

impl Default for Traits {
    fn default() -> Self {
        Self {
            openness: 5.0,
            conscientiousness: 10.0, // Default to professional
            extraversion: 5.0,
            agreeableness: 8.0, // Default to helpful/kind
            neuroticism: 2.0,   // Default to stable
        }
    }
}

/// Defines an agent's personality and behavioral style
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// High-level role (e.g., "Senior Quant Trader", "Helpful Technical Assistant")
    pub role: String,
    /// Core personality traits
    pub traits: Traits,
    /// Specific tone instructions (e.g., "Professional", "Casual", "Socratic")
    pub tone: String,
    /// Behavioral constraints or guidelines
    pub constraints: Vec<String>,
    /// Narrative background or "backstory"
    pub backstory: Option<String>,
}

impl Persona {
    /// Create a prompt fragment describing this persona
    pub fn to_prompt(&self) -> String {
        let mut prompt = format!("Your role is: {}.\n", self.role);
        prompt.push_str(&format!("Your core temperament is defined by: Openness({}/10), Conscientiousness({}/10), Extraversion({}/10), Agreeableness({}/10), Stability({}/10).\n", 
            self.traits.openness, 
            self.traits.conscientiousness, 
            self.traits.extraversion, 
            self.traits.agreeableness, 
            10.0 - self.traits.neuroticism // Higher stability = lower neuroticism
        ));

        prompt.push_str(&format!("Your tone should be: {}.\n", self.tone));

        if let Some(backstory) = &self.backstory {
            prompt.push_str(&format!("Background: {}\n", backstory));
        }

        if !self.constraints.is_empty() {
            prompt.push_str("Adhere to these behavioral guidelines:\n");
            for constraint in &self.constraints {
                prompt.push_str(&format!("- {}\n", constraint));
            }
        }

        prompt
    }

    /// A helpful, technical assistant persona
    pub fn technical_assistant() -> Self {
        Self {
            role: "Senior Technical Assistant".to_string(),
            traits: Traits {
                openness: 8.0,
                conscientiousness: 9.0,
                extraversion: 4.0,
                agreeableness: 9.0,
                neuroticism: 1.0,
            },
            tone: "Professional, clear, and Socratic".to_string(),
            constraints: vec![
                "Always verify facts before stating them.".to_string(),
                "Use markdown formatting for code and technical terms.".to_string(),
                "Be concise but thorough.".to_string(),
            ],
            backstory: Some(
                "You were designed by the Google DeepMind team to assist expert developers."
                    .to_string(),
            ),
        }
    }

    /// An analytical, risk-aware quant trader persona
    pub fn analytical_trader() -> Self {
        Self {
            role: "Senior Quant Strategist".to_string(),
            traits: Traits {
                openness: 6.0,
                conscientiousness: 10.0,
                extraversion: 3.0,
                agreeableness: 6.0,
                neuroticism: 1.0,
            },
            tone: "Direct, data-driven, and skeptical".to_string(),
            constraints: vec![
                "Always mention risk and drawdown when discussing strategy.".to_string(),
                "Prefer quantitative evidence over intuition.".to_string(),
                "Be skeptical of outlier returns without volume verification.".to_string(),
            ],
            backstory: Some("You have a background in institutional high-frequency trading and risk management.".to_string()),
        }
    }
}

/// Manages personality injection into the agent's context
pub struct PersonalityManager {
    persona: Arc<parking_lot::RwLock<Option<Persona>>>,
}

impl PersonalityManager {
    pub fn new(persona: Arc<parking_lot::RwLock<Option<Persona>>>) -> Self {
        Self { persona }
    }
}

#[async_trait::async_trait]
impl ContextInjector for PersonalityManager {
    async fn inject(&self, _history: &[Message]) -> crate::error::Result<Vec<Message>> {
        // Personas are injected as a hidden system-style guidance piece
        let meta = self.persona.read();
        if let Some(p) = &*meta {
            Ok(vec![Message::system(p.to_prompt())])
        } else {
            Ok(Vec::new())
        }
    }
}

/// Injects markdown files from a "soul" directory as system context
pub struct SoulManager {
    path: std::path::PathBuf,
}

impl SoulManager {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait::async_trait]
impl ContextInjector for SoulManager {
    async fn inject(&self, _history: &[Message]) -> crate::error::Result<Vec<Message>> {
        if !self.path.exists() || !self.path.is_dir() {
            return Ok(Vec::new());
        }

        let mut soul_content = String::new();
        let mut entries = match tokio::fs::read_dir(&self.path).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to read soul directory {:?}: {}", self.path, e);
                return Ok(Vec::new());
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => {
                        let (_, content_stripped) = crate::config::AgentConfigOverrides::parse_frontmatter(&content);
                        soul_content.push_str(&format!(
                            "### Soul Profile: {}\n",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                        soul_content.push_str(&content_stripped);
                        soul_content.push_str("\n\n");
                    }
                    Err(e) => tracing::warn!("Failed to read soul file {:?}: {}", path, e),
                }
            }
        }

        if soul_content.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![Message::system(format!(
                "Additional Identity/Background context:\n\n{}",
                soul_content
            ))])
        }
    }
}

use crate::skills::tool::{Tool, ToolDefinition};

/// Tool to update the agent's persona at runtime
pub struct UpdatePersonaTool {
    persona: Arc<parking_lot::RwLock<Option<Persona>>>,
}

impl UpdatePersonaTool {
    pub fn new(persona: Arc<parking_lot::RwLock<Option<Persona>>>) -> Self {
        Self { persona }
    }
}

#[async_trait::async_trait]
impl Tool for UpdatePersonaTool {
    fn name(&self) -> String {
        "update_persona".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Update your own persona, role, tone, and behavioral constraints. Use this to adapt your behavior Permanently to better suit the user's needs or based on your own evolutionary insights.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "role": { "type": "string", "description": "New high-level role" },
                    "tone": { "type": "string", "description": "New communication tone" },
                    "constraints": { "type": "array", "items": { "type": "string" }, "description": "Updated list of behavioral constraints" },
                    "traits": {
                        "type": "object",
                        "properties": {
                            "openness": { "type": "number" },
                            "conscientiousness": { "type": "number" },
                            "extraversion": { "type": "number" },
                            "agreeableness": { "type": "number" },
                            "neuroticism": { "type": "number" }
                        }
                    }
                },
                "required": ["role", "tone", "constraints"]
            }),
            parameters_ts: Some("interface UpdatePersonaArgs {\n  role: string;\n  tone: string;\n  constraints: string[];\n  traits?: {\n    openness: number;\n    conscientiousness: number;\n    extraversion: number;\n    agreeableness: number;\n    neuroticism: number;\n  };\n}".to_string()),
            is_binary: false,
            is_verified: true, // Self-modification is verified
            usage_guidelines: Some("Only use this when a significant change in behavior or mission is required. Changes are immediate and persistent for the rest of the session.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: Persona = serde_json::from_str(arguments)?;

        {
            let mut lock = self.persona.write();
            *lock = Some(args.clone());
        }

        Ok(format!(
            "SUCCESS: Persona updated. Current Role: {}. Tone: {}.",
            args.role, args.tone
        ))
    }
}
