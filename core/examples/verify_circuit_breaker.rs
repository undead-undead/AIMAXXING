use brain::prelude::*;
use brain::risk::{RiskManager, DeadManSwitch, TradeContext};
use std::sync::Arc;
use std::path::PathBuf;
use rust_decimal_macros::dec;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup
    let stop_file = PathBuf::from("STOP_TRADING_TEST");
    if stop_file.exists() {
        std::fs::remove_file(&stop_file)?;
    }
    
    let manager = RiskManager::new().await.unwrap();
    let switch = DeadManSwitch::new(stop_file.clone());
    manager.add_check(Arc::new(switch));
    
    let context = TradeContext {
        user_id: "test".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(100.0),
        expected_slippage: dec!(0.1),
        liquidity_usd: Some(dec!(500000.0)),
        is_flagged: false,
    };
    
    println!("--- Verifying Dead Man's Switch ---");
    
    // 2. Test Normal Operation (File absent)
    match manager.check_and_reserve(&context).await {
        Ok(_) => println!("✅ Trade allowed when STOP file is absent."),
        Err(e) => {
            println!("❌ Normal trade failed: {}", e);
            std::process::exit(1);
        }
    }
    
    // 3. Test Emergency Stop (File present)
    std::fs::File::create(&stop_file)?;
    println!("> Created STOP file: {:?}", stop_file);
    
    match manager.check_and_reserve(&context).await {
        Ok(_) => {
            println!("❌ Trade ALLOWED when STOP file exists! (FAIL)");
            std::process::exit(1);
        }
        Err(e) => {
             println!("✅ Trade blocked: {}", e);
        }
    }
    
    // Cleanup
    std::fs::remove_file(stop_file)?;
    println!("--- Verification Success ---");
    Ok(())
}
