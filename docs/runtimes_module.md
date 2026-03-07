# AIMAXXING Runtimes Module Documentation

The `runtimes` module provides the execution engines for AI skills. It encapsulates the complex logic of process spawning, environment variables, and in-language bindings to support diverse task execution.

## 🛠 Technology Stack & Dependencies

- **JavaScript/TypeScript**: `rquickjs` (an FFI bridge to the QuickJS engine) for ultra-lightweight, in-process execution.
- **Python Provisioning**: `uv` (a Rust-based Python package manager) for high-speed workspace setup and dependency resolution.
- **Python Environment**: `pixi` for robust environment isolation and cross-platform management.
- **Process Management**: `tokio::process` for asynchronous subprocess control.
- **Data Exchange**: `serde_json` for high-speed IPC (Inter-Process Communication) and argument passing.

## 📂 Architecture & Modules

### 1. In-Process Engines
- **`js/`**: Implements the **QuickJS** runtime. It allow agents to execute logic and data transformations in JS/TS without the overhead of external Node.js.
- **`micropython/`**: (Experimental) Standardized interface for MicroPython as a lightweight alternative to full CPython.

### 2. External Subprocess Runtimes
- **`python_utils.rs`**: Orchestrates **uv** and **pixi** to provision isolated Virtual Environments for Python-based skills.
- **`shell/`**: Provides the bridge for executing native bash, PowerShell, and Zsh scripts within a protected security sandbox.

### 3. Execution Infrastructure
- **`runner.rs`**: The high-level coordinator that chooses the correct runtime for a skill based on its `SKILL.md` metadata.
- **`ipc.rs`**: Standards for how agents exchange JSON-RPC style messages with their running skill processes.

## 🚀 Purpose

The `runtimes` module's purpose is to allow **Dynamic Capability Expansion**. Instead of AIMAXXING being limited to a static set of features, agents can download and execute thousands of community skills written in Python, JS, or Bash, with the runtime handling the heavy-lifting of environment setup and secure execution.
