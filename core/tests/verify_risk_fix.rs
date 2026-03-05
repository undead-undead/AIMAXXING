
#[tokio::test]
async fn test_risk_manager_negative_amount() {
    use aimaxxing_core::trading::risk::{RiskManager, TradeContext, RiskConfig, InMemoryRiskStore};
    use rust_decimal_macros::dec;

    let manager = RiskManager::new().await.unwrap();
    
    // 1. Normal trade - should pass
    let ctx = TradeContext {
        user_id: "test_user".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(100.0),
        expected_slippage: dec!(0.1),
        liquidity_usd: Some(dec!(100000.0)),
        is_flagged: false,
    };
    
    assert!(manager.check_and_reserve(&ctx).await.is_ok());
    
    // 2. Negative trade - MUST FAIL
    let attack_ctx = TradeContext {
        user_id: "test_user".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(-50000.0), 
        expected_slippage: dec!(0.1),
        liquidity_usd: Some(dec!(100000.0)),
        is_flagged: false,
    };

    let result = manager.check_and_reserve(&attack_ctx).await;
    assert!(result.is_err(), "RiskManager accepted negative amount!");
    
    // 3. Verify volume didn't change (still 100 from first trade)
    let remaining = manager.remaining_daily_limit("test_user").await;
    // Default daily limit is 50,000. Used 100. Remaining should be 49,900.
    assert_eq!(remaining, dec!(49_900.0));
}
