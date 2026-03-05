//! Execution runtimes for AI agent skills.
//!
//! Provides the core abstraction and implementations for various
//! script execution environments (Wasm, QuickJS, Node.js, Python).

use async_trait::async_trait;
use brain::env::EnvManager;
use brain::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub use brain::skills::runtime::SkillRuntime;
pub use brain::skills::{ModelSpec, SkillExecutionConfig, SkillMetadata};

pub mod micropython;
pub mod node;
pub mod python_utils;
pub mod quickjs;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use micropython::MicroPythonRuntime;
pub use node::SmartNodeRuntime;
pub use quickjs::QuickJSRuntime;

#[cfg(feature = "wasm")]
pub use wasm::WasmRuntime;
