use aimaxxing_core::prelude::*;
use aimaxxing_core::skills::SkillLoader;
use std::sync::Arc;
use std::path::Path;

#[tokio::test]
async fn test_load_and_call_openclaw_audit_skill() -> anyhow::Result<()> {
    // 1. Setup paths
    // The skill is already installed at /home/biubiuboy/aimaxxing/skills/aimaxxing-memory-audit
    let base_path = Path::new("/home/biubiuboy/aimaxxing/skills");
    
    // 2. Initialize SkillLoader
    let loader = SkillLoader::new(base_path);
    loader.load_all().await?;
    
    // 3. Verify Skill Loaded
    let skill_name = "aimaxxing-memory-audit";
    assert!(loader.skills.contains_key(skill_name), "Skill '{}' should be loaded", skill_name);
    
    let skill = loader.skills.get(skill_name).unwrap();
    let definition = skill.definition().await;
    println!("Loaded Skill: {}", definition.name);
    println!("Description: {}", definition.description);
    
    // 4. Test execution
    // Since this skill requires bubblewrap (bwrap) in production, 
    // we use the AIMAXXING_UNSAFE_SKILL_EXEC override for the test environment.
    std::env::set_var("AIMAXXING_UNSAFE_SKILL_EXEC", "true");
    
    // Create a dummy file to scan in a controlled directory
    let temp_dir = tempfile::tempdir()?;
    let secret_file = temp_dir.path().join("leak.txt");
    std::fs::write(&secret_file, "sk-proj-1234567890abcdef1234567890abcdef1234567890abcdef")?;
    
    // Call the skill. Many dynamic skills take arguments as JSON or string.
    // Looking at SKILL.md, it just runs python3 with the script.
    // DynamicSkill::call passes arguments as sys.argv[1].
    let args = temp_dir.path().to_str().unwrap();
    
    println!("Calling skill with path: {}", args);
    let result = skill.call(args).await?;
    
    println!("Skill Output:\n{}", result);
    
    // 5. Assertions
    assert!(result.contains("Found 2 potential secret(s) exposed"), "Should detect the injected secret");
    assert!(result.contains("OpenAI Project API Key"), "Should identify the key type");

    Ok(())
}
