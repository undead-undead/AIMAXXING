//! Approval System Module
//!
//! Provides a policy-based approval mechanism for tool calls.
//! Supports auto-approve, deny, and interactive-ask modes.
//!
//! Completely optional — if no policies are configured, everything is auto-approved.

pub mod policy;

pub use policy::{ApprovalDecision, ApprovalPolicy, PolicyEngine, ToolPolicy};
