# AIMAXXING Skills & Tools Documentation

The `/home/biubiuboy/aimaxxing/core/src/skills/tool/` directory contains the native "Skills" (Tools) that dictate what an AIMAXXING Agent can actually *do* in the real world. Every tool implements the `Tool` trait and utilizes auto-generated JSON schemas (`schemars`) for perfect Frontend / LLM synchronization.

Below is an in-depth breakdown of every built-in Tool provided by the `brain` module.

---

## 💻 OS & File System Interaction

These tools form the core capabilities allowing agents to modify codebases and interact with their local isolated environment.

1. **`filesystem.rs`**: 
   - **`ListDirTool`**: Lists contents of a directory. Used by the agent to orient itself within a workspace.
   - **`ReadFileTool`**: Reads raw text from a local path.
   - **`WriteFileTool` / `EditFileTool`**: Allows the agent to persistently create or incrementally modify files. Safe-guarded by the Kernel and Application Firewalls to prevent jumping outside the designated Agent Workspace.
2. **`code_interpreter.rs`**: 
   - Evaluates dynamic code blocks (like Python or JS). Ties closely into the `sandbox.rs` and `runtime/` environment to execute scripts natively inside secure VM-like boundaries using `uv` or `rquickjs`.
3. **`git_ops.rs`**:
   - `GitOpsTool`: Allows agents to clone, commit, pull, push, and create pull requests. Completely automates software engineering agentic loops.

---

## 🧠 Memory & Context (RAG)

Tools explicitly designed to interface with the `engram` module.

4. **`memory.rs`**:
   - **`RememberThisTool`**: Explicit command overriding passive memory. Used when the user tells the agent: "Remember my name is John". Injects a high-priority Fact to the vector store.
   - **`SearchHistoryTool`**: Uses SIMD-accelerated semantic search to recall past conversations or facts from `engram`.
   - **`FetchDocumentTool`**: Combines keyword BM25 + Vector Search (using `rrf` Reciprocal Rank Fusion) to precisely extract knowledge documentation chunks.
   - **`TieredSearchTool`**: Multi-modal search bridging the Cold, Warm, and Background memory tiers using Quantized Codebooks.

---

## 🌐 Web & Network Integration

Tools for browsing the open internet.

5. **`browser.rs`**: 
   - `BrowserTool`: A heavyweight integration wrapping `headless_chrome` / Puppeteer. Allows the agent to literally spin up a hidden Chromium instance, click buttons, accept cookies, evaluate JS, and take screenshots to navigate complex SPAs (Single Page Applications) that cannot be scraped by simple HTTP.
6. **`web_fetch.rs`**: 
   - `WebFetchTool`: A high-speed, headless HTTP client (via `reqwest`). Converts arbitrary webpages into clean markdown using readability algorithms for fast LLM summarization.
7. **`web_search.rs`**: 
   - `WebSearchTool`: Integrates with DuckDuckGo/Tavily/Google Search APIs to let the agent retrieve real-time world knowledge (e.g., current stock prices, news).

---

## 🔄 Agentic Flow & Orchestration

Specialized tools that alter the lifecycle or spawn other agents.

8. **`delegation.rs`**: 
   - `DelegateTool`: Implements swarm logic. Allows a generic "Manager" agent to pause itself, spawn a specialized "Coder" or "Researcher" agent, hand them a sub-task, and wait for their response.
9. **`handover.rs`**: 
   - `HandoverTool`: An explicit trigger for an agent to yield control back to the Human (or UI) indicating it has finished a complex autonomous loop.
10. **`refine.rs`**: 
    - `RefineSkill`: Iterative self-reflection loop. The agent can critique its own previous output and trigger a re-generation loop before showing the final result to the user.
11. **`cron.rs`**:
    - `CronTool`: Exposes `tokio-cron-scheduler`. Permits the agent to schedule itself to wake up in the future. (e.g., "Remind me to check the database at 9 AM tomorrow").

---

## 🛠 Utility & Media Processing

Tools granting the agent auxiliary superpowers to handle specific data formats and communications.

12. **`chart.rs`**:
    - `ChartTool`: Generates PNG/SVG visual charts (Bar, Line, Pie) dynamically using `plotters` based on JSON data evaluated by the LLM.
13. **`cipher.rs`**:
    - `CipherTool`: Cryptography endpoints allowing agents to encrypt/decrypt strings, generate HMACs, or hash passwords safely for the user.
14. **`data_transform.rs`**:
    - `DataTransformTool`: An ultra-fast data munging tool. Allows the agent to instantly convert CSV -> JSON -> YAML -> XML natively in Rust without consuming LLM token limits to "re-write" the format.
15. **`image.rs`**:
    - Processing for resizing, cropping, or identifying meta-data inside imagery.
16. **`mailer.rs`**:
    - `MailerTool`: SMTP integration. Allows the agent to construct and fire off formatted HTML emails to external users.
17. **`notifier.rs`**:
    - `NotifierTool`: Fires Webhooks (e.g., to Slack, Discord) or OS-level Toast notifications when a long-running background agent task completes.
18. **`text_extract.rs`**:
    - `TextExtractTool`: A document ingestion pipeline for PDF, DOCX, and PPTX mapping into raw text buffers accessible to the prompt context.
19. **`voice.rs`**:
    - `TranscribeTool` / `SpeakTool`: Hooks into Whisper APIs (or local models) for Audio->Text, and Text-to-Speech (TTS) bindings to allow agents to generate audible feedback.
20. **`forge.rs`**:
    - `ForgeSkill`: A meta-tool that allows an agent to actually *write and compile* new Rust tools dynamically to expand its own capability set at runtime. 

## 🛡️ Security Implementation
All tools listed above strictly adhere to the `vessel` boundary limits and pass outputs back through the `SecurityManager` (e.g. `LeakDetector`) before returning execution state to the prompt.
