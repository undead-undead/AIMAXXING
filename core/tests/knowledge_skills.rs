use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::context::ContextInjector;
use aimaxxing_core::skills::ReadSkillDoc;
use std::sync::Arc;
use tokio::fs;

#[tokio::test]
async fn test_knowledge_skill_loading_and_injection() -> anyhow::Result<()> {
    // 1. Setup temp directory for skills
    let temp_dir = std::env::temp_dir().join(format!("aimaxxing-test-skills-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).await?;
    
    let skill_dir = temp_dir.join("deployment-guide");
    fs::create_dir_all(&skill_dir).await?;

    // 2. Create a Knowledge SKILL.md
    let skill_content = r#"---
name: deployment-guide
description: How to deploy the application
kind: knowledge
---

# Deployment Instructions

1. Run cargo build --release
2. Copy binary to /usr/local/bin
3. Restart service

Notes:
- Ensure db migration is run
"#;
    
    fs::write(skill_dir.join("SKILL.md"), skill_content).await?;

    // 3. Initialize SkillLoader
    let loader = SkillLoader::new(&temp_dir);
    loader.load_all().await?;

    // 4. Verify Skill Loaded
    assert_eq!(loader.skills.len(), 1);
    let skill = loader.skills.get("deployment-guide").expect("Skill not found");
    // script should be None
    assert!(skill.metadata().script.is_none());

    // 5. Verify Context Injection
    println!("Test: Calling inject() on loader with {} skills...", loader.skills.len());
    let messages = loader.inject(&[]).await?;
    println!("Test: inject() returned {} messages", messages.len());
    assert_eq!(messages.len(), 1);
    
    let sys_msg = &messages[0];
    assert_eq!(sys_msg.role, Role::System);
    
    let content = sys_msg.content.as_text();
    println!("Injected Content:\n{}", content);

    assert!(content.contains("## Available Skills"));
    assert!(content.contains("read_skill_manual"));
    assert!(content.contains("- **deployment-guide**: How to deploy the application"));
    // instructions should stay in Skill, not injected in system prompt
    assert!(!content.contains("1. Run cargo build --release"));

    // Cleanup
    let _ = fs::remove_dir_all(temp_dir).await; // Ignore errors

    Ok(())
}

#[tokio::test]
async fn test_read_skill_manual_tool() -> anyhow::Result<()> {
    // 1. Setup temp directory for skills
    let temp_dir = std::env::temp_dir().join(format!("aimaxxing-test-read-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).await?;
    
    let skill_dir = temp_dir.join("test-tool");
    fs::create_dir_all(&skill_dir).await?;

    let skill_content = r#"---
name: test-tool
description: A test tool
---

# Test Tool Guide
Step 1. Do something.
"#;
    
    fs::write(skill_dir.join("SKILL.md"), skill_content).await?;

    // 2. Load skills
    let loader = SkillLoader::new(&temp_dir);
    loader.load_all().await?;
    let loader = Arc::new(loader);

    // 3. Test ReadSkillDoc tool
    let tool = ReadSkillDoc::new(Arc::clone(&loader));
    assert_eq!(tool.name(), "read_skill_manual");

    let result = tool.call(r#"{"skill_name": "test-tool"}"#).await?;
    assert!(result.contains("# Skill: test-tool"));
    assert!(result.contains("Step 1. Do something."));

    // 4. Test missing skill
    let error = tool.call(r#"{"skill_name": "non-existent"}"#).await;
    assert!(error.is_err());

    // Cleanup
    let _ = fs::remove_dir_all(temp_dir).await;

    Ok(())
}
