# AIMAXXING Roadmap

This document outlines the planned future developments for the AIMAXXING project.

## Phase 1: Enhanced Multi-Platform Security (CRITICAL)

### 1.1 Windows Shell Firewall Parity
- **Goal**: Bring the `ShellFirewall` rules in `core/src/security/shell_firewall.rs` to parity with Linux by adding Windows-specific command patterns.
- **Tasks**:
    - [ ] Add regex for Windows file deletion: `del`, `rd /s`, `erase`.
    - [ ] Add regex for Windows privilege escalation: `runas`.
    - [ ] Add regex for system disruption commands: `format`, `vssadmin delete shadows`.
    - [ ] Add regex for obfuscated Windows execution: `powershell -enc`, `powershell -EncodedCommand`, `certutil -urlcache`.
    - [ ] Implement path canonicalization for Windows (`\` vs `/`).

### 1.2 Resource Quota Hardening
- [ ] Expand Windows Job Object limits to include network I/O throttling per process.
- [ ] Implement disk I/O limits in Linux `bwrap` using `cgroups v2`.

### 1.3 macOS Security Strategy
- **Goal**: Harden the macOS execution environment beyond basic `Seatbelt` profiles.
- **Tasks**:
    - [ ] **TCC Integration**: Implement pre-flight checks for macOS "Full Disk Access" and "Input Monitoring" permissions to avoid silent failures.
    - [ ] **Seatbelt (sandbox-exec) Hardening**: Refine the Scheme profile to explicitly deny access to `~/Library/Keychains`, `~/Library/Safari`, and `~/Documents` unless explicitly whitelisted.
    - [ ] **Firewall Expansion**: Add regex for macOS-specific data exfiltration tools: `pbpaste`, `screencapture`, `mdfind` (potential sensitive file searching).
    - [ ] **Code Signing**: Integrate automatic self-signing for generated WASM/Native tools to satisfy macOS Gatekeeper requirements in local environments.

---

## Phase 2: Communication & Connectivity

### 2.1 Connector Ecosystem Expansion
- [ ] Implement **Feishu (ByteDance)** connector for enterprise workspace automation.
- [ ] Implement **Slack** Webhook & Real-time Messaging (RTM) support.
- [ ] Add support for **E-mail (SMTP/IMAP)** as a persistent communication channel.

### 2.2 Unified Notification Center
- [ ] Create a cross-connector notification abstraction to allow agents to "broadcast" important alerts to all active channels.

---

## Phase 3: Performance & RAG

### 3.1 Advanced Memory Compression
- [ ] Implement dynamic quantization switching (moving memory from FP32/U8 to INT4/Ternary as it ages).
- [ ] Optimize `redb` compacting logic for long-running deployments.

### 3.2 Visual Knowledge Ingestion
- [ ] Integrate local OCR (using Tesseract or similar WASM-based engines) for document parsing.

---

## Phase 4: Developer Experience

### 4.1 Documentation Overhaul
- [ ] Create a comprehensive "Developer's Guide" for writing new Skills/Tools.
- [ ] Document the bit-level communication protocol between the `core` and the `panel` GUI.

---

## Phase 5: Architectural Modularity & Decoupling

### 5.1 "Fat Core" Slimming
- **Goal**: Transition `brain` crate from monolithic implementation to a pure abstraction layer while **preserving full-stack connectivity** (Bus/Traits).
- **Tasks**:
    - [x] Extract `connectors/` into a standalone crate.
    - [x] Extract `security/` (Firewall + Sandbox) into a standalone crate.
    - [x] Extract `runtimes/` (Executors) into a standalone crate.
    - [x] Extract `skills/` (Engine + Built-in Tools) into a standalone crate.
    - [x] Extract `knowledge/`, `mcp/`, `auth/`, `infra/` into their respective crates.

### 5.2 Filesystem Hygiene
- [x] Direct all runtime artifacts (`.log`, `.pid`, `.json` tokens) to a unified `/data` or `/var` directory.
- [x] Remove all persistent state files from the project root.
We welcome contributions to any of these areas! Please open an issue or PR on the respective module.

---

## Phase 6: Native Windows Experience (CRITICAL TRANSITION)

*Constraint*: The entire AIMAXXING engine, including gateways, panels, and all spawned agent sandboxes, **MUST execute in user-space without Windows Administrator (`UAC`) privileges**.

### 6.1 Unified Zero-Admin Runtimes (QuickJS + Pixi `m2-bash`)
- **Goal**: Sever reliance on Git Bash or WSL by promoting embedded QuickJS, and safely containerizing legacy bash via Pixi's MSYS2 environment.
- **Tasks**:
    - [x] Prioritize embedded `QuickJS` for cross-platform hook scripts (`JsHook`) in `core/src/hooks/engine.rs` instead of relying on `ShellHook`.
    - [x] For `SKILL.md` scripts that strictly require `runtime: bash`, update `EnvManager` to automatically provision Pixi's `m2-bash` package (a portable MSYS2 environment). This guarantees 100% bug-free upstream bash compatibility without parsing errors.
    - [x] Bypass system bash for Python-based skills on Windows: intercept executions and route them directly to the `uv` virtual environment managed by `Pixi` in `core/src/env/mod.rs`.
    - [x] Ensure Windows paths (`\\?\` or C:\`) are precisely handled during bash context and `uv` environment provisioning in `runtimes/src/python_utils.rs`.

### 6.2 Cross-Platform Command Equivalents
- **Goal**: Refactor hardcoded Linux commands in built-in tools to use cross-platform Rust equivalents or OS-aware branches.
- **Tasks**:
    - [x] `core/src/env/mod.rs`: Replace `df -m` with pure-Rust `sysinfo` crate for disk space checking.
    - [x] `core/src/env/mod.rs`: Replace `sha256sum` / `shasum` with pure-Rust `sha2` crate for model checksums.
    - [x] `builtin-tools/src/tool/notifier.rs`: Ensure Windows 11 Toast Notifications are supported (e.g., using `winrt-notification` or `notify-rust` with Windows features enabled).
    - [x] `core/src/hooks/engine.rs`: Refactor `ShellHook` which hardcodes `Command::new("sh").arg("-c")` to use PowerShell or `cmd.exe` on Windows.
    - [x] `gateway/src/api/server.rs`: Update `handle_terminal_socket` which currently hardcodes `/bin/bash` to launch `powershell.exe` on Windows.

### 6.3 Developer Experience & Tooling
- **Goal**: Provide an accessible entry point for Windows developers.
- **Tasks**:
    - [x] Create `install.ps1` for native Windows environment setup (matching `install.sh`).
    - [x] Create `build_all.ps1` for compilation.
