# AIMAXXING Connectors Module Documentation

The `connectors` module provides the data ingestion pipes for AIMAXXING. It is responsible for pulling unstructured data from the web, file systems, and corporate APIs into the `engram` vector database.

## 🛠 Technology Stack & Dependencies

- **Web Scraping**: `reqwest` for HTTP fetching and `scraper` (based on `html5ever`) for high-speed HTML parsing.
- **PDF Extraction**: `pdf-extract` for Converting PDF files into clean, searchable markdown text.
- **Async Runtime**: `tokio` and `futures` for parallel data fetching.
- **Serialization**: `serde_json` for mapping external API payloads into the AIMAXXING `Document` schema.
- **Metadata Extraction**: `opengraph` and standard metadata parsers for enriching document entries.

## 📂 Architecture & Modules

### 1. Web Connectors
- **`web/`**: Implements URL scraping, following links (crawling), and cleaning HTML boilerplate into readable markdown.
- **`rss.rs`**: Integration for monitoring news feeds and blog updates.

### 2. Document Parsers
- **`pdf.rs`**: Handling of binary PDF data and text alignment.
- **`markdown.rs`**: Normalization of diverse markdown dialects into a standard AIMAXXING internal format.

### 3. API Integrations
- **`github.rs`**: Connectors for pulling issues, PRs, and repository contents.
- **`notion.rs`**: (Experimental) Integration for syncing knowledge from Notion workspaces.

## 🚀 Purpose

The `connectors` module's purpose is to **Bridge the Knowledge Gap**. It automates the process of turning raw, fragmented data from around the web and local disks into structured facts that the `knowledge` and `engram` modules can process.
