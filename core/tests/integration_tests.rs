//! Integration tests for brain

use brain::prelude::*;
use brain::agent::message::{Message, Role, Content, ToolCall};
use brain::agent::memory::{MemoryManager, InMemoryMemory, ShortTermMemory};
use engram::EngramMemory;
use std::sync::Arc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_message_creation() {
    let user_msg = Message::user("Hello");
    assert_eq!(user_msg.role, Role::User);
    assert_eq!(user_msg.content.as_text(), "Hello");

    let assistant_msg = Message::assistant("Hi there!");
    assert_eq!(assistant_msg.role, Role::Assistant);

    let system_msg = Message::system("You are helpful");
    assert_eq!(system_msg.role, Role::System);
}

#[test]
fn test_tool_definition() {
    let def = ToolDefinition {
        name: "get_price".to_string(),
        description: "Get token price".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": {"type": "string"}
            }
        }),
        parameters_ts: None,
        is_binary: false,
        is_verified: true,
        usage_guidelines: None,
    };

    assert_eq!(def.name, "get_price");
}

#[test]
fn test_toolset_basic() {
    use brain::skills::tool::ToolSet;

    let toolset = ToolSet::new();
    assert!(toolset.is_empty());
    assert_eq!(toolset.len(), 0);
}

#[test]
fn test_agent_config_default() {
    use brain::agent::core::AgentConfig;

    let config = AgentConfig::default();
    assert_eq!(config.model, "gpt-4o");
    assert_eq!(config.preamble, "You are a helpful AI assistant.");
    assert_eq!(config.max_tokens, Some(128000));
    assert_eq!(config.temperature, Some(0.7));
}

#[tokio::test]
async fn test_memory_short_term() {
    use brain::agent::memory::{Memory, ShortTermMemory};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_stm.json");
    let memory = ShortTermMemory::new(5, 100, path).await;
    
    memory.store("user1", None, Message::user("Message 1")).await.unwrap();
    memory.store("user1", None, Message::user("Message 2")).await.unwrap();
    memory.store("user1", None, Message::user("Message 3")).await.unwrap();
    
    assert_eq!(memory.message_count("user1", None), 3);
    
    // Test capacity limits
    memory.store("user1", None, Message::user("Message 4")).await.unwrap();
    memory.store("user1", None, Message::user("Message 5")).await.unwrap();
    memory.store("user1", None, Message::user("Message 6")).await.unwrap();
    
    assert_eq!(memory.message_count("user1", None), 5); // Should be capped
    
    // Test retrieve
    let messages = memory.retrieve("user1", None, 3).await;
    assert_eq!(messages.len(), 3);
}

#[tokio::test]
async fn test_memory_long_term_placeholder() {
    // LongTermMemory was replaced by tiered MemoryManager + specific backends (like Engram)
    // This is a placeholder for future tiered tests
    assert!(true);
}

#[test]
fn test_risk_config() {
    use brain::trading::risk::RiskConfig;

    let config = RiskConfig::default();
    assert!(config.max_single_trade_usd > Decimal::ZERO);
    assert!(config.max_daily_volume_usd > Decimal::ZERO);
}

#[tokio::test]
async fn test_risk_manager_basic_checks() {
    use brain::trading::risk::{RiskManager, RiskConfig, TradeContext, InMemoryRiskStore};
    use std::sync::Arc;

    let config = RiskConfig {
        max_single_trade_usd: dec!(10000.0),
        max_daily_volume_usd: dec!(50000.0),
        max_slippage_percent: dec!(5.0),
        min_liquidity_usd: dec!(100000.0),
        enable_rug_detection: true,
        trade_cooldown_secs: 5,
    };

    let manager = RiskManager::with_config(config, Arc::new(InMemoryRiskStore)).await.unwrap();

    // Test a valid trade
    let valid_trade = TradeContext {
        user_id: "user1".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(1000.0),
        expected_slippage: dec!(0.5),
        liquidity_usd: Some(dec!(500000.0)),
        is_flagged: false,
    };

    assert!(manager.check_and_reserve(&valid_trade).await.is_ok());

    // Test trade exceeding limit
    let large_trade = TradeContext {
        user_id: "user1".to_string(),
        from_token: "USDC".to_string(),
        to_token: "SOL".to_string(),
        amount_usd: dec!(15000.0), // Exceeds 10k limit
        expected_slippage: dec!(0.5),
        liquidity_usd: Some(dec!(500000.0)),
        is_flagged: false,
    };

    assert!(manager.check_and_reserve(&large_trade).await.is_err());
}

#[test]
fn test_strategy_condition_serialization() {
    use brain::trading::strategy::Condition;

    let condition = Condition::PriceAbove {
        token: "SOL".to_string(),
        threshold: dec!(200.0),
    };

    let json = serde_json::to_string(&condition).unwrap();
    let parsed: Condition = serde_json::from_str(&json).unwrap();
    
    match parsed {
        Condition::PriceAbove { token, threshold } => {
            assert_eq!(token, "SOL");
            assert_eq!(threshold, dec!(200.0));
        }
        _ => panic!("Wrong condition type"),
    }
}

#[tokio::test]
async fn test_simulation_basic_placeholder() {
    // BasicSimulator was removed to keep core decoupled.
    assert!(true);
}

#[test]
fn test_error_types() {
    use brain::error::Error;

    let err = Error::agent_config("Invalid model");
    assert!(matches!(err, Error::AgentConfig { .. }));

    let err2 = Error::tool_execution("my_tool", "failed to run");
    assert!(matches!(err2, Error::ToolExecution { .. }));
}

#[test]
fn test_streaming_choice_types() {
    use brain::agent::streaming::StreamingChoice;

    let msg = StreamingChoice::Message("Hello".to_string());
    assert!(matches!(msg, StreamingChoice::Message(_)));

    let tool = StreamingChoice::ToolCall {
        id: "call_1".to_string(),
        name: "get_price".to_string(),
        arguments: serde_json::json!({"symbol": "SOL"}),
    };
    
    if let StreamingChoice::ToolCall { name, .. } = tool {
        assert_eq!(name, "get_price");
    }

    let done = StreamingChoice::Done;
    assert!(matches!(done, StreamingChoice::Done));
}

#[test]
fn test_message_builder() {
    let msg = Message::user("Hello")
        .with_name("Alice");
    
    assert_eq!(msg.name, Some("Alice".to_string()));
}

#[test]
fn test_tool_call_creation() {
    let call = ToolCall::new("call_123", "get_price", serde_json::json!({"symbol": "SOL"}));
    
    assert_eq!(call.id, "call_123");
    assert_eq!(call.name, "get_price");
}

#[tokio::test]
async fn test_memory_manager() {
    let manager = MemoryManager::new(
        Arc::new(InMemoryMemory::new()),
        Arc::new(InMemoryMemory::new()),
    );
    
    // Store a message
    manager.hot_tier.store("user1", None, Message::user("Hello")).await.unwrap();
    
    // Retrieve
    let messages = manager.hot_tier.retrieve("user1", None, 10).await;
    assert_eq!(messages.len(), 1);
}
