/// Example: Risk Management
/// 
/// Demonstrates how to use the AIMAXXING Risk Manager to enforce trading safety.
/// The `RiskManager` uses an Actor model to manage state (daily volume, drawdowns)
/// safely across concurrent agents.
///
/// checks:
/// - Max single trade size
/// - Max daily volume
/// - Slippage limits


use aimaxxing_core::risk::{RiskManager, RiskConfig, TradeContext, InMemoryRiskStore};
use std::sync::Arc;
use rust_decimal_macros::dec;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    // 1. Define Risk Configuration
    let config = RiskConfig {
        max_single_trade_usd: dec!(1000.0),
        max_daily_volume_usd: dec!(5000.0),
        max_slippage_percent: dec!(2.0),
        min_liquidity_usd: dec!(50000.0),
        enable_rug_detection: true,
        trade_cooldown_secs: 0, // Disable cooldown for this demo
    };

    println!("🛡️ Risk Limits Initialized:");
    println!("   - Max Trade: ${}", config.max_single_trade_usd);
    println!("   - Max Daily: ${}", config.max_daily_volume_usd);

    // 2. Initialize Risk Manager with InMemory store (use FileRiskStore for persistence)
    let store = Arc::new(InMemoryRiskStore);
    let manager = RiskManager::with_config(config, store).await.unwrap();

    // 3. Simulate Trades

    // Scenario A: Safe Trade
    println!("\n--- Scenario A: Safe Trade ($500) ---");
    let trade_a = TradeContext {
        user_id: "user123".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(500.0),
        expected_slippage: dec!(0.5),
        liquidity_usd: Some(dec!(1000000.0)),
        is_flagged: false,
    };

    match manager.check_and_reserve(&trade_a).await {
        Ok(_) => {
            println!("✅ Trade Approved");
            // In a real app, you would execute the swap here.
            // After success/failure, you MUST commit or rollback.
            manager.commit_trade(&trade_a.user_id, trade_a.amount_usd).await?;
            println!("📝 Trade Committed (Usage Updated)");
        },
        Err(e) => println!("❌ Trade Rejected: {}", e),
    }

    // Scenario B: Dangerous Trade (Oversized)
    println!("\n--- Scenario B: Oversized Trade ($2000) ---");
    let trade_b = TradeContext {
        user_id: "user123".to_string(),
        from_token: "USDC".to_string(),
        to_token: "BTC".to_string(),
        amount_usd: dec!(2000.0), // Limit is 1000
        expected_slippage: dec!(0.5),
        liquidity_usd: Some(dec!(1000000.0)),
        is_flagged: false,
    };

    match manager.check_and_reserve(&trade_b).await {
        Ok(_) => println!("✅ Trade Approved (Unexpected!)"),
        Err(e) => println!("❌ Trade Rejected: {}", e),
    }

    // Scenario C: Daily Limit Check
    println!("\n--- Scenario C: Max Daily Volume ---");
    println!("Attempting to trade $800 repeatedly...");
    
    for i in 1..=10 {
        let trade = TradeContext {
            user_id: "user123".to_string(),
            from_token: "USDC".to_string(),
            to_token: "SOL".to_string(),
            amount_usd: dec!(800.0),
            expected_slippage: dec!(0.1),
            liquidity_usd: Some(dec!(1000000.0)),
            is_flagged: false,
        };

        if manager.check_and_reserve(&trade).await.is_ok() {
            manager.commit_trade(&trade.user_id, trade.amount_usd).await?;
            println!("  [Trade {}] Accepted ($800)", i);
        } else {
            println!("  [Trade {}] REJECTED (Daily Limit Reached)", i);
            break;
        }
    }

    Ok(())
}
