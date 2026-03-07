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

### 5.1 Unified Zero-Admin Runtimes (QuickJS + Pixi `m2-bash`)
- **Goal**: Sever reliance on Git Bash or WSL by promoting embedded QuickJS, and safely containerizing legacy bash via Pixi's MSYS2 environment.
- **Tasks**:
    - [x] Prioritize embedded `QuickJS` for cross-platform hook scripts (`JsHook`) in `core/src/hooks/engine.rs` instead of relying on `ShellHook`.
    - [x] For `SKILL.md` scripts that strictly require `runtime: bash`, update `EnvManager` to automatically provision Pixi's `m2-bash` package (a portable MSYS2 environment). This guarantees 100% bug-free upstream bash compatibility without parsing errors.
    - [x] Bypass system bash for Python-based skills on Windows: intercept executions and route them directly to the `uv` virtual environment managed by `Pixi` in `core/src/env/mod.rs`.
    - [x] Ensure Windows paths (`\\?\` or C:\`) are precisely handled during bash context and `uv` environment provisioning in `runtimes/src/python_utils.rs`.

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
    - [ ] **Optional Media Downloader**: Add "Download Media Components" buttons in the Model Management UI (sharing the same unified downloader logic as Llama models).
    - [ ] **Local Whisper (STT)**: Implement a local speech-to-text runner (via `whisper.cpp` or `sherpa-onnx`) for instant transcription.
        - [ ] **STT Language Model Selector**: In the Panel, provide a selector to choose which language model to download/activate (Chinese, English, Japanese, Korean, etc.) for transcription.
        - [ ] **Space-Saving "Swap" Logic**: Only the selected transcription model is kept active in memory. Switching the target language triggers the Phase 3.5 Model Pool to swap models to save system resources.
    - [ ] **Local Piper (TTS)**: Implement a high-speed, local neural text-to-speech engine using `Piper` with curated voice models.
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

## Phase 11: Developer Experience

### 12.1 Documentation Overhaul
- [ ] Create a comprehensive "Developer's Guide" for writing new Skills/Tools.
- [ ] Document the bit-level communication protocol between the `core` and the `panel` GUI.
