use brain::prelude::*;
use brain::risk::{RiskConfig, RiskManager, FileRiskStore};
use std::sync::Arc;
use std::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = "test_risk_corrupt.json";
    // 1. Create a corrupt JSON file
    fs::write(path, "{ invalid json: }")?;
    
    let config = RiskConfig::default();
    let store = Arc::new(FileRiskStore::new(path));
    
    // 2. Try to initialize strict
    println!("Checking if new_strict fails on corrupted file...");
    let result = RiskManager::new_strict(config, store).await;
    
    match result {
        Ok(_) => {
            println!("FAILED: RiskManager::new_strict succeeded with corrupted file!");
            std::process::exit(1);
        }
        Err(e) => {
            println!("SUCCESS: Caught expected error: {}", e);
        }
    }
    
    // Cleanup
    fs::remove_file(path)?;
    Ok(())
}
