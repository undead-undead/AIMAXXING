#[cfg(not(target_arch = "wasm32"))]
pub mod attempt;
#[cfg(not(target_arch = "wasm32"))]
pub mod cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod context;
#[cfg(not(target_arch = "wasm32"))]
pub mod core;
#[cfg(not(target_arch = "wasm32"))]
pub mod history;
#[cfg(not(target_arch = "wasm32"))]
pub mod kv_cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod memory;
pub mod message;
#[cfg(not(target_arch = "wasm32"))]
pub mod multi_agent;
#[cfg(all(feature = "vector-db", not(target_arch = "wasm32")))]
pub mod namespaced_memory;
#[cfg(not(target_arch = "wasm32"))]
pub mod personality;
#[cfg(not(target_arch = "wasm32"))]
pub mod provider;
#[cfg(all(feature = "cron", not(target_arch = "wasm32")))]
pub mod scheduler;
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod streaming;
// Swarm and orchestration module (gated sub-modules)
#[cfg(not(target_arch = "wasm32"))]
pub mod evolution;
#[cfg(not(target_arch = "wasm32"))]
pub mod heartbeat;
#[cfg(not(target_arch = "wasm32"))]
pub mod identity;
#[cfg(not(target_arch = "wasm32"))]
pub mod swarm;

#[cfg(not(target_arch = "wasm32"))]
pub use core::{Agent, AgentBuilder, AgentConfig};
#[cfg(not(target_arch = "wasm32"))]
pub use kv_cache::{KvCacheConfig, KvPage, TwoTierKvCache};
#[cfg(all(feature = "vector-db", not(target_arch = "wasm32")))]
pub use namespaced_memory::{MemoryEntry, NamespacedMemory};
pub use session::{AgentSession, SessionStatus};
