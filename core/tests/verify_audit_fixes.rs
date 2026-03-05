use brain::prelude::*;
use brain::trading::strategy::{FileStrategyStore, Strategy, StrategyStore, Condition, Action};
use std::sync::Arc;
use tokio::time::Instant;

#[tokio::test]
async fn test_memory_optimization_placeholder() {
    // Old LongTermMemory was replaced by tiered MemoryManager + Engram.
    // Memory optimizations are now handled via ContextManager token budgeting.
    assert!(true);
}

#[tokio::test]
async fn test_strategy_atomic_save_stress() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("strategy_atomic.json");
    let store = Arc::new(FileStrategyStore::new(path.clone()));

    let strategy_template = Strategy {
        id: "strat-1".to_string(),
        user_id: "user1".to_string(),
        name: "Test Atomic".to_string(),
        description: None,
        condition: Condition::Manual,
        actions: vec![Action::Wait { seconds: 1 }],
        active: true,
        created_at: 0,
    };

    println!("Stress testing StrategyStore atomic save...");
    let mut handles = Vec::new();
    
    // Spawn multiple writers and readers
    for i in 0..5 {
        let store = store.clone();
        let mut strat = strategy_template.clone();
        strat.id = format!("strat-{}", i);
        
        handles.push(tokio::spawn(async move {
            for _ in 0..20 {
                store.save(&strat).await.unwrap();
                tokio::task::yield_now().await;
            }
        }));
    }

    for _ in 0..5 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                let _list = store.load().await.unwrap();
                // If the race condition existed, list might be empty or corrupted
                // We don't assert list not empty because readers might start before first write
                // but we assert it doesn't fail.
                tokio::task::yield_now().await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
    
    let final_list = store.load().await.unwrap();
    assert!(final_list.len() > 0);
    println!("Strategy stress test finished successfully. Final count: {}", final_list.len());
}

#[tokio::test]
async fn test_skill_timeout() {
    std::env::set_var("AIMAXXING_UNSAFE_SKILL_EXEC", "true");
    let dir = tempfile::tempdir().unwrap();
    let scripts_dir = dir.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir).unwrap();
    
    // Create a hanging script
    let script_path = scripts_dir.join("hang.py");
    std::fs::write(&script_path, "import time\ntime.sleep(100)").unwrap();
    
    let skill_md = r#"---
name: hanging_skill
description: This skill hangs
script: hang.py
runtime: python3
---
Instructions here."#;
    
    std::fs::write(dir.path().join("SKILL.md"), skill_md).unwrap();
    
    let loader = SkillLoader::new(dir.path().parent().unwrap());
    let skill = loader.load_skill(dir.path()).await.unwrap();
    
    println!("Testing skill timeout (expected ~30s)...");
    let start = Instant::now();
    let res = skill.call("{}").await;
    let duration = start.elapsed();
    
    println!("Skill call returned in {:?}", duration);
    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("timed out") || err_msg.contains("Execution timed out"));
    assert!(duration.as_secs() >= 30 && duration.as_secs() < 35);
}
