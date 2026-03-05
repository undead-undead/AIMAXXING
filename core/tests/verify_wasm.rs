use brain::skill::{SkillLoader, DynamicSkill};
use brain::tool::Tool;
use std::path::PathBuf;

#[tokio::test]
async fn test_wasm_skill_loading() {
    println!("Current dir: {:?}", std::env::current_dir().unwrap());
    
    // Use absolute path to ensure we find the skills directory
    let mut base_path = std::env::current_dir().unwrap();
    if base_path.ends_with("brain") {
        base_path.pop();
    }
    base_path.push("skills");
    println!("Loading skills from: {:?}", base_path);

    let mut loader = SkillLoader::new(base_path);
    loader.load_all().await.expect("Failed to load skills");
    
    let skill = loader.skills.get("wasm_test").expect("wasm_test skill not found");
    let result = skill.call(r#"{"text": "hello"}"#).await.expect("Failed to call WASM skill");
    
    assert!(result.contains("WASM Skill received: {\"text\": \"hello\"}"));
}

