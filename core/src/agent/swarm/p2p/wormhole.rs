use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn, error};
use magic_wormhole::{
    transfer::{send_file, receive_file, TransitInfo},
    transit::{Transit, Abilities},
    Wormhole, WormholeWelcome, Code,
};

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
            // Using the official public mailbox server for now
            rendezvous_url: "ws://relay.magic-wormhole.io:4000/v1".to_string(),
            // Unique app ID for AIMAXXING to prevent cross-talk with standard wormhole clients
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
    /// Note: This function blocks/yields until the transfer is complete or an error occurs.
    pub async fn send_vessel(&self, file_path: PathBuf) -> Result<String> {
        if !file_path.exists() || !file_path.is_file() {
            return Err(Error::Internal(format!("Vessel file not found: {:?}", file_path)));
        }

        info!("Initializing Magic Wormhole sender for {:?}", file_path);

        // 1. Connect to the rendezvous server to get a welcome message and allocate a code
        let (welcome, mut wormhole) = Wormhole::connect_without_code(
            self.config.app_id.clone(),
            self.config.rendezvous_url.clone(),
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to connect to mailbox server: {}", e)))?;

        // Extract the generated code to show to the user
        let code = wormhole.code().to_string();
        info!("Generated Wormhole Code (share this with the receiver): {}", code);

        // 2. Setup Transit (Direct P2P or Relay)
        let abilities = Abilities::ALL_ABILITIES;
        let mut transit = Transit::new(self.config.app_id.clone(), abilities)
            .map_err(|e| Error::Internal(format!("Failed to create Transit state: {}", e)))?;

        // Start the transfer process
        // Note: The `send_file` API in magic-wormhole takes care of the key exchange and encryption
        info!("Waiting for receiver to join using code: {}", code);

        send_file(
            &mut wormhole,
            &mut transit,
            &file_path,
            file_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            &welcome,
            None, // No specific cancellation mechanism for now
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to send vessel: {}", e)))?;

        info!("Successfully sent vessel file: {:?}", file_path);
        Ok(code)
    }

    /// Connects using the provided code and receives the .vessel file into `download_dir`.
    /// Returns the absolute path to the downloaded file.
    pub async fn receive_vessel(&self, code_str: &str, download_dir: PathBuf) -> Result<PathBuf> {
        if !download_dir.exists() {
            tokio::fs::create_dir_all(&download_dir).await.map_err(|e| {
                Error::Internal(format!("Failed to create download directory: {}", e))
            })?;
        }

        let code = Code(code_str.to_string());
        info!("Connecting with Wormhole Code: {}", code);

        // 1. Connect to the rendezvous server using the provided code
        let (welcome, mut wormhole) = Wormhole::connect_with_code(
            self.config.app_id.clone(),
            self.config.rendezvous_url.clone(),
            code,
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to connect to mailbox server: {}", e)))?;

        info!("Connected to sender. Establishing transfer tunnel...");

        // 2. Setup Transit
        let abilities = Abilities::ALL_ABILITIES;
        let mut transit = Transit::new(self.config.app_id.clone(), abilities)
            .map_err(|e| Error::Internal(format!("Failed to create Transit state: {}", e)))?;

        // 3. Receive the file
        let (file_name, mut receive_stream) = receive_file(
            &mut wormhole,
            &mut transit,
            &welcome,
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to receive file metadata: {}", e)))?;

        let safe_file_name = std::path::Path::new(&file_name)
            .file_name()
            .ok_or_else(|| Error::Internal("Invalid file name received".to_string()))?;
            
        let dest_path = download_dir.join(safe_file_name);
        info!("Receiving vessel file into: {:?}", dest_path);

        // Accept the file transfer immediately (we could prompt here in a UI)
        receive_stream.accept().await.map_err(|e| {
            Error::Internal(format!("Failed to accept file transfer: {}", e))
        })?;

        // Save the file
        let mut file = tokio::fs::File::create(&dest_path).await.map_err(|e| {
            Error::Internal(format!("Failed to create local file: {}", e))
        })?;

        tokio::io::copy(&mut receive_stream, &mut file).await.map_err(|e| {
            Error::Internal(format!("Failed to save received file: {}", e))
        })?;

        info!("Successfully received vessel file: {:?}", dest_path);
        
        // Wait for protocol completion (ack)
        receive_stream.wait_for_completion().await.map_err(|e| {
            Error::Internal(format!("Error completing protocol: {}", e))
        })?;

        Ok(dest_path)
    }
}
