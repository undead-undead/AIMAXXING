pub mod api;
pub mod blueprints;
// mcp logic moved to standalone 'mcp' crate

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersonaTemplate {
    pub name: String,
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub tools: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersonaConfig {
    pub personas: Vec<PersonaTemplate>,
}
