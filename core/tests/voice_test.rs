use aimaxxing_core::skills::tool::{SpeakTool, TranscribeTool, Tool};

#[tokio::test]
async fn test_speak_tool_definition() {
    let tool = SpeakTool::new("test-key", "/tmp");
    let def = tool.definition().await;
    
    assert_eq!(def.name, "text_to_speech");
    assert!(def.description.contains("Convert text to audio"));
    
    // Check required parameters
    let params = def.parameters;
    let required = params.get("required").and_then(|r| r.as_array()).unwrap();
    assert!(required.iter().any(|v| v.as_str() == Some("text")));
}

#[tokio::test]
async fn test_transcribe_tool_definition() {
    let tool = TranscribeTool::new("test-key");
    let def = tool.definition().await;
    
    assert_eq!(def.name, "transcribe_audio");
    assert!(def.description.contains("Transcribe audio"));
    
    // Check required parameters
    let params = def.parameters;
    let required = params.get("required").and_then(|r| r.as_array()).unwrap();
    assert!(required.iter().any(|v| v.as_str() == Some("file_path")));
}
