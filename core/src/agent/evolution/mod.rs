//! Phase 12-A: Evolution pipeline for autonomous immunity and self-improvement.
//!
//! Contains:
//! - `auditor` - Independent LLM-based auditing of changes
//! - `observation` - Observation window for quarantining new interactions
//! - `rollback` - SOUL.md snapshot and rollback mechanism
//! - `consolidation` - Sleep-consolidation for memory verification

pub mod auditor;
pub mod auto_reflection;
pub mod consolidation;
pub mod evolution_manager;
pub mod memory_decay;
pub mod merger;
pub mod observation;
pub mod rollback;
