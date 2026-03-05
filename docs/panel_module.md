# AIMAXXING Panel Module Documentation

The `panel` module is the official Graphical User Interface (GUI) Desktop application for the AIMAXXING AI Agent framework. It serves as the primary way a human user interacts with, manages, and configures their local or remote Agent Swarm.

## 🛠 Technology Stack & Dependencies

The `panel` is built to be a fast, native, cross-platform binary (Linux, Windows, macOS) using immediate-mode GUI concepts:
- **GUI Framework**: `eframe` (the egui framework integration) for drawing highly responsive, OpenGL/WGPU accelerated, resource-friendly UI components.
- **Async Runtime**: `tokio` for handling non-blocking background tasks like polling the Gateway, downloading skills, and fetching LLM metrics.
- **HTTP Client**: `reqwest` to interact with the AIMAXXING `gateway` REST APIs.
- **Data Serialization**: `serde` and `serde_json` for parsing API responses into UI State models.
- **Dates & Hashes**: `chrono` for timestamps in chat logs, and cryptographic primitives for token signing (Optional/If enabled).
- **Internationalization**: A custom built i18n module supporting English (`En`) and Chinese (`Zh`) directly inside the binary.
- **Fonts & Assets**: Bundles `HarmonyOS_Sans_SC` natively to ensure perfect CJK rendering across platforms.

## 📂 Architecture & Modularity

The `panel/src/` directory contains all UI layout, state management, API service calls, and styling logic.

### 1. Unified Application State
- **`app_state.rs`**: The single source of truth for the entire UI. Holds active chat histories, connected API keys, provider metadata, loaded skills (tools), selected agent persona (Soul), and navigation routing (e.g., Which tab is open?). It's passed mutably `&mut self.state` to rendering functions.

### 2. Rendering & Layout
- **`app.rs`**: The colossal immediate-mode drawing loop `fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame)`. 
  - Every frame (typically 60-144 times a second), the entire UI structure is recalculated and drawn based on `app_state.rs`.
  - Responsible for top-level navigation tabs: **Dashboard, API, Skills, Logs, Sessions, Cron, Persona, Connection, Chat**.
  - Contains all the custom styling overrides (Dark Mode palettes, rounding, shadows, and animations) required by immediate-mode graphics to look modern and sleek.

### 3. Network & Services
- **`api.rs`**: A collection of structured, asynchronous helper functions wrapped around `reqwest`. 
  - **Responsibilities**: Spawning `tokio::spawn` background promises for long-polling the Gateway (saving UI freezes), verifying LLM tokens, installing marketplace skills, saving agent personalities, and managing knowledge ingestion.

### 4. Utilities
- **`i18n.rs`**: Simple dictionary mapping keys like `tabs.skills` or `dashboard.total_tokens` to `"Skills"` / `"技能"` based on the user's selected language in `AppState`.
- **`main.rs`**: Application entry point. Initializes tracing logs, sets up the `eframe::NativeOptions` (window size, transparency, icon), loads custom fonts (like HarmonyOS SC and Roboto), and kicks off the `eframe::run_native` loop.

## 🚀 Purpose

The purpose of the `panel` is to convert the immense complexity of an autonomous, multi-agent AI system running in a terminal (the `gateway`) into a clean, intuitive, visual mission-control center.

Key functionalities include:
1. **Persona Editing**: Visually configuring the `brain`'s instructions, assigning models, and toggling specific Skills for an agent.
2. **Knowledge Management**: Uploading local files to push to the `engram` vector database via the `/v1/engram` API.
3. **Chat Interface**: Providing an OpenAI/ChatGPT-style view to converse with the running agent directly to test logic.
4. **Marketplace Integration**: Browsing, downloading, and injecting new Python/Wasm/Native tools from the Smithery/ClawHub repository seamlessly into the Gateway.
5. **Observability**: Exposing total token usage, latency metrics, background Cron-Job status, and raw internal logs (`tail` style) to the user.
