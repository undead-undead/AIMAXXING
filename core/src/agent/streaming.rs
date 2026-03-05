//! Streaming response types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;

use crate::error::Error;
use crate::agent::message::ToolCall;

/// Token usage information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Number of tokens in the prompt
    pub prompt_tokens: u32,
    /// Number of tokens in the completion
    pub completion_tokens: u32,
    /// Total number of tokens
    pub total_tokens: u32,
}

/// A chunk from a streaming response
#[derive(Debug, Clone)]
pub enum StreamingChoice {
    /// Text content chunk
    Message(String),

    /// Single tool call (sequential)
    ToolCall {
        /// Tool call ID
        id: String,
        /// Tool name
        name: String,
        /// Arguments as JSON
        arguments: serde_json::Value,
    },

    /// Multiple tool calls (parallel)
    ParallelToolCalls(HashMap<usize, ToolCall>),

    /// Thinking/reasoning chunk (e.g., Gemini's thoughts)
    Thought(String),

    /// Usage information (emitted at the end)
    Usage(Usage),

    /// Stream finished
    Done,
}

impl StreamingChoice {
    /// Check if this is a message chunk
    pub fn is_message(&self) -> bool {
        matches!(self, Self::Message(_))
    }

    /// Check if this is a tool call
    pub fn is_tool_call(&self) -> bool {
        matches!(self, Self::ToolCall { .. } | Self::ParallelToolCalls(_))
    }

    /// Check if stream is done
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done)
    }

    /// Get message text if this is a message chunk
    pub fn as_message(&self) -> Option<&str> {
        match self {
            Self::Message(s) => Some(s),
            _ => None,
        }
    }
}

/// Type alias for streaming result
pub type StreamingResult = Pin<Box<dyn Stream<Item = Result<StreamingChoice, Error>> + Send>>;

/// A wrapper for streaming responses with utility methods
pub struct StreamingResponse {
    inner: StreamingResult,
}

impl StreamingResponse {
    /// Create from a stream
    pub fn new(stream: StreamingResult) -> Self {
        Self { inner: stream }
    }

    /// Create from any stream that implements the right traits
    pub fn from_stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<StreamingChoice, Error>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }

    /// Collect all message chunks into a single string
    pub async fn collect_text(mut self) -> Result<String, Error> {
        use futures::StreamExt;

        let mut result = String::new();
        while let Some(chunk) = self.inner.next().await {
            match chunk? {
                StreamingChoice::Message(text) => result.push_str(&text),
                StreamingChoice::Done => break,
                _ => {}
            }
        }
        Ok(result)
    }

    /// Get the inner stream
    pub fn into_inner(self) -> StreamingResult {
        self.inner
    }
}

impl Stream for StreamingResponse {
    type Item = Result<StreamingChoice, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

/// Builder for creating mock streams (useful for testing)
pub struct MockStreamBuilder {
    chunks: Vec<Result<StreamingChoice, Error>>,
}

impl Default for MockStreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockStreamBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    /// Add a message chunk
    pub fn message(mut self, text: impl Into<String>) -> Self {
        self.chunks.push(Ok(StreamingChoice::Message(text.into())));
        self
    }

    /// Add a tool call
    pub fn tool_call(
        mut self,
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        self.chunks.push(Ok(StreamingChoice::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        }));
        self
    }

    /// Add done marker
    pub fn done(mut self) -> Self {
        self.chunks.push(Ok(StreamingChoice::Done));
        self
    }

    /// Add an error
    pub fn error(mut self, error: Error) -> Self {
        self.chunks.push(Err(error));
        self
    }

    /// Add usage info
    pub fn usage(mut self, usage: Usage) -> Self {
        self.chunks.push(Ok(StreamingChoice::Usage(usage)));
        self
    }

    /// Build the stream
    pub fn build(self) -> StreamingResponse {
        StreamingResponse::from_stream(futures::stream::iter(self.chunks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_streaming_response() {
        let stream = MockStreamBuilder::new()
            .message("Hello, ")
            .message("world!")
            .done()
            .build();

        let text = stream.collect_text().await.expect("collect should succeed");
        assert_eq!(text, "Hello, world!");
    }

    #[tokio::test]
    async fn test_stream_iteration() {
        let mut stream = MockStreamBuilder::new()
            .message("chunk1")
            .message("chunk2")
            .done()
            .build();

        let mut messages = Vec::new();
        while let Some(chunk) = stream.next().await {
            if let Ok(StreamingChoice::Message(text)) = chunk {
                messages.push(text);
            }
        }

        assert_eq!(messages, vec!["chunk1", "chunk2"]);
    }
}
