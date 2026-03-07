#[cfg(feature = "swarm-p2p")]
pub mod wormhole;

#[cfg(feature = "swarm-p2p")]
pub use wormhole::{VesselExchange, WormholeConfig};
