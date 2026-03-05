//! Message types for LLM communication

use serde::{Deserialize, Serialize};

/// Role of the message sender
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message (instructions)
    System,
    /// User message
    User,
    /// Assistant (AI) message
    Assistant,
    /// Tool result message
    Tool,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    /// Simple text content
    Text(String),
    /// Structured content with multiple parts
    Parts(Vec<ContentPart>),
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

impl Content {
    /// Create text content
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Create multi-part content
    pub fn parts(parts: Vec<ContentPart>) -> Self {
        Self::Parts(parts)
    }

    /// Get as text (concatenates parts if needed)
    pub fn as_text(&self) -> String {
        match self {
            Self::Text(t) => t.clone(),
            Self::Parts(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Soft-trim the content: keep head and tail, truncate middle.
    pub fn soft_trim(&mut self, limit: usize) {
        if limit < 100 {
            return;
        } // Too small to trim meaningfully

        match self {
            Self::Text(t) => {
                if t.len() <= limit {
                    return;
                }
                let head = &t[..limit / 2];
                let tail = &t[t.len() - limit / 2..];
                *t = format!(
                    "{}\n\n[... ({} bytes trimmed for context optimization) ...]\n\n{}",
                    head,
                    t.len() - limit,
                    tail
                );
            }
            Self::Parts(parts) => {
                for part in parts {
                    if let ContentPart::ToolResult { content, .. } = part {
                        if content.len() > limit {
                            let head = &content[..limit / 2];
                            let tail = &content[content.len() - limit / 2..];
                            *content = format!(
                                "{}\n\n[... ({} bytes trimmed) ...]\n\n{}",
                                head,
                                content.len() - limit,
                                tail
                            );
                        }
                    }
                }
            }
        }
    }

    /// Hard-clear the content: replace with a placeholder.
    pub fn hard_clear(&mut self) {
        match self {
            Self::Text(t) => {
                *t = "[CONTENT_CLEARED_TO_SAVE_CONTEXT_WINDOW]".to_string();
            }
            Self::Parts(parts) => {
                for part in parts {
                    match part {
                        ContentPart::Text { text } => {
                            *text = "[TEXT_CLEARED]".to_string();
                        }
                        ContentPart::ToolResult { content, .. } => {
                            *content = "[RESULT_CLEARED]".to_string();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

impl From<String> for Content {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<&str> for Content {
    fn from(s: &str) -> Self {
        Self::Text(s.to_string())
    }
}

/// A part of structured content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content
    Text {
        /// The text
        text: String,
    },
    /// Image content (base64 or URL)
    Image {
        /// Image source (base64 data or URL)
        source: ImageSource,
    },
    /// Tool call from assistant
    ToolCall {
        /// Unique ID for this tool call
        id: String,
        /// Name of the tool to call
        name: String,
        /// Arguments as JSON
        arguments: serde_json::Value,
    },
    /// Tool result from user
    ToolResult {
        /// ID of the tool call this is responding to
        tool_call_id: String,
        /// Optional name of the tool (required by some providers like Gemini)
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Result content
        content: String,
    },
}

/// Source for image content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64 encoded image
    Base64 {
        /// Media type (e.g., "image/png")
        media_type: String,
        /// Base64 encoded data
        data: String,
    },
    /// URL to an image
    Url {
        /// Image URL
        url: String,
    },
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the sender
    pub role: Role,
    /// Content of the message
    pub content: Content,
    /// Optional name (for multi-agent scenarios)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Phase 12-A: Whether this message is unverified (quarantined)
    #[serde(default)]
    pub unverified: bool,
    /// Phase 14: Originating collection (for utility tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_collection: Option<String>,
    /// Phase 14: Originating path (for utility tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

impl Message {
    /// Create a new message
    pub fn new(role: Role, content: impl Into<Content>) -> Self {
        Self {
            role,
            content: content.into(),
            name: None,
            unverified: false,
            source_collection: None,
            source_path: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<Content>) -> Self {
        Self::new(Role::System, content)
    }

    /// Create a user message
    pub fn user(content: impl Into<Content>) -> Self {
        Self::new(Role::User, content)
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<Content>) -> Self {
        Self::new(Role::Assistant, content)
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Content::Parts(vec![ContentPart::ToolResult {
                tool_call_id: tool_call_id.into(),
                name: None,
                content: content.into(),
            }]),
            name: None,
            unverified: false,
            source_collection: None,
            source_path: None,
        }
    }

    /// Set the tool name for a tool result message (required for Gemini)
    pub fn with_tool_name(mut self, tool_name: impl Into<String>) -> Self {
        // Since 'name' is already a field in this method (from self.name), lets use tool_name
        let tool_name = tool_name.into();

        if let Content::Parts(parts) = &mut self.content {
            for part in parts {
                if let ContentPart::ToolResult { name, .. } = part {
                    *name = Some(tool_name.clone());
                    // Typically only one tool result per message, so break
                    break;
                }
            }
        }
        self
    }

    /// Set the name for this message
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Get the text content of this message
    pub fn text(&self) -> String {
        self.content.as_text()
    }

    /// Soft-trim this message
    pub fn soft_trim(&mut self, limit: usize) {
        self.content.soft_trim(limit);
    }

    /// Hard-clear this message
    pub fn hard_clear(&mut self) {
        self.content.hard_clear();
    }
}

/// Tool call extracted from assistant response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Name of the tool
    pub name: String,
    /// Arguments as JSON
    pub arguments: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// Parse arguments into a typed struct
    pub fn parse_args<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.arguments.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text(), "Hello");
    }

    #[test]
    fn test_tool_call_parse() {
        #[derive(Deserialize)]
        struct SwapArgs {
            from: String,
            to: String,
            amount: f64,
        }

        let call = ToolCall::new(
            "call_123",
            "swap_tokens",
            serde_json::json!({
                "from": "USDC",
                "to": "SOL",
                "amount": 100.0
            }),
        );

        let args: SwapArgs = call.parse_args().expect("parse should succeed");
        assert_eq!(args.from, "USDC");
        assert_eq!(args.to, "SOL");
        assert!((args.amount - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_tool_result_name() {
        let msg = Message::tool_result("call_1", "result").with_tool_name("get_price");
        if let Content::Parts(parts) = msg.content {
            if let ContentPart::ToolResult { name, .. } = &parts[0] {
                assert_eq!(name.as_deref(), Some("get_price"));
            } else {
                panic!("Wrong part type");
            }
        }
    }
}
