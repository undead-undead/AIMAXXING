use anyhow::Result;
use colored::*;
use dialoguer::{Input, Select, Password};
use std::fs;
use brain::config::AppConfig;

pub async fn run_onboard() -> Result<()> {
    println!("{}", "Welcome to AIMAXXING Onboarding Wizard! 🧙‍♂️".bold().purple());
    println!("{}", "Lets get you set up.\n".dimmed());

    let mut config = AppConfig::default();
    
    // 1. Provider Selection
    let providers = vec!["OpenAI", "Anthropic", "DeepSeek", "Gemini", "MiniMax"];
    let selection = Select::new()
        .with_prompt("Select your primary LLM provider")
        .default(0)
        .items(&providers)
        .interact()?;

    let provider_name = providers[selection];
    config.providers.active_provider = Some(provider_name.to_lowercase());

    // 2. API Key
    let api_key = Password::new()
        .with_prompt(format!("Enter API Key for {}", provider_name))
        .interact()?;

    match provider_name {
        "OpenAI" => config.providers.openai_api_key = Some(api_key),
        "Anthropic" => config.providers.anthropic_api_key = Some(api_key),
        "DeepSeek" => config.providers.deepseek_api_key = Some(api_key),
        "Gemini" => config.providers.gemini_api_key = Some(api_key),
        "MiniMax" => config.providers.minimax_api_key = Some(api_key),
        _ => {}
    }

    // 3. Knowledge Mode
    let knowledge_modes = vec!["Full Semantic (Recommended, ~400MB RAM)", "Light Keyword-only (Saves Memory, ~50MB RAM)"];
    let k_selection = Select::new()
        .with_prompt("Select RAG Engine Mode")
        .default(0)
        .items(&knowledge_modes)
        .interact()?;
    
    // Engram handles both keyword and vector modes automatically
    let _ = k_selection; // selection preserved for future use

    // 4. Port
    let port: u16 = Input::new()
        .with_prompt("Server Port")
        .default(3000)
        .interact_text()?;
    config.server.port = port;

    // 4. Save
    let config_path = std::env::current_dir()?.join("aimaxxing.yaml");
    println!("\nSaving configuration to {:?}...", config_path);
    
    // Helper to save yaml
    let yaml = serde_yaml_ng::to_string(&config)?;
    fs::write(&config_path, yaml)?;

    println!("\n{}", "Configuration saved! 🎉".bold().green());
    println!("You can now run: `aimaxxing-gateway web`");

    Ok(())
}
