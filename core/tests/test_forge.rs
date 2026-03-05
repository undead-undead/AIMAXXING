use aimaxxing_core::agent::provider::{ChatRequest, Provider};
use aimaxxing_core::agent::streaming::StreamingResponse;
use aimaxxing_core::prelude::*;
use aimaxxing_core::skills::SkillLoader;
use async_trait::async_trait;
use std::sync::Arc;
use tempfile::tempdir;

struct MockProvider;
impl MockProvider {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn stream_completion(
        &self,
        _request: ChatRequest,
    ) -> aimaxxing_core::error::Result<StreamingResponse> {
        unimplemented!("MockProvider for test_forge doesn't need to stream")
    }
    fn name(&self) -> &'static str {
        "mock"
    }
}

#[tokio::test]
async fn test_skill_forging_and_execution() {
    std::env::set_var("AIMAXXING_UNSAFE_SKILL_EXEC", "true");
    let dir = tempdir().unwrap();
    let skills_dir = dir.path().join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    let skill_loader = Arc::new(SkillLoader::new(&skills_dir));
    let provider = MockProvider::new();

    let agent = Agent::builder(provider)
        .with_dynamic_skills(Arc::clone(&skill_loader))
        .expect("Failed to init dynamic skills")
        .build()
        .expect("Failed to build agent");

    // 1. Manually call ForgeSkill to create a "hello" tool
    let forge_args = serde_json::json!({
        "name": "hello_tool",
        "description": "A tool that says hello",
        "instructions": "Call this to say hello to anyone.",
        "script": "import sys; import json; args = json.loads(sys.argv[1]); print(f'Hello, {args[\"name\"]}!')",
        "runtime": "python3",
        "filename": "hello.py",
        "interface": "interface HelloArgs { name: string; }"
    });

    let result = agent
        .call_tool("forge_skill", &forge_args.to_string())
        .await
        .expect("Forging failed");
    assert!(result.contains("SUCCESS"));

    // 2. Verify files exist on disk
    let skill_path = skills_dir.join("hello_tool");
    assert!(skill_path.exists());
    assert!(skill_path.join("SKILL.md").exists());
    assert!(skill_path.join("scripts/hello.py").exists());

    // 3. Immediately call the new tool
    // Note: Since Agent::build auto-registers tools from the ToolSet (which is now shared via Arc),
    // the new tool should be available immediately.
    let hello_args = serde_json::json!({ "name": "AIMAXXING" });
    let hello_result = agent
        .call_tool("hello_tool", &hello_args.to_string())
        .await
        .expect("Executing forged tool failed");

    assert_eq!(hello_result.trim(), "Hello, AIMAXXING!");
}
