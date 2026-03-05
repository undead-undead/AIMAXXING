//! Prelude: Re-exports common types for convenience
//!
//! # Usage
//! ```
//! use brain::prelude::*;
//! ```

pub use crate::error::{Error, Result};

// Agent
#[cfg(not(target_arch = "wasm32"))]
pub use crate::agent::context::{ContextConfig, ContextInjector, ContextManager};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::agent::core::{Agent, AgentBuilder, AgentConfig};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::agent::memory::{Memory, MemoryManager, ShortTermMemory};
pub use crate::agent::message::{Content, ContentPart, ImageSource, Message, Role, ToolCall};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::agent::personality::{Persona, Traits};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::agent::provider::Provider;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::agent::streaming::{StreamingChoice, StreamingResponse};

// Skills
#[cfg(not(target_arch = "wasm32"))]
pub use crate::skills::tool::{Tool, ToolDefinition};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::skills::{DynamicSkill, SkillExecutionConfig, SkillLoader};

// Infra
#[cfg(not(target_arch = "wasm32"))]
pub use crate::infra::maintenance::{MaintenanceConfig, MaintenanceManager};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::notification::{Notifier, NotifyChannel};
