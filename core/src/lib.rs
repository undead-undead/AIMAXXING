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
// auth moved to auth crate
#[cfg(not(target_arch = "wasm32"))]
pub mod bus; // NEW: Message Bus
pub mod config;
// connectors moved to standalone 'connectors' crate
#[cfg(not(target_arch = "wasm32"))]
pub mod env;
#[cfg(not(target_arch = "wasm32"))]
pub mod hooks;
#[cfg(not(target_arch = "wasm32"))]
pub mod infra;
pub mod knowledge;
pub mod notification;
pub mod prelude;
// runtime and security moved to standalone crates - traits kept in core
#[cfg(not(target_arch = "wasm32"))]
pub mod security;
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod skills;

// Re-export common types for convenience
#[cfg(not(target_arch = "wasm32"))]
pub use agent::core::{Agent, AgentBuilder, AgentConfig};
pub use agent::message::{Content, Message, Role};
pub use error::{Error, Result};
