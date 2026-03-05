//! MCP Transport Layer
//!
//! Provides Stdio transport for communicating with MCP servers.
//! Handles JSON-RPC framing, process lifecycle, and graceful shutdown.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, oneshot};
use tracing::{debug, warn};

use super::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// Trait for MCP transports (Stdio, SSE, etc.)
#[async_trait::async_trait]
pub trait McpTransport: Send + Sync {
    /// Send a JSON-RPC request and wait for the response.
    async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse>;

    /// Send a JSON-RPC notification (fire-and-forget).
    async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()>;

    /// Check if the transport is still alive.
    async fn is_alive(&self) -> bool;

    /// Kill the transport / child process.
    async fn kill(&self);
}

/// Stdio-based MCP transport that communicates via child process stdin/stdout.
pub struct StdioTransport {
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    /// Pending request waiters
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    next_id: AtomicU64,
    /// Background reader task handle
    _reader_handle: tokio::task::JoinHandle<()>,
}

impl StdioTransport {
    /// Spawn a child process and set up the transport.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Arc<dyn McpTransport>> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server: {} {:?}", command, args))?;

        let stdin = child
            .stdin
            .take()
            .context("Failed to capture child stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture child stdout")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Background task: read stdout line by line and dispatch responses
        let pending_clone = Arc::clone(&pending);
        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<JsonRpcResponse>(trimmed) {
                            Ok(resp) => {
                                if let Some(id) = resp.id {
                                    let mut pending = pending_clone.lock().await;
                                    if let Some(sender) = pending.remove(&id) {
                                        let _ = sender.send(resp);
                                    } else {
                                        debug!(id = id, "Received response for unknown request ID");
                                    }
                                }
                                // Notifications from the server (no id) are currently ignored
                            }
                            Err(e) => {
                                debug!(
                                    error = %e,
                                    line = trimmed,
                                    "Failed to parse JSON-RPC response from MCP server"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Error reading from MCP server stdout");
                        break;
                    }
                }
            }
        });

        Ok(Arc::new(Self {
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            _reader_handle: reader_handle,
        }))
    }
}

#[async_trait::async_trait]
impl McpTransport for StdioTransport {
    async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let mut json = serde_json::to_string(&request)?;
        json.push('\n');

        // Register the waiter before sending
        let (tx, rx) = oneshot::channel();
        {
            self.pending.lock().await.insert(id, tx);
        }

        // Send the request
        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(json.as_bytes())
                .await
                .context("Failed to write to MCP server stdin")?;
            stdin.flush().await?;
        }

        // Wait for the response with a timeout
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                // Remove the pending entry on timeout
                let pending = self.pending.clone();
                tokio::spawn(async move {
                    pending.lock().await.remove(&id);
                });
                anyhow::anyhow!("MCP request '{}' timed out after 30s", method)
            })?
            .map_err(|_| anyhow::anyhow!("MCP response channel closed"))?;

        // Check for JSON-RPC error
        if let Some(ref error) = response.error {
            anyhow::bail!("MCP error: {}", error);
        }

        Ok(response)
    }

    async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        let mut json = serde_json::to_string(&notification)?;
        json.push('\n');

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(json.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn is_alive(&self) -> bool {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(None) => true,  // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,
        }
    }

    async fn kill(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
    }
}
