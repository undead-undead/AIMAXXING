//! Vessel Exchange via Magic Wormhole
//!
//! NOTE: This module is a stub pending migration to the "hole punching" connectivity
//! strategy. The previous implementation used magic-wormhole APIs that have changed
//! significantly in 0.7.x. This stub preserves the public interface so the rest of
//! the codebase compiles, but the actual transfer logic is not yet re-implemented.

use std::path::PathBuf;
use tracing::info;

use crate::error::{Error, Result};

/// Configuration for the Vessel Exchange (Magic Wormhole).
#[derive(Debug, Clone)]
pub struct WormholeConfig {
    /// The default rendezvous (mailbox) server URL.
    pub rendezvous_url: String,
    /// The application ID string for the wormhole connection.
    pub app_id: String,
}

impl Default for WormholeConfig {
    fn default() -> Self {
        Self {
            rendezvous_url: "ws://relay.magic-wormhole.io:4000/v1".to_string(),
            app_id: "lmdx.aimaxxing.vessel-exchange.v1".to_string(),
        }
    }
}

/// Service to handle on-demand P2P exchange of .vessel files via Magic Wormhole.
pub struct VesselExchange {
    config: WormholeConfig,
}

impl VesselExchange {
    pub fn new() -> Self {
        Self {
            config: WormholeConfig::default(),
        }
    }

    pub fn with_config(config: WormholeConfig) -> Self {
        Self { config }
    }

    /// Generates a code, waits for the receiver, and sends the .vessel file.
    /// Returns the generated code that must be shared with the receiver.
    ///
    /// **STUB**: Pending re-implementation with updated magic-wormhole 0.7.x API
    /// or replacement with hole-punching transport.
    pub async fn send_vessel(&self, file_path: PathBuf) -> Result<String> {
        if !file_path.exists() || !file_path.is_file() {
            return Err(Error::Internal(format!("Vessel file not found: {:?}", file_path)));
        }

        info!("VesselExchange::send_vessel is a stub — awaiting hole-punching migration");
        Err(Error::Internal(
            "P2P send not yet re-implemented. Pending hole-punching migration.".to_string(),
        ))
    }

    /// Connects using the provided code and receives the .vessel file into `download_dir`.
    /// Returns the absolute path to the downloaded file.
    ///
    /// **STUB**: Pending re-implementation with updated magic-wormhole 0.7.x API
    /// or replacement with hole-punching transport.
    pub async fn receive_vessel(&self, code_str: &str, download_dir: PathBuf) -> Result<PathBuf> {
        if !download_dir.exists() {
            tokio::fs::create_dir_all(&download_dir).await.map_err(|e| {
                Error::Internal(format!("Failed to create download directory: {}", e))
            })?;
        }

        info!(
            "VesselExchange::receive_vessel is a stub — code={}, dir={:?}",
            code_str, download_dir
        );
        Err(Error::Internal(
            "P2P receive not yet re-implemented. Pending hole-punching migration.".to_string(),
        ))
    }
}
