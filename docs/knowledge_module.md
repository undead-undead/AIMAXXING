# AIMAXXING Knowledge Module Documentation

The `knowledge` module is the high-level orchestration layer for Agent awareness. While `engram` handles the low-level data storage, the `knowledge` crate is responsible for deciding *how* and *when* an agent should search for facts to improve its reasoning.

## 🛠 Technology Stack & Dependencies

- **Vector Database Abstraction**: Uses the `VectorStore` and `Embeddings` traits from `brain::knowledge::rag`.
- **Async Runtime**: `tokio` for concurrent knowledge retrieval from multiple sources.
- **Intent Analysis**: `regex` for detecting knowledge-retrieval intents in user prompts.
- **Knowledge Graphs**: `petgraph` for managing hierarchical relationships between facts (experimental).
- **Serialization**: `serde_json` for structuring context for LLM injection.

## 📂 Architecture & Modules

### 1. Context Orchestration
- **`rag.rs`**: High-level implementation of the Retrieval-Augmented Generation (RAG) algorithm.
- **`router.rs`**: Decides whether a prompt requires a simple database lookup, a full-text search, or a call to the Knowledge Graph.
- **`intent.rs`**: Analyzes the semantic intent of a user query to determine the best search strategy.

### 2. Knowledge Graph (KG)
- **`kg.rs`**: (Advanced) Structures facts into a graph format, allowing agents to navigate complex relationships (e.g., "A is part of B").

### 3. File & Source Management
- **`virtual_path.rs`**: Maps the real-world file paths into a virtual, sandboxed knowledge structure that the Agent can safely reference.
- **`store/`**: Connects the `knowledge` orchestration layer to the persistent storage backends (like `engram`).

## 🚀 Purpose

The `knowledge` module's purpose is to **Provide Contextual Depth**. It acts as the "librarian" for the Agent, ensuring that its limited context window is always filled with the most relevant facts extracted from massive datasets, enabling more accurate and informed reasoning.
