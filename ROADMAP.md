# AIMAXXING Roadmap (v2.0)

This document outlines the strategic evolution of AIMAXXING, focusing on the **Zero-Admin Windows Experience**, **Hybrid Runtimes**, and **Granular Security**.

---

## 🔴 Phase 1: CRITICAL - Windows Native & Security Hardening
*Objective: Achieve industry-leading "Zero-Admin" and "Zero-Login" performance and security on Windows without requiring WSL, Docker, or elevated privileges.*

### 1.0 Zero-Login Strategy (UX First)
- [x] **Agnostic Identity**: Design the application to be "Single-User by Default" where the local OS session provides the identity. No "Login Screen" or "JWT Sessions" for core functionality.
- [x] **Static Secret Exchange**: Implement a simple, one-time static API key exchange between Panel (GUI) and Gateway (Backend) only for non-localhost connections to prevent cross-app unauthorized commands.

### 1.1 Hybrid & Portable Zero-Admin Runtimes (uv + pixi + Mini Bash)
- [x] **Pixi Responsibility Contraction**: Refactor `EnvManager` to use Pixi *only* for Python interpreter isolation. Remove git/bash/gcc from Conda installs.
- [x] **UV Integration**: Bundle `uv.exe` in `infra/bin`. Implement `uv pip install` inside Pixi environments for 10x faster package management and improved build success.
- [x] **15MB Mini Git Bash**: Package a curated, UPX-compressed subset of Git Bash (bash, grep, awk, sed, coreutils) to provide 99% script compatibility at < 30MB total footprint.
- [x] **Portable Toolchain Integration**:
    - [x] Bundle `bun.exe` (~20MB) and a minimal `git.exe` (~10MB) in `infra/bin`.
    - [x] Bundle a minimal `gcc` (MinGW-w64 subset, ~50MB) for essential C-extensions.
    - [x] Ensure `EnvManager` injects `infra/bin` at the front of `PATH`.
- [x] **PowerShell First Strategy**:
    - [x] Implement "Syntax Detection": Simple tasks (ls, rm, cp) map to native PowerShell aliases.
    - [x] Complex tasks (pipes, redirects, `.sh` scripts) fallback to Mini Git Bash.
    - [x] Replace hardcoded `/bin/bash` with Windows-aware branch in terminal sockets.

### 1.2 Multi-Platform Security Firewall (Phase 1.1 Parity)
- [x] **Windows Shell Firewall**: Comprehensive regex for `del`, `rd`, `runas`, `powershell -enc`, `certutil`.
- [x] **Path Canonicalization**: Unified `\` to `/` normalization for firewall matching.
- [x] **Security Verification**: Perform aggressive coverage testing to ensure sandbox escape attempts are blocked.
- [x] **macOS TCC Integration**: Pre-flight checks for Full Disk Access/Input Monitoring.
- [x] **macOS Seatbelt**: Hardened profiles denying access to Keychains, Safari data, and private docs.

---

## 🟡 Phase 2: HIGH - Architecture & Knowledge (Engram V2)
*Objective: Scale the engine to handle massive swarm missions with extreme memory efficiency.*

### 2.1 "Fat Core" Slimming & Modularity
- [x] **Crate Decoupling**: Extract `connectors`, `security`, `runtimes`, `skills`, `knowledge`, `mcp`, `auth` into standalone crates.
- [x] **Filesystem Hygiene**: Move all artifacts (`.log`, `.json`, `.pid`) to the isolated `/data` directory.
- [ ] **Cross-Crate Interface Verification**: Ensure the `Bus/Traits` abstraction holds under the new runtime isolation.

### 2.2 Engram V2: Performance Knowledge
- [x] **Storage Traits**: Generic `KVStore` and zero-copy `Bytes` fetching.
- [x] **Quantization Tiering**: FP32 (Soul) -> U8 (Warm) -> INT4 (Cold/Ternary).
- [x] **Hybrid Search**: BM25 + Vector Search with RRF (Reciprocal Rank Fusion).
- [x] **Local Reranker**: Candle-based BGE-Reranker integration.
- [x] **Model Pooling**: LRU-based swapping for RAG and Media models.
- [x] **Local OCR**: Statically embedded WASM-based Tesseract for zero-setup offline OCR.

### 2.3 Connectivity Context
- [x] **Enterprise Connectors**: Feishu, Slack (Socket Mode), E-mail (SMTP/IMAP).
- [x] **Unified Notification Center**: Broadcast alerts across all active channels.

---

## 🟢 Phase 3: MEDIUM - Multimedia & Autonomous Ops
*Objective: Transform from a tool into a professional, proactive AI environment.*

### 3.1 Local Media Runtime (Phase 6)
- [x] **Model Management UI**: "Download/Load" buttons for local media runtimes.
- [x] **Local Whisper (STT)**: Candle-based transcription with multi-language selector.
- [x] **Local Piper (TTS)**: High-speed neural text-to-speech with curated local voices.
- [ ] **Interactive "Light-Up" Logic**: Dynamic injection of Microphone button in UI based on model readiness.

### 3.2 Autonomous Operations & Governance
- [ ] **Skill Governance**: Per-agent skill allowlisting in `SOUL.md` to prune LLM context and prevent unauthorized tool calls.
- [ ] **Unified Scheduler UI**: Manage Cron jobs, Event triggers (file watchers, webhooks), and execution history.
- [ ] **Memory Tiering**: Automated "Distillation" tasks to move facts from logs to long-term permanent memory.
- [ ] **A2A Swarm Protocol**: Zero-latency async bus for local multi-agent communication using official `AgentCard` formats.

---

## 🚀 Milestones & Delivery

| Milestone | Core Target | 
| :--- | :--- | 
| **V1.0 (Basic)** | Zero-Admin Launch + Python Isolation + Basic Shell. |
| **V1.1 (Full)** | Portable Toolchain + 15MB Mini Bash + Hardened Firewall. | 
| **V1.2 (Enhanced)** | Optimized RAG + Enterprise Connectors + Professional UI Overhaul. |

---

## ⚠️ Key Risks & Mitigation

1.  **Zero-Admin Escape**: Subprocesses might attempt to bypass Job Object limits.
    *   *Mitigation*: Strict ShellFirewall rules combined with user-space identity enforcement.
2.  **Toolchain Bloat**: Portable binaries (Bun/Git/GCC/Bash) could exceed 100MB.
    *   *Mitigation*: Implement "Lite" (Framework only) vs "Full" (Auto-downloader) installation tiers.
3.  **Cross-Platform Parity**: Hardcoded Linux commands (e.g., `df -m`) in legacy skills.
    *   *Mitigation*: Mandate all core system calls use cross-platform Rust crates (`sysinfo`, `sha2`).

---

# 原版路线图：
# AIMAXXING Roadmap

This document outlines the planned future developments for the AIMAXXING project.

## Phase 1: Enhanced Multi-Platform Security (CRITICAL)

### 1.1 Windows Shell Firewall Parity
- **Goal**: Bring the `ShellFirewall` rules in `core/src/security/shell_firewall.rs` to parity with Linux by adding Windows-specific command patterns.
- **Tasks**:
    - [x] Add regex for Windows file deletion: `del`, `rd /s`, `erase`.
    - [x] Add regex for Windows privilege escalation: `runas`.
    - [x] Add regex for system disruption commands: `format`, `vssadmin delete shadows`.
    - [x] Add regex for obfuscated Windows execution: `powershell -enc`, `powershell -EncodedCommand`, `certutil -urlcache`.
    - [x] Implement path canonicalization for Windows (`\` vs `/`).

### 1.2 Resource Quota Hardening
- [x] Expand Windows Job Object limits to include network I/O throttling per process.
- [x] Implement disk I/O limits in Linux `bwrap` using `cgroups v2`.

### 1.3 macOS Security Strategy
- **Goal**: Harden the macOS execution environment beyond basic `Seatbelt` profiles.
- **Tasks**:
    - [x] **TCC Integration**: Implement pre-flight checks for macOS "Full Disk Access" and "Input Monitoring" permissions to avoid silent failures.
    - [x] **Seatbelt (sandbox-exec) Hardening**: Refine the Scheme profile to explicitly deny access to `~/Library/Keychains`, `~/Library/Safari`, and `~/Documents` unless explicitly whitelisted.
    - [x] **Firewall Expansion**: Add regex for macOS-specific data exfiltration tools: `pbpaste`, `screencapture`, `mdfind` (potential sensitive file searching).
    - [ ] **Code Signing**: Integrate automatic self-signing for generated WASM/Native tools to satisfy macOS Gatekeeper requirements in local environments.

---

## Phase 2: Communication & Connectivity

### 2.1 Connector Ecosystem Expansion
- [x] Implement **Feishu (ByteDance)** connector for enterprise workspace automation (Bi-directional via Webhook Bridge).
- [x] Implement **Slack** Webhook & **Socket Mode** support.
- [x] Add support for **E-mail (SMTP/IMAP)** as a persistent communication channel.

### 2.2 Unified Notification Center
- [x] Create a cross-connector notification abstraction to allow agents to "broadcast" important alerts to all active channels.

---

---

## Phase 3: Unified Knowledge Engine & Performance (Engram V2)
*Constraint*: Consolidate local RAG accuracy with extreme memory efficiency and hardware acceleration.

### 3.1 Storage Abstraction (Memory Decoupling)
- [x] Define generic `Storage` and `KVStore` traits.
- [x] Implement `Bytes`-based zero-copy fetching.
- [x] Decouple `Engram` from hardcoded `redb` bindings.

### 3.2 Quantization Precision (Dynamic Memory Management)
- [x] Implement INT4 and Q1.58 (Ternary) quantization.
- [x] Add dynamic aging logic: FP32 (Soul) -> U8 (Warm) -> INT4 (Cold).

### 3.3 RAG Pipeline Optimization
- [x] Implement Hybrid Search (BM25 + Vector) with RRF fusion.
- [x] Optimize CJK segmentation for memory-efficient BM25 indexing.

### 3.4 Hardware Acceleration (Phase 3-D)
- [x] Enable AVX-512/Neon SIMD via `simsimd`.
- [x] (Optional) Integrate GPU acceleration for heavy RAG workloads.
- **Tasks**:
    - [x] **Local Reranker**: Integrate lightweight Cross-Encoder models via `candle` (e.g., BGE-Reranker-v2-Minica).
    - [x] **Model Pooling**: Allow dynamic local model selection to save compute and API costs (LRU-based swapping).
    - [x] **Pipeline Update**: Coarse Search (BM25+HNSW) -> Precision Rerank -> Top-K.

- **Upcoming/Candidate Features**:
    - [x] **Local OCR (WASM)**: Integrate WASM-based Tesseract (Statically embedded for zero-setup offline OCR).
    - [x] **SIMD Optimization**: Implement AVX2/NEON accelerated distance metrics for vector search.

---

## Phase 4: Architectural Modularity & Decoupling

### 4.1 "Fat Core" Slimming
- **Goal**: Transition `brain` crate from monolithic implementation to a pure abstraction layer while **preserving full-stack connectivity** (Bus/Traits).
- **Tasks**:
    - [x] Extract `connectors/` into a standalone crate.
    - [x] Extract `security/` (Firewall + Sandbox) into a standalone crate.
    - [x] Extract `runtimes/` (Executors) into a standalone crate.
    - [x] Extract `skills/` (Engine + Built-in Tools) into a standalone crate.
    - [x] Extract `knowledge/`, `mcp/`, `auth/`, `infra/` into their respective crates.

### 4.2 Filesystem Hygiene
- [x] Direct all runtime artifacts (`.log`, `.pid`, `.json` tokens) to a unified `/data` or `/var` directory.
- [x] Remove all persistent state files from the project root.
We welcome contributions to any of these areas! Please open an issue or PR on the respective module.

---

## Phase 5: Native Windows Experience (CRITICAL TRANSITION)

*Constraint*: The entire AIMAXXING engine, including gateways, panels, and all spawned agent sandboxes, **MUST execute in user-space without Windows Administrator (`UAC`) privileges**.

### 5.1 Hybrid & Portable Zero-Admin Runtimes (uv + pixi + Portable Bash)
- **Goal**: Minimize cold-start latency and disk footprint by splitting environment management between Pixi (Isolation), UV (Speed), and Local Portable Binaries.
- **Tasks**:
    - [x] **Pixi Refactoring**: Restrict Pixi's role to *only* managing Python interpreter isolation (Conda environments). Eliminate the overhead of installing general tools like git/bash via Conda.
    - [x] **UV Integration**: Use `uv` for lightning-fast `pip` package installation inside Pixi-managed environments.
    - [x] **15MB Mini Git Bash**: Bundle a stripped-down, portable Git Bash (~15MB compressed) with core utilities (`bash`, `grep`, `awk`, `sed`) to provide 99% Bash script compatibility on Windows without the ~200MB MSYS2 footprint.
    - [x] **Portable Toolchain**: Include portable binaries for `bun`, `git`, and basic `gcc` in the `infra/bin` directory, prioritized in the `PATH` by `EnvManager` in `core/src/env/mod.rs`.
    - [x] **PowerShell First**: Promote PowerShell as the primary Windows shell when `runtime: shell` is used, utilizing automatic command mapping/aliases for common Bash idioms while falling back to Mini Git Bash for complex `.sh` scripts.

### 5.2 Cross-Platform Command Equivalents
- **Goal**: Refactor hardcoded Linux commands in built-in tools to use cross-platform Rust equivalents or OS-aware branches.
- **Tasks**:
    - [x] `core/src/env/mod.rs`: Replace `df -m` with pure-Rust `sysinfo` crate for disk space checking.
    - [x] `core/src/env/mod.rs`: Replace `sha256sum` / `shasum` with pure-Rust `sha2` crate for model checksums.
    - [x] `builtin-tools/src/tool/notifier.rs`: Ensure Windows 11 Toast Notifications are supported (e.g., using `winrt-notification` or `notify-rust` with Windows features enabled).
    - [x] `core/src/hooks/engine.rs`: Refactor `ShellHook` which hardcodes `Command::new("sh").arg("-c")` to use PowerShell or `cmd.exe` on Windows.
    - [x] `gateway/src/api/server.rs`: Update `handle_terminal_socket` which currently hardcodes `/bin/bash` to launch `powershell.exe` on Windows.

### 5.3 Developer Experience & Tooling
- **Goal**: Provide an accessible entry point for Windows developers.
- **Tasks**:
    - [x] Create `install.ps1` for native Windows environment setup (matching `install.sh`).
    - [x] Create `build_all.ps1` for compilation.
    - [x] Create professional `Setup.exe` installer with Lite and Recommended (Tools + Bash) tiers.
    - [x] Rebrand entire experience from `ClawHub` to `Smithery`.

---

---
---

## Phase 6: Professional UI & Multimedia (Local Media)

### 6.1 Local Media Runtime (Self-Hosted Voice)
- **Goal**: Provide a completely offline, zero-cost voice system (STT/TTS) leveraging local hardware, independent of cloud API subscriptions.
- **Tasks**:
    - [x] **Optional Media Downloader**: Add "Download Media Components" buttons in the Model Management UI (sharing the same unified downloader logic as Llama models).
    - [x] **Local Whisper (STT)**: Implement a local speech-to-text runner (via `whisper.cpp` or `sherpa-onnx`) for instant transcription.
        - [x] **STT Language Model Selector**: In the Panel, provide a selector to choose which language model to download/activate (Chinese, English, Japanese, Korean, etc.) for transcription.
        - [x] **Space-Saving "Swap" Logic**: Only the selected transcription model is kept active in memory. Switching the target language triggers the Phase 3.5 Model Pool to swap models to save system resources.
    - [x] **Local Piper (TTS)**: Implement a high-speed, local neural text-to-speech engine using `Piper` with curated voice models.
    - [ ] **Interactive "Light-Up" Logic**: Automatically detect downloaded media runtimes and dynamically inject the 🎙️ (Microphone) button into the Agent chat interface only when ready.

### 6.2 Professional Navigation & Session Overhaul
- **Goal**: Elevate the existing top-tab architecture into a premium, high-performance navigation system for better task focus and multi-agent management.
- **Tasks**:
    - [ ] **Tab Bar Modernization**: Refine the existing top-tab bar with "Glassmorphism" effects, smooth transitions, and better active-state visualization.
    - [ ] **Unified Header Concept**: Integrate the logo, connectivity status, and main navigation into a single, cohesive "Control Center" header.
    - [ ] **Chat-Internal Session Tabs**: Implement a sophisticated secondary tab layer *inside* the Chat tab to manage multiple independent contexts and Swarm missions.
    - [ ] **In-Chat Execution Tracing**: Integrate real-time "thought" and "tool" tracing directly into the message stream. Instead of a heavy, separate "Workflow" tab, users can see the agent's internal reasoning and tool execution steps as expandable "trace" blocks within the chat.
    - [ ] **A2A Lite (Swarm Protocol)**: Implement a high-performance, memory-backed Agent-to-Agent communication bus.
        - [ ] Use official A2A `AgentCard` and `Task` data structures for industry compatibility.
        - [ ] Bypass HTTP/RPC overhead for local agents, using direct async channels for zero-latency task delegation.
    - [ ] **Swarm Memory Isolation**: Implement "Context Guard" to prevent multi-agent crosstalk from polluting the long-term `SOUL.md` of individual agents during collaborative tasks.

---

## Phase 7: Granular Agent Governance & Capability Isolation

- **Goal**: Implement "Least Privilege" security and reduce context clutter by allowing per-agent skill/tool configuration, moving away from the "global-by-default" skill model.
- **Tasks**:
    - [ ] **Skill Allowlisting**: Implement a `skills` field in the agent manifest/SOUL.md to explicitly white-list which skills are available to a specific agent.
    - [ ] **Context Pruning**: Dynamically filter the tool definitions sent to the LLM based on the agent's specific allowlist to minimize prompt noise and token consumption.
    - [ ] **Per-Agent Capability UI**: Update the Panel to allow users to toggle specific skills/tools on or off for each agent individually.
    - [ ] **Runtime Enforcement**: Ensure the tool runner strictly enforces the allowlist at execution time, preventing agents from calling unassigned tools even if they "know" the tool exists.

---

## Phase 8: Autonomous Operations & Advanced Scheduling

- **Goal**: Elevate the existing simple cron functionality into a professional, multi-tab autonomous execution center.
- **Tasks**:
    - [ ] **Unified Scheduler UI**: Implement a premium tabbed interface in the Panel:
        - **Scheduled Jobs**: Manage recurring tasks with cron expressions (e.g., daily code audits, hourly news sweeps).
        - **Event Triggers**: Reactive execution based on events (file modified, new Git PR, Webhook received).
        - **Run History**: A dedicated audit log to view the status, execution time, and output of background tasks.
    - [ ] **Job Persistence**: Ensure all scheduled and reactive jobs are stored in the persistent database and automatically resume after system restarts.
    - [ ] **Visual Job Builder**: Add a guided interface for creating jobs that allows users to select the target agent, input message, and schedule/trigger without raw JSON/Cron knowledge.
    - [ ] **Background Execution Monitoring**: Real-time visualization of running background agents and their current resource consumption.

---

## Phase 9: Unified Persistence Engine (redb Transition)

- **Goal**: Sever all remaining reliance on `serde_json` file-overwrites for runtime state and eliminate the performance bottlenecks of the current "fat-JSON" model.
- **Tasks**:
    - [ ] **redb Implementation for STM**: Refactor `ShortTermMemory` to use `redb` (via `EngramKV`) for storing conversation history. This enables transactional, concurrent writes and memory-mapped, random-access reads.
    - [ ] **Session State Externalization**: Move active agent session states (内心独白、中间步骤) from RAM-only DashMaps to `redb`, ensuring full recovery after unexpected gateway restarts.
    - [ ] **Pagination/Cursor API**: Implement an `OFFSET/LIMIT` retrieval strategy in the gateway to fetch large conversation histories in chunks instead of bulk-loading entire session files.

---

## Phase 10: Advanced Memory Tiering & Consolidation

- **Goal**: Mathematically separate "Conversation Logs" (Ephemeral) from "Agent Memory" (Persistent) to maximize token efficiency and prevent long-term context clutter.
- **Tasks**:
    - [ ] **Log/Memory Decoupling**: Establish a strict boundary where `ShortTermMemory` is treated as a transient audit log, while `MEMORY.md` and the Vector DB store "distilled" knowledge.
    - [ ] **Memory Distillation Cron**: Implement an automated background task that periodicially scans inactive sessions, extracts key facts (user preferences, task outcomes), and commits them to the agent's long-term memory.
    - [ ] **Log Pruning Policy**: Implement an automatic pruning mechanism for conversation logs (e.g., delete logs older than 30 days) *without* affecting the distilled "Memory" of the agent.
    - [ ] **Memory CRUD Protocol**: Standardize the interface for agents to manage their own knowledge based on four fundamental operations:
        - **Create**: Add new facts or user preferences discovered during conversation.
        - **Read**: Explicitly list or query the current permanent memory.
        - **Update**: Correct or overwrite outdated/incorrect stored facts.
---

## Phase 12: Secure Collaborative Ecosystem (Deep Sync)

- **Goal**: Enable frictionless, secure multi-user collaboration and cross-device syncing without manual key management.
- **Tasks**:
    - [ ] **One-Click Invitation Links**: Implement deep-link support (`aimaxxing://connect?host=...&token=...`) allowing users to join a session by simply clicking a URL.
    - [ ] **Role-Based Access Control (RBAC)**: Introduce granular keys:
        - **Admin Key**: Full control over settings, agents, and system shutdown.
        - **Collaborator Key**: Can chat and use skills, but cannot modify agent configurations.
        - **Guest Key**: Read-only access to conversation logs and agent status.
    - [ ] **Key Management Dashboard**: A UI in the Panel to generate, name, and revoke specific collaboration keys.
    - [ ] **Connection Persistence**: Securely store remote host tokens in the local `Vault` to prevent re-authentication after restarts.

---

## Phase 13: Developer Experience & Documentation

### 13.1 Documentation Overhaul
- [ ] Create a comprehensive "Developer's Guide" for writing new Skills/Tools.
- [ ] Document the bit-level communication protocol between the `core` and the `panel` GUI.
