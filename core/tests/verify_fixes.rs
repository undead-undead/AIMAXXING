use aimaxxing_core::prelude::*;
use aimaxxing_core::agent::memory::{MemoryManager, InMemoryMemory};
use aimaxxing_engram::EngramMemory;
use aimaxxing_core::trading::risk::{RiskManager, RiskConfig};
use rust_decimal::Decimal;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_verify_memory_tiering_compilation() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test_memory.db");
    
    let hot = Arc::new(InMemoryMemory::new());
    let cold = Arc::new(aimaxxing_engram::EngramMemory::new(Arc::new(aimaxxing_engram::EngramStore::new(db_path).expect("Failed to create EngramStore"))));
    let manager = MemoryManager::new(hot, cold);
    
    manager.store("user1", None, Message::user("hello".to_string())).await?;
    let retrieved = manager.retrieve_unified("user1", None, 10).await;
    assert_eq!(retrieved.len(), 1);
    
    Ok(())
}

#[test]
fn test_verify_context_safety() {
    let config = ContextConfig {
        max_tokens: 2000,
        max_history_messages: 50,
        response_reserve: 100,
        ..Default::default()
    };
    let mut mgr = ContextManager::new(config);
    mgr.set_system_prompt("System");
    
    let long_msg = "hello ".repeat(500); 
    let history = vec![
        Message::user(long_msg.clone()),
        Message::user("recent message".to_string()),
    ];
    
    let strategy = aimaxxing_core::agent::attempt::Strategy::Standard;
    let ctx = futures::executor::block_on(mgr.build_context(&history, &strategy)).unwrap();
    assert_eq!(ctx.len(), 3);

    let huge_msg = "hello ".repeat(1500); 
    let history_huge = vec![
        Message::user(huge_msg.clone()),
        Message::user("recent message".to_string()),
    ];
    
    let ctx_pruned = futures::executor::block_on(mgr.build_context(&history_huge, &strategy)).unwrap();
    assert_eq!(ctx_pruned.len(), 2);
    assert_eq!(ctx_pruned[1].content.as_text(), "recent message");
}

#[tokio::test]
async fn test_verify_risk_zombie_cleanup() -> anyhow::Result<()> {
    use aimaxxing_core::trading::risk::{FileRiskStore};
    // 1. Create a Risk Store on disk with dirty state
    let dir = tempdir()?;
    let db_path = dir.path().join("risk.json");
    
    // Manually write JSON with pending_volume > 0
    // We replicate the RiskState serialization format
    // Map<UserId, UserState>
    use std::collections::HashMap;
    use serde_json::json;
    
    let now = chrono::Utc::now().to_rfc3339();
    let zombie_json = json!({
        "zombie_user": {
            "daily_volume_usd": "100.0",
            "pending_volume_usd": "500.0", // ZOMBIE!
            "last_trade": now,
            "volume_reset": now
        }
    });
    
    tokio::fs::write(&db_path, serde_json::to_string(&zombie_json)?).await?;
    
    // 2. Load RiskManager
    let config = RiskConfig {
        max_daily_volume_usd: Decimal::new(1000, 0),
        trade_cooldown_secs: 0,
        ..Default::default()
    };
    
    let store = Arc::new(FileRiskStore::new(db_path));
    let manager = RiskManager::with_config(config, store).await?;
    
    // 3. Verify state is cleaned
    // access state? We need check_and_reserve to see if it starts from 0 or 500.
    // The Limit is 1000. 
    // User used 100. Pending was 500.
    // If cleaned, pending is 0. 
    // check_and_reserve(amount=800).
    // If pending=0: 100+800 = 900 <= 1000. OK.
    // If pending=500: 100+500+800 = 1400 > 1000. FAIL.
    
    let ctx = aimaxxing_core::trading::risk::TradeContext {
        user_id: "zombie_user".to_string(),
        from_token: "A".into(),
        to_token: "B".into(),
        amount_usd: Decimal::new(800, 0),
        expected_slippage: Decimal::ONE,
        liquidity_usd: None,
        is_flagged: false,
    };
    
    let result = manager.check_and_reserve(&ctx).await;
    assert!(result.is_ok(), "Should allow 800 if zombie pending (500) was cleared. 100(used)+800=900 < 1000");

    Ok(())
}
