use brain::prelude::*;
use std::sync::Arc;

#[tokio::test]
async fn test_persona_injection() {
    let persona = Persona::analytical_trader();
    
    // Create a mock provider that doesn't really matter for this test
    // We just want to see if build_context works
    let _config = AgentConfig {
        persona: Some(persona.clone()),
        ..Default::default()
    };
    
    // We can test ContextManager directly
    let mut manager = ContextManager::new(ContextConfig::default());
    let persona_shared = Arc::new(parking_lot::RwLock::new(Some(persona)));
    manager.add_injector(Box::new(brain::agent::personality::PersonalityManager::new(persona_shared)));
    
    let history = vec![Message::user("Hello")];
    let strategy = brain::agent::attempt::Strategy::Standard;
    let context = manager.build_context(&history, &strategy).await.expect("Failed to build context");
    
    // Should have: Personality System Prompt + User Message
    assert!(context.len() >= 2);
    
    let system_msg = &context[0];
    assert_eq!(system_msg.role, Role::System);
    
    let content = system_msg.content.as_text();
    assert!(content.contains("Senior Quant Strategist"));
    assert!(content.contains("Direct, data-driven, and skeptical"));
    assert!(content.contains("Stability(9/10)")); // 10 - neuroticism(1)
}

#[test]
fn test_persona_prompt_generation() {
    let persona = Persona::technical_assistant();
    let prompt = persona.to_prompt();
    
    assert!(prompt.contains("Senior Technical Assistant"));
    assert!(prompt.contains("Agreeableness(9/10)"));
    assert!(prompt.contains("Socratic"));
}
