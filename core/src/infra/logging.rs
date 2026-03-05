//! Logging configuration with rotation support
//!
//! # Example
//!
//! ```rust,no_run
//! use brain::infra::logging::init_logging;
//!
//! init_logging("logs", "agent.log", "info").unwrap();
//! ```

use crate::error::Result;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize logging with file rotation and optional Tokio Console
///
/// - `directory`: Directory to store logs
/// - `filename_prefix`: Prefix for log files (e.g. "aimaxxing.log")
/// - `level`: Default log level (e.g. "info", "debug")
pub fn init_logging(directory: &str, filename_prefix: &str, level: &str) -> Result<()> {
    // 1. File Appender with Rotation (Daily)
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(filename_prefix)
        .build(directory)
        .map_err(|e| {
            crate::error::Error::Internal(format!("Failed to create log appender: {}", e))
        })?;

    // 2. Formatting layers
    // Stdout: Human readable
    let stdout_layer = fmt::layer().with_target(false).compact();

    // File: JSON for parsing or full text
    let file_layer = fmt::layer().with_writer(file_appender).with_ansi(false);

    // 3. Filter
    // Allow RUST_LOG env var to override, otherwise use default
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    // 4. Registry
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer);

    // 5. Tokio Console (Optional via Feature or Config)
    // We enable it by default in this refactor to improve Observability.
    let (console_layer, server) = console_subscriber::ConsoleLayer::builder()
        .with_default_env()
        .build();

    // Spawn console server in background
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build console server runtime");
        
        runtime.block_on(async move {
            if let Err(e) = server.serve().await {
                eprintln!("Console server failed: {}", e);
            }
        });
    });

    registry
        .with(console_layer)
        .try_init()
        .map_err(|e| crate::error::Error::Internal(format!("Failed to init tracing: {}", e)))?;

    Ok(())
}
