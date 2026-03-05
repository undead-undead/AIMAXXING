# AIMAXXING Gateway Module Documentation

The `gateway` module serves as the central HTTP routing, orchestration, and security entry point for the AIMAXXING AI Agent framework. It exposes the underlying `brain` (core logic), `engram` (memory), and `providers` (LLM communication) as a standardized REST and WebSocket API. 

## 🛠 Technology Stack & Dependencies

The `gateway` is built as a high-performance web service using standard cloud-native Rust technologies:
- **Web Framework**: `axum` for HTTP routing, WebSockets, and middleware.
- **Async Runtime**: `tokio` for handling thousands of concurrent connections.
- **Middleware & Networking**: `tower` and `tower-http` for CORS, tracing (logging), request timeouts, and rate limiting.
- **State Management**: `dashmap` and `parking_lot` for shared, lock-free concurrent application state handling across threads.
- **Caching**: `moka` for high-performance, concurrent, asynchronous in-memory caching.
- **CLI & Interactivity**: `clap` for command-line argument parsing, `dialoguer` and `indicatif` for interactive terminal prompts when running in console mode.
- **Configuration**: `dotenv` for `.env` file loading, `serde_yaml_ng` for parsing `aimaxxing.yaml`.

## 📂 Architecture & Modules

The `gateway/src/` directory is logically separated into API definitions, security protocols, and integration bridges:

### 1. `api/` (Core HTTP Endpoints)
This directory defines the Axum routers and handlers exposed to clients (e.g., the Panel UI, external scripts).
- **`server.rs`**: The main entry point that bootstraps the `axum::Router`, binds to the network port, and handles configuration hot-reloading.
- **`factory.rs`**: Endpoints for dynamically spawning, configuring, and testing new Agents (Souls/Personas) on the fly without restarting the server.
- **`bridge.rs`**: The chat interface. Exposes `POST /v1/chat/completions` (OpenAI-compatible) and WebSocket upgrade endpoints for real-time agent conversations and streaming.
- **`tool.rs`**: Endpoints for registering, unregistering, and listing dynamic Tools/Skills that an Agent can use.
- **`knowledge.rs`**: Endpoints to ingest data into the `engram` vector database and manage the active knowledge indexes.
- **`security.rs`**: Handles authentication (API keys) and authorization for API endpoints.

### 2. `blueprints/` (Agent Templates)
Contains pre-defined agent architectures. Blueprints are YAML or JSON configurations that define an agent's starting system prompt, preferred LLM, temperature, and specific skill sets (e.g., a "Coder" blueprint vs. a "Trader" blueprint).

### 3. Core Coordinators
- **`main.rs`**: Starts the `GatewayServer`, initializes the global tracing subscriber, parse CLI arguments, and loads the environment.
- **`mcp.rs`**: Implements the Model Context Protocol (MCP) server side. Allows the Gateway itself to act as an MCP provider to IDEs or other external agents, exposing internal tools.
- **`doctor.rs`**: A self-diagnostic tool built into the gateway. On startup or via CLI command, it checks system health (e.g., "Is redb accessible?", "Are the sandbox paths valid?", "Are LLM API keys configured?").
- **`onboard.rs`**: Handles the first-time setup logic when the user runs the Gateway on a fresh machine (creating default data directories, generating encryption keys).

## 🚀 Purpose

While the `brain` executes thought loops and `engram` remembers facts, the `gateway` is the physical face of the application. 

Its key responsibilities include:
1. **Network Interface**: Providing a stable API that frontends (like `aimaxxing-panel`) or generic OpenAI clients (like Cursor or Continue.dev) can connect to.
2. **Fleet Management**: Running multiple independent agents simultaneously in the same process, managing their lifecycles, and routing messages to the correct instance.
3. **Security Boundary**: Enforcing API key authentication before anyone can prompt an Agent or access the knowledge base.
4. **Proxying**: Sitting between the client and the `providers`, ensuring prompt injection checks and rate limits are enforced before paying for LLM tokens.
