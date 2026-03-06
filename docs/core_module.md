# AIMAXXING Core Module Documentation

The `brain` module serves as the central nervous system of the AIMAXXING AI Agent framework. Rather than just being an orchestration layer, it is engineered as a **highly-secure, isolated execution engine**. It provides the core abstractions, sandboxing environments, security boundaries, and basic logic necessary to run untrusted AI-generated code safely.

This directory (`/home/biubiuboy/aimaxxing/core`) contains the fundamental building blocks of the framework, completely decoupled from the HTTP server mapping or the user interface layer.

---

## 🛡️ The AIMAXXING Security Framework

Because AIMAXXING empowers Large Language Models to write and execute code on the host machine, security is the absolute highest priority. The `brain` module implements a robust, multi-layered defense-in-depth security model.

### 1. Tri-Platform Native Sandboxing (三端安全性)
When an agent attempts to execute a native script or shell command (e.g., Python, Bash), the `brain` does not blindly spawn a generic subprocess. Instead, it utilizes OS-level kernel isolation features native to each operating system to lock down the execution space (implemented in `skills/sandbox.rs`):
- **Linux (`bwrap`)**: Utilizes Bubblewrap to create an unprivileged namespace sandbox. The root file system is mounted as Read-Only via `--ro-bind`, and the process is strictly isolated from the host network via `--unshare-net` (unless explicitly whitelisted by the Skill configuration).
- **macOS (Seatbelt / `sandbox-exec`)**: Dynamically generates and applies a strict Seatbelt Safari-style `.sb` profile file. It explicitly denies network access `(deny network*)` and blocks write access globally `(deny file-write*)` except for the designated temporary scratchpad workspace for that specific skill.
- **Windows (`JobObject`)**: Constrains the process securely using Windows Job Objects. It enforces strict UI restrictions (e.g., blocking clipboard hijacking via `JOB_OBJECT_UILIMIT_READCLIPBOARD`) and prevents the process from jumping out of the sandbox to spawn orphaned daemon zombies.

### 2. Application-Layer Firewalls & Leak Detection
Before a command even reaches the OS-level sandbox, it must pass through AIMAXXING's internal guards:
- **Pre-flight Shell Firewall**: Analyzes the raw command intent. If the LLM generates a catastrophic or obviously malicious command (like `rm -rf /`, `mkfs`, or unauthorized root escalations), the execution is preemptively aborted and an error is fed back to the LLM.
- **Secret-in-Args Guard**: Ensures that API keys, DB passwords, and tokens are never accidentally leaked via command line arguments (`ps aux` visibility). It forces the LLM to use the encrypted Vault for environmental variable injections.
- **Output Sanitization (Leak Detector)**: The standard output and standard error streams are actively scanned and cleaned. The globally instantiated `LeakDetector` contains an arsenal of hardcoded **Regular Expressions** matching credentials for AWS, Stripe, Anthropic, OpenAI, GitHub, etc. Based on the pattern matched, the detector triggers a strict `LeakAction`:
  - **`Redact`**: Instantly swaps the key for `***` without disrupting the workflow.
  - **`Warn`**: Logs the potential leak (like a generic JWT or basic `Authorization` header) but allows execution.
  - **`Block`**: Preemptively aborts and kills the sandbox entirely (used when a raw PEM Private Key is echoed).

---

## 🏗️ Seamless API & Schema Generation

A major pain point in building agentic frameworks is maintaining synchronization between the Backend tools and the Frontend prompt schemas. `core` completely automates this logic:
- **Write Once in Rust**: Developers implement the `Tool` trait alongside a strongly-typed Rust `struct` that represents the tool's input arguments. By deriving `JsonSchema` (using the `schemars` crate), the Rust compiler evaluates the struct at build-time.
- **Frontend / LLM Automatic Generation**: The `generate_schema()` backend utility automatically parses the `schemars` type information and converts it directly into a standard OpenAPI JSON schema (or a raw TypeScript interface string). 
- **Result**: The "Frontend" (which implies both the LLMs parsing system prompts, and the `aimaxxing-panel` visualizing tool settings automatically) receives these definitions dynamically. If an engineer adds a new `pub max_retries: u32` field to a skill in the backend, the GPT-4 agent and the UI know about it instantly without any manual duplicative JSON writing.

---

## ⚡ Execution Runtimes & Provisioning

Agents need diverse environments to execute their dynamic skills (e.g., running python data science scripts or JavaScript API integrations). The `core` module provides zero-configuration, lightning-fast provisioning for these environments.

### 1. In-Process JavaScript (QuickJS)
For lightweight scripts and data manipulation, AIMAXXING uses the `rquickjs` crate to embed the **QuickJS** engine natively within the Rust process. 
- **Why?** It completely bypasses the overhead of spawning V8 or Node.js instances. QuickJS allows the agent to execute JavaScript and TypeScript data conversions instantaneously in an ultra-secure, memory-limited Rust sandbox.

### 2. High-Speed Python Provisioning (`uv` and `pixi`)
When a skill strictly requires Python (e.g., PyTorch, Numpy), the framework avoids relying on the unpredictable state of the user's host OS Python installation.
- **`uv` Integration**: Utilizes Astral's incredibly fast `uv` package manager (written in Rust) to silently fetch and install exact CPython versions and resolve `pip` dependencies in milliseconds. 
- **`pixi` / Isolated Environments**: For every unique skill, the `brain` provisions a highly isolated Virtual Environment (`venv` or `conda` structure). If an agent decides it needs `requests` and `pandas` for a specific task, `uv` instantly drops these into a sandboxed folder (`~/.aimaxxing/venvs/<skill_name>`).
- **Result**: The host system is never polluted. 100 different agents can run 100 different, conflicting Python package versions simultaneously without crashes.

---

## 📂 Complete Internal Architecture (`core/src/`)

The core framework is fully decoupled into 16 distinct sub-modules, each handling a specific domain of the AI lifecycle:

- **`agent/`**: Brain & Identity. Contains `Agent`, `AgentBuilder`, Persona loading, and the short-term sliding conversational memory cache.
- **`approval/`**: Human-In-The-Loop (HITL) system. Allows agents to pause execution and formally request human permission.
- **`auth/`**: Core authentication traits (Implementations in standalone `auth` crate).
- **`bus/`**: Async Event Pub/Sub system. A decoupled message router allowing agents to emit events.
- **`config/`**: Configuration parsers mapping the `aimaxxing.yaml`.
- **`connectors/`**: Data ingestion conduits (Logic moved to standalone `connectors` crate).
- **`env/`**: Zero-config environment isolation (Integrated with `runtimes`).
- **`hooks/`**: Global lifecycle interceptors.
- **`infra/`**: Core infrastructure traits (Implementations in standalone `infra` crate: Gateway, Telegram, Notifications).
- **`knowledge/`**: High-level bridging logic (Traits for `VectorStore` and `Embeddings`).
- **`mcp/`**: Core MCP traits (Implementation in standalone `mcp` crate).
- **`runtime/`**: Language bridges (Traits for QuickJS, Python; implementations in `runtimes` crate).
- **`security/`**: Defensive boundary traits (Implementation in standalone `security` crate).
- **`session/`**: Multi-tenancy structures.
- **`skills/`**: Tool abstractions & Registry (Built-in tools moved to `builtin-tools` crate).
- **`store/`**: Persistent state management linking to `redb` and `sqlite`.

By segregating this deep sandboxing and provisioning logic into `core` (`brain`), AIMAXXING ensures that its foundational AI behavior remains portable, secure by default, and rigorously testable.
