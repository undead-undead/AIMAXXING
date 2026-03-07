# AIMAXXING Infrastructure Module Documentation

The `infra` crate is a standalone, multifunctional infrastructure layer for the AIMAXXING framework. It provides essential services that support the core Agent logic, including communication, monitoring, and administrative tasks.

## 🛠 Technology Stack & Dependencies

- **WebSocket/HTTP Server**: `axum` and `tower-http` for the lightweight Gateway.
- **Async Runtime**: `tokio` for handling concurrent background services.
- **Monitoring & Metrics**: `dashmap` and `parking_lot` for the high-performance `MetricsRegistry`.
- **External Notifications**: `reqwest` for pushing to Telegram, Discord, and Email APIs.
- **Serialization**: `serde_json` and `serde` for JSON-RPC and protocol message exchange.
- **System Metrics**: `sysinfo` for cross-platform hardware monitoring.

## 📂 Architecture & Modules

### 1. Connectivity & Gateway
- **`gateway/`**: The lightweight control plane. Includes WebSocket handlers (`handlers.rs`), the OpenAI-compatible bridge (`openai.rs`), and real-time state management (`state.rs`).
- **`telegram.rs`**: Specialized one-way Telegram bot integration for agent event broadcasting.
- **`notifications.rs`**: Unified multi-channel notification sender (Discord, Email, Webhooks).

### 2. Observability & Logging
- **`observable.rs`**: Implements the `AgentObserver` trait and the `MetricsRegistry` for tracking token usage, latency, and success rates.
- **`logging.rs`**: Provides advanced tracing subscribers and log rotation for the AIMAXXING dashboard.

### 3. LLM Operations (LLMOps)
- **`prefix_cache.rs`**: Optimizes prompting by managing common text fragments and preventing redundant calculation across expensive LLM requests.
- **`pricing.rs`**: Dynamically calculates the cost of agent operations based on provider-specific token pricing.

### 4. Background Services
- **`maintenance.rs`**: Handles health checks, diagnostic reports, and automated cleanup of temporary runtime environments.

## 🚀 Purpose

The `infra` module's purpose is to **Bridge the Core and the User**. By isolating the "bare-metal" complexities of external APIs, network servers, and monitoring from the `brain`, it ensures that AIMAXXING remains performant and observable at scale.
