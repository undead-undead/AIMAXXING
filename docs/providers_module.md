# AIMAXXING Providers Module Documentation

The `providers` module serves as the primary integration layer between the internal `brain` of AIMAXXING Agents and multiple external Large Language Model (LLM) providers via their respective REST APIs.

## 🛠 Technology Stack & Dependencies

The entire module is asynchronous and network-heavy, leveraging:
- **HTTP Client**: `reqwest` for robust streaming and asynchronous HTTP requests to external services.
- **Serialization**: `serde` and `serde_json` to marshal structured Agent instructions, function calling schemas, and natural language into the JSON format expected by OpenAI-compatible logic.
- **Retry Logic**: A custom exponential backoff implementation (`retry.rs`) to gracefully handle rate limiting (429 Too Many Requests) and temporary server unavailability (500 Series).
- **Streaming**: `tokio-stream` and `bytes` for handling Server-Sent Events (SSE), enabling the Agent UI or MCP clients to display typing text in real-time.
- **Local Fallbacks**: `llama-cpp-2` for optional direct, in-process C++ inference bindings when users elect to run models locally without an HTTP wrapper (like Ollama).

## 📂 Architecture & Modularity

Each provider API has a dedicated file in `providers/src/`, all implementing the unified `Provider` trait defined in the `brain` (`core`). 

### Supported Providers:
- **`openai.rs`**: The reference implementation supporting `gpt-4o`, `o1`, and embeddings. Handles both standard and strict JSON structured parsing.
- **`anthropic.rs`**: A heavily-customized integration mapping the specialized Messages API and XML-like system prompt formats used by the Claude 3 and 3.5 family.
- **`gemini.rs`**: Google Gemini integration supporting its unique multi-modal structure, tools, and `generateContent` API variants.
- **`deepseek.rs`**, **`moonshot.rs`**, **`groq.rs`**, **`minimax.rs`**, **`openrouter.rs`**: Implementations mapping specific API quirks, endpoint URL overrides, or custom header structures over their underlying OpenAI-compatibility layer.
- **`ollama.rs`**: Handles local HTTP endpoints. Understands Ollama's specific tool-calling (function calling) format which often deviates subtly from standard OpenAI.
- **`llama_cpp.rs`**: Bypasses the HTTP stack entirely, interacting directly with loaded `.gguf` weights on disk using CPU/GPU memory natively.
- **`mock.rs`**: Provides a deterministic, fake LLM for unit tests, ensuring `brain` logic (like event lifecycle or memory tests) can run in CI pipelines without incurring API costs.

### Internal Utilities:
- **`generic_http.rs`**: Contains the boilerplates and shared mapping routines for "OpenAI-compatible" third party APIs, avoiding massive code duplication.
- **`utils.rs`**: Helper functions to convert between AIMAXXING `brain::agent::message` components (Roles, Tool Calls, Tool Results) to the respective formats needed by different APIs.

## 🚀 Purpose

The primary goal of the `providers` module is **Extensibility and Standardization**. 

No matter which specific model or API a user decides to configure for an Agent (e.g., swapping from GPT-4 to Claude 3.5 Sonnet to Local Llama 3), the `brain` module only interacts with a uniform internal interface. The `providers` module sits in between and performs the necessary data-wrangling, formatting, and networking needed to translate deep framework logic into standard vendor API payloads.
