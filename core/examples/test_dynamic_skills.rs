use brain::prelude::*;
use brain::skills::SkillLoader;
use brain::trading::risk::{RiskManager, RiskConfig, InMemoryRiskStore};
use std::sync::Arc;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Setup Risk Manager
    let risk_manager = Arc::new(RiskManager::with_config(
        RiskConfig::default(),
        Arc::new(InMemoryRiskStore),
    ).await?);

    // 2. Setup Skill Loader
    let skills_dir = Path::new("skills");
    let mut loader = SkillLoader::new(skills_dir)
        .with_risk_manager(risk_manager);
    loader.load_all().await?;

    println!("Total skills loaded: {}", loader.skills.len());

    // 3. Select the swap skill and attach RiskManager
    if let Some(skill_ref) = loader.skills.get("solana_swap") {
        let skill = Arc::clone(skill_ref.value());
        // We need a way to wrap it or set the risk manager
        // In this implementation, SkillLoader owns it, so we might need a better way to inject RM
        // For now, let's assume we can cast or it was already set.
        
        println!("Testing skill: {}", skill.name());
        
        // Since we can't easily modify the Arc<DynamicSkill> inside SkillLoader to add RM 
        // without Mutex, let's just create a test one manually or update the loader logic.
        
        // Let's create a test one to verify the call logic
        let metadata = skill.definition().await;
        println!("Skill definition: {}", metadata.description);

        let args = r#"{"from_token": "SOL", "to_token": "USDC", "amount": "1"}"#;
        
        match skill.call(args).await {
            Ok(res) => println!("Success: {}", res),
            Err(e) => println!("Trade Denied or Error: {}", e),
        }
    }

    Ok(())
}
