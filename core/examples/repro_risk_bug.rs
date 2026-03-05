
#[tokio::main]
async fn main() {
    use brain::trading::risk::{RiskManager, TradeContext, RiskConfig, InMemoryRiskStore};
    use rust_decimal_macros::dec;

    let manager = RiskManager::new().await.unwrap();
    
    // Normal trade
    let ctx = TradeContext {
        user_id: "hacker".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(100.0),
        expected_slippage: dec!(0.1),
        liquidity_usd: Some(dec!(100000.0)),
        is_flagged: false,
    };
    
    manager.check_and_reserve(&ctx).await.expect("First trade should pass");
    println!("Reserved $100");
    
    let remaining = manager.remaining_daily_limit("hacker").await;
    println!("Remaining: {}", remaining);

    // MALICIOUS trade with NEGATIVE amount
    let attack_ctx = TradeContext {
        user_id: "hacker".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(-50000.0), // CREDIT HACK
        expected_slippage: dec!(0.1),
        liquidity_usd: Some(dec!(100000.0)),
        is_flagged: false,
    };

    println!("Attempting negative amount reservation...");
    match manager.check_and_reserve(&attack_ctx).await {
        Ok(_) => println!("CRITICAL: Negative amount reservation SUCCEEDED! Risk limits are broken."),
        Err(e) => println!("Safe: Negative amount rejected: {}", e),
    }

    let remaining_after = manager.remaining_daily_limit("hacker").await;
    println!("Remaining after attack: {}", remaining_after);
}
