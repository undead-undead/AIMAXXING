# Providers Module

The `providers` crate manages the integration with various Large Language Model (LLM) providers. It provides a unified interface for streaming completions and handling provider-specific metadata.

## Key Features

- **Unified Interface**: Implements the `Provider` trait for multiple LLMs.
- **Supported Providers**: 
    - OpenAI
    - Anthropic (Claude)
    - Google Gemini
    - DeepSeek
    - Groq
    - Minimax
    - OpenRouter
- **Streaming Support**: Full support for server-sent events (SSE) and chunked streaming.
- **Tool Calling**: Standardized handling of tool/function calling across different provider schemas.

## Architecture

- `src/lib.rs`: Defines the `Provider` trait and common types.
- `src/openai.rs`, `src/anthropic.rs`, etc.: Provider-specific implementations.
- `src/mock.rs`: Mock provider for testing purposes.
