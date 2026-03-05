use brain::skills::tool::{GenerateImageTool, Tool};

#[tokio::test]
async fn test_generate_image_tool_definition() {
    let tool = GenerateImageTool::new("test-key", "/tmp");
    let def = tool.definition().await;
    
    assert_eq!(def.name, "generate_image");
    assert!(def.description.contains("Generate an image based on a text prompt"));
    
    // Check required parameters
    let params = def.parameters;
    let required = params.get("required").and_then(|r| r.as_array()).unwrap();
    assert!(required.iter().any(|v| v.as_str() == Some("prompt")));
}
