# AIMAXXING Auth & Secret Module

The `auth` module provides **Single-User Secret Management (Vault)** and **Agent-to-Service Connectivity (OAuth2)**. 

Unlike traditional multi-tenant SaaS applications, AIMAXXING is a **local-first desktop tool**. It prioritizes a **"Zero-Login"** experience where the user who launches the application has full ownership of the local gateway and its resources.

---

## 🛠 Strategic Principles
1.  **Zero Friction**: No mandatory login, password, or account creation to use local AI features.
2.  **Vault-Centric**: Focus on secure, encrypted storage of API Keys (OpenAI, Anthropic, etc.) in a local database.
3.  **Optional Multi-Session**: Identity is tied to the **Agent's Persona (Soul)**, not a global "User Account".
4.  **Local Trust**: Communication between the GUI (Panel) and Backend (Gateway) is open by default on `localhost`, with optional static-key protection for advanced setups.

---

## 📂 Architecture & Modules

### 1. The Vault (Secret Storage)
- **Purpose**: Persistent, encrypted storage for sensitive strings (API Keys, OAuth tokens).
- **Security**: AES-256-GCM encryption with a master key stored in the OS **Keyring/Keychain**.
- **Implementation**: Uses `redb` as the high-performance embedded database engine.
- **Workflow**: Skills and Connectors retrieve their secrets from the Vault at runtime.

### 2. OAuth2 Manager (Service Connectivity)
- **Purpose**: Allows AI Agents to interact with third-party services (Google, GitHub, Slack) on behalf of the user.
- **Why it's NOT a login system**: This is **outbound** authentication. It manages tokens that the Agent uses to call external APIs, not tokens used to log into AIMAXXING itself.
- **Storage**: Tokens are stored in the local Vault with automatic background refresh.

### 3. API Security & Access Control
- **Local Access**: By default, the Gateway listens on `0.0.0.0` but applies an **ApiGuard** middleware. Loopback connections (`127.0.0.1`) are automatically trusted for a frictionless experience.
- **Non-Local Access**: Requests coming from different machines (LAN/Public) must provide a **Static Secret** in the `X-API-Key` header.
- **Internal Key**: A random 32-character hex key is generated on the first run and stored securely in the Vault as `GATEWAY_INTERNAL_KEY`.
- **NO JWT Overhead**: System skips heavy OAuth2 session management for core API calls to minimize latency.

---

## 🚀 Performance & UX Impact
- **Instant Start**: The app boots directly into the UI. No "Login Screen" roadblock.
- **Privacy First**: No identity data is ever sent to a central server. All credentials live and stay in the local `/data/vault`.
- **Developer Simplicity**: Writing a new skill is easier—just call `vault.get("OPENAI_KEY")` instead of navigating complex multi-user permission trees.
