# AIMAXXING MCP Module Documentation

The `mcp` crate implements the **Model Context Protocol (MCP)**, a standard for connecting AI models to external tools and data. It allows AIMAXXING to both consume corporate/open-source tools and expose its own capabilities to other agents.

## 🛠 Technology Stack & Dependencies

- **Protocol Specification**: Follows the Anthropic MCP JSON-RPC standard.
- **Async Runtime**: `tokio` for handling multiple concurrent MCP sessions.
- **Serialization**: `serde_json` for protocol messages and capability negotiation.
- **Transport**: Supports both `stdio` (for local MCP servers) and `HTTP/SSE` (for remote servers).

## 📂 Architecture & Modules

### 1. Client Implementation
- **`client/`**: Allows AIMAXXING agents to connect to external MCP servers (e.g., Google Search, Slack, GitHub).
- **`discovery.rs`**: Handles the discovery and listing of capabilities offered by an MCP server.

### 2. Server Implementation
- **`server/`**: Allows AIMAXXING to act as an MCP server. It exposes the AIMAXXING tools (from `builtin-tools`) so they can be used by external IDES (like Claude Desktop or Cursor).
- **`resource_manager.rs`**: Manages the lifecycle of MCP resources (files, tables, and blobs) exposed to external clients.

### 3. Shared Logic
- **`protocol.rs`**: Common data structures and message types for MCP version 0.1.0+.
- **`session.rs`**: Manages the state and heartbeat of active MCP connections.

## 🚀 Purpose

The `mcp` module's purpose is to **Enable Interoperability**. By adopting the MCP standard, AIMAXXING eliminates the need for proprietary tool-calling formats, allowing users to plug in any MCP-compatible service and instantly expand their agent's capabilities.
