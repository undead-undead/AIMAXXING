/// Example: Strategy Pipeline
///
/// Demonstrates how to define a data-driven trading strategy and execute it.
/// AIMAXXING strategies are composed of:
/// - **Conditions**: Triggers (e.g., Price > X)
/// - **Actions**: Outcomes (e.g., Swap, Notify)
/// 
/// The Pipeline engine manages the execution flow.

use brain::prelude::*;
use brain::trading::strategy::{FileStrategyStore, PriceDirection, StrategyStore};
use brain::trading::pipeline::{Pipeline, Step, Context};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

// Mock Step for fetching price (in real app, this calls an API)
struct FetchPriceStep {
    token: String,
    mock_price: Decimal,
}

#[async_trait]
impl Step for FetchPriceStep {
    fn name(&self) -> &str { "Fetch Price" }
    
    async fn execute(&self, ctx: &mut Context) -> Result<()> {
        println!("  [Step] Fetching price for {}...", self.token);
        // Inject mock price into context
        ctx.set(
            &format!("price_{}", self.token), 
            serde_json::to_value(self.mock_price.to_f64().unwrap_or(0.0))?
        );
        Ok(())
    }
}

// Step to evaluate the strategy
struct EvaluateStrategyStep {
    strategy: Strategy,
}

#[async_trait]
impl Step for EvaluateStrategyStep {
    fn name(&self) -> &str { "Eval Strategy" }

    async fn execute(&self, ctx: &mut Context) -> Result<()> {
        println!("  [Step] Evaluating condition: {:?}", self.strategy.condition);
        
        let should_trigger = match &self.strategy.condition {
            Condition::PriceAbove { token, threshold } => {
                let key = format!("price_{}", token);
                if let Some(val) = ctx.get(&key) {
                    let price_f64 = val.as_f64().unwrap_or(0.0);
                    let price = Decimal::from_f64_retain(price_f64).unwrap_or(Decimal::ZERO);
                    println!("         Current Price: ${} | Threshold: ${}", price, threshold);
                    price > *threshold
                } else {
                    false
                }
            },
            _ => false, // Simplification for demo
        };

        if should_trigger {
            println!("  ✅ Condition MET! Executing actions: {:?}", self.strategy.actions);
        } else {
            println!("  ❌ Condition NOT met.");
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // 1. Define a Strategy (Data)
    // "If SOL > $200, buy $100 worth"
    let strategy = Strategy {
        id: "strat_1".to_string(),
        user_id: "user_1".to_string(),
        name: "Buy SOL Breakout".to_string(),
        description: Some("Demo strategy".to_string()),
        active: true,
        created_at: Utc::now().timestamp(),
        condition: Condition::PriceAbove {
            token: "SOL".to_string(),
            threshold: dec!(200.0),
        },
        actions: vec![
            Action::Swap {
                from_token: "USDC".to_string(),
                to_token: "SOL".to_string(),
                amount: "100".to_string(),
            },
            Action::Notify {
                channel: NotifyChannel::Telegram,
                message: "SOL breakout! Buying $100.".to_string(),
            }
        ],
    };

    println!("📋 Loaded Strategy: {}", strategy.name);

    // 2. Build Pipeline
    // A pipeline is a sequence of steps.
    // We mix data fetching + logic evaluation.
    // Note: We pass structs directly, not Boxed, as Pipeline::add_step takes impl Step
    let pipeline = Pipeline::new("Trade Executor")
        .add_step(FetchPriceStep { token: "SOL".to_string(), mock_price: dec!(205.50) })
        .add_step(EvaluateStrategyStep { strategy });

    // 3. Execute
    println!("\n🚀 Running Pipeline...");
    pipeline.run("Start Trigger").await?;
    println!("✨ Pipeline finished.");

    Ok(())
}
