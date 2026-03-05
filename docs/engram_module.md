# AIMAXXING Engram Module Documentation

The `engram` module is the high-performance memory and knowledge storage engine designed specifically for the AIMAXXING framework. It functions as the long-term memory of the AI Agents.

## 🛠 Technology Stack & Dependencies

`engram` leverages several extremely fast Rust technologies to achieve semantic understanding and lightning fast retrieval:
- **Storage Engine**: `redb` (a pure Rust, ACID-compliant, embedded Key-Value store using memory mapped I/O).
- **Vector Search Engine**: `hnsw_rs` (Hierarchical Navigable Small World graphs for fast Approximate Nearest Neighbor search).
- **SIMD Acceleration**: `simsimd` for hardware-accelerated vector distance calculations (cosine similarity, dot product).
- **Machine Learning Integration**: `candle-core`, `candle-nn`, `candle-transformers`, and `tokenizers` from Hugging Face for running embedding models entirely within the Rust process without needing external APIs.
- **Full Text Search**: A custom built pure-Rust BM25 implementation for classical keyword text search (`fts.rs`).
- **Data Hashing**: `sha2` and `hex` for generating deterministic content IDs (content-addressable storage).
- **File Watching**: `notify` for real-time monitoring of disk directories (Active Indexing).
- **Multi-language Support**: `jieba-rs` for CJK (Chinese, Japanese, Korean) word segmentation.

## 📂 Architecture & Modules

The `engram/src/` directory contains the building blocks for creating a robust memory backend:

### 1. Unified Storage
- **`kv.rs`**: Implements the `redb` wrapper for saving raw document contents, chunk metadata, and indexes reliably to disk.
- **`store.rs`**: The high-level coordinator that ties Vector indexing and BM25 together, syncing them with the underlying key-value store.

### 2. Search Modalities
- **`fts.rs`**: Implements the BM25 text search algorithm. It maintains inverted indexes and term frequencies for exact word matching.
- **`hybrid_search.rs`**: Combines the results of `vector_store` (semantic search - meaning) and `fts` (keyword search - exact text).
- **`rrf.rs`**: Re-ranking logic. Implements Reciprocal Rank Fusion to logically merge and score the results returned by both Hybrid Search branches.

### 3. Data Ingestion & Processing
- **`chunker.rs`**: Splits large files (markdown, txt, logic) into small, semantically meaningful "chunks" while maintaining chunk overlap so context isn't lost.
- **`embedder.rs`**: Interfaces with Candle or external providers to convert text chunks into high-dimensional vector arrays (`f32`).
- **`content_hash.rs`**: Ensures that identical pieces of content get the same deterministic hash, avoiding duplicity in the knowledge base.

### 4. Advanced Algorithms & Compression
- **`quant.rs`** (Differentiated Memory Quantization): Implements tiered scalar/ternary quantization to reduce the RAM footprint of the vector database without losing persona stability.
  - **Full (FP32)** for core "Soul" meta-instructions
  - **Warm (U8)** scalar quantization for relevant recent memory
  - **Cold (INT4)** 4-bit scalar for background knowledge
  - **Background (Q1.58 Ternary)** for extreme compression of massive datasets
- **`vector_store.rs`** (HNSW, PQ & Hyperbolic Space): 
  - Provides **Product Quantization (PQ)** for compressing f32 inputs before HNSW insertion.
  - Features dynamic distance metric selection, including standard `SimdCosineDistance` and **`HyperbolicPoincareDistance`** for mapping hierarchical "Layer 2" data efficiently.

### 5. Agent Integration
- **`agent_memory.rs`**: The bridge between `engram` and `brain`. Defines how an Agent saves conversation histories and structured memories into the storage engine.
- **`tool.rs`**: Exposes standard Tools that the LLM can use to directly query the `engram` database.

## 🚀 Purpose

The primary purpose of `engram` is to allow AIMAXXING agents to ingest gigabytes of documentation, codebases, or chat history and accurately retrieve the exact context they need to answer a prompt (Retrieval-Augmented Generation / RAG). By running Candle and redb entirely in-process, it completely eliminates the need to deploy and manage heavy external vector databases like Chroma, Pinecone, or Milvus.
