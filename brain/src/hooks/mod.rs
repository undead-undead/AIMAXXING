//! Hook System Module
//!
//! Provides a pre/post-processing pipeline for Agent execution.
//! Hooks can run before/after LLM calls, tool calls, and responses.
//!
//! Completely decoupled — zero overhead if no hooks are registered.

pub mod discovery;
pub mod engine;

pub use discovery::discover_hooks;
pub use engine::{Hook, HookEngine, HookEvent, HookResult, HookTiming};
