//! Filesystem tools (Reader, Writer, Editor, Lister)
//! 
//! Provides robust file operations with strict workspace sandboxing.
//! Ported and adapted for AIMAXXING.

use std::path::{Path, PathBuf};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, warn};
use tokio::fs;

use brain::error::Error;
use super::{Tool, ToolDefinition};

/// Helper function to validate that a path stays within the workspace root.
/// Returns the absolute path if valid, or an error if outside.
fn validate_path(workspace: &Path, relative_path: &str) -> anyhow::Result<PathBuf> {
    // 1. Resolve relative path
    let full_path = if relative_path.starts_with('/') {
        // If absolute, key check is it must start with workspace
        PathBuf::from(relative_path)
    } else {
        workspace.join(relative_path)
    };

    // 2. Canonicalize (resolve .., symlinks)
    // Note: fs::canonicalize requires file to exist for some checks, but we might be creating it.
    // For creation, we check the parent.
    // However, a simple robust check is to normalize components.
    
    // Rust's std::fs::canonicalize returns error if path doesn't exist.
    // So we use a purely lexical normalization if possible, or fall back to checking parent.
    
    // For security, we DO want to resolve symlinks to ensure they don't point outside.
    // So we should canonicalize. If it doesn't exist, we can't fully verify symlinks until creation.
    // But for `ReadFile`, it must exist.
    // For `WriteFile`, parent must exist (usually).
    
    // Let's use a simplified approach:
    // 1. Clean the path (remove . and ..) using a crate or simple logic?
    //    Actually, `fs::canonicalize` is the safest but requires existence.
    //    We can assume the agent is working with existing paths or creating new ones in existing dirs.
    
    // Let's implement a mix:
    // If path exists -> canonicalize and check prefix.
    // If path doesn't exist -> canonicalize parent and check prefix, then check filename.
    
    let path_to_check = if full_path.exists() {
        full_path.canonicalize()?
    } else if let Some(parent) = full_path.parent() {
        if parent.exists() {
            let canon_parent = parent.canonicalize()?;
            canon_parent.join(full_path.file_name().unwrap())
        } else {
             // Parent doesn't exist either. This is tricky.
             // For strict security, we might reject deep creation in non-existent trees if we can't verify.
             // But `mkdir -p` is useful.
             // We can check if `full_path` starts with `workspace` textually as a fallback, 
             // assuming no malicious symlinks in the non-existent part.
             // BUT `workspace` itself should be canonicalized first.
             full_path
        }
    } else {
        full_path
    };

    let workspace_canon = if workspace.exists() {
        workspace.canonicalize()?
    } else {
        workspace.to_path_buf()
    };

    if path_to_check.starts_with(&workspace_canon) {
        Ok(path_to_check)
    } else {
        anyhow::bail!("Access Denied: Path '{}' is outside workspace '{}'", relative_path, workspace.display())
    }
}

// ─── 1. ReadFileTool ─────────────────────────────────────────────────────────

pub struct ReadFileTool {
    workspace: PathBuf,
}

impl ReadFileTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> String {
        "read_file".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the full content of a file from the filesystem.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file (relative to workspace root)"
                    }
                },
                "required": ["path"]
            }),
            parameters_ts: Some("interface ReadFile {\n  path: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use this to read code, configuration, or text files. If you need to search, use `list_dir` first.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { path: String }
        let args: Args = serde_json::from_str(arguments)?;
        
        let safe_path = validate_path(&self.workspace, &args.path)?;
        
        debug!("Reading file: {:?}", safe_path);
        let content = fs::read_to_string(&safe_path).await
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
            
        Ok(content)
    }
}

// ─── 2. WriteFileTool ────────────────────────────────────────────────────────

pub struct WriteFileTool {
    workspace: PathBuf,
}

impl WriteFileTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> String {
        "write_file".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file. Overwrites existing files. Creates parent directories if needed.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full content to write"
                    }
                },
                "required": ["path", "content"]
            }),
            parameters_ts: Some("interface WriteFile {\n  path: string;\n  content: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use to create new files or overwrite existing ones. careful with overwrites.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { path: String, content: String }
        let args: Args = serde_json::from_str(arguments)?;
        
        let safe_path = validate_path(&self.workspace, &args.path)?;
        
        if let Some(parent) = safe_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }
        
        fs::write(&safe_path, &args.content).await
            .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;
            
        Ok(format!("Successfully wrote {} bytes to {}", args.content.len(), args.path))
    }
}

// ─── 3. ListDirTool ──────────────────────────────────────────────────────────

pub struct ListDirTool {
    workspace: PathBuf,
}

impl ListDirTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> String {
        "list_dir".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_dir".to_string(),
            description: "List files and directories in a given path.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path (use '.' for root)"
                    }
                },
                "required": ["path"]
            }),
            parameters_ts: Some("interface ListDir {\n  path: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: None,
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { path: String }
        let args: Args = serde_json::from_str(arguments)?;
        
        let safe_path = validate_path(&self.workspace, &args.path)?;
        
        let mut entries = fs::read_dir(&safe_path).await
            .map_err(|e| anyhow::anyhow!("Failed to read directory: {}", e))?;
            
        let mut items = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let meta = entry.metadata().await?;
            let name = entry.file_name().to_string_lossy().to_string();
            let suffix = if meta.is_dir() { "/" } else { "" };
            items.push(format!("{}{}", name, suffix));
        }
        
        items.sort();
        
        if items.is_empty() {
            Ok("(empty directory)".to_string())
        } else {
            Ok(items.join("\n"))
        }
    }
}

// ─── 4. EditFileTool ─────────────────────────────────────────────────────────

pub struct EditFileTool {
    workspace: PathBuf,
}

impl EditFileTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> String {
        "edit_file".to_string()
    }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit_file".to_string(),
            description: "Edit a file by replacing a specific block of text.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file"
                    },
                    "old_text": {
                        "type": "string",
                        "description": "Exact text block to replace"
                    },
                    "new_text": {
                        "type": "string",
                        "description": "New content to insert"
                    }
                },
                "required": ["path", "old_text", "new_text"]
            }),
            parameters_ts: Some("interface EditFile {\n  path: string;\n  old_text: string;\n  new_text: string;\n}".to_string()),
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use for small edits. Provide enough context in `old_text` to be unique.".to_string()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Args { path: String, old_text: String, new_text: String }
        let args: Args = serde_json::from_str(arguments)?;
        
        let safe_path = validate_path(&self.workspace, &args.path)?;
        let content = fs::read_to_string(&safe_path).await?;
        
        // Normalize line endings? ZeptoClaw doesn't, maybe we should?
        // Let's stick to strict replacement first.
        
        if !content.contains(&args.old_text) {
             // Fallback: Try with trimmed whitespace if exact match fails?
             // We are strict for safety.
             anyhow::bail!("Text to replace not found in file. Ensure exact match including whitespace.");
        }
        
        let new_content = content.replace(&args.old_text, &args.new_text);
        
        fs::write(&safe_path, &new_content).await?;
        
        Ok(format!("Successfully modified {}", args.path))
    }
}
