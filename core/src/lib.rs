//! # AIMAXXING Core - AI Agent for Trading
//!
//! Core types, traits, and abstractions for the AIMAXXING framework.
//!
//! This crate provides:
//! - Agent system (`agent`)
//! - Trading logic (`trading`)
//! - Skills & Tools (`skills`)
//! - Knowledge & Memory (`knowledge`)
//! - Infrastructure (`infra`)

pub mod error;

pub mod agent;
#[cfg(not(target_arch = "wasm32"))]
pub mod approval;
#[cfg(not(target_arch = "wasm32"))]
pub mod auth;
#[cfg(not(target_arch = "wasm32"))]
pub mod bus; // NEW: Message Bus
pub mod config;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub mod connectors;
#[cfg(not(target_arch = "wasm32"))]
pub mod env;
#[cfg(not(target_arch = "wasm32"))]
pub mod hooks;
#[cfg(not(target_arch = "wasm32"))]
pub mod infra;
#[cfg(feature = "vector-db")]
pub mod knowledge;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub mod mcp;
pub mod notification;
pub mod prelude;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
pub mod security;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod skills;

// Re-export common types for convenience
#[cfg(not(target_arch = "wasm32"))]
pub use agent::core::{Agent, AgentBuilder, AgentConfig};
pub use agent::message::{Content, Message, Role};
pub use error::{Error, Result};
