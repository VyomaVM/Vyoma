//! vyoma-net - Network layer for Ignite VM management
//! 
//! Provides Rust-native bindings for bridge and TAP device operations.

pub mod error;
pub mod bridge;
pub mod tap;
pub mod wireguard;

pub use error::{NetworkError, Result};
pub use bridge::{BridgeManager, BridgeInfo};
pub use tap::{TapManager, TapInfo};
pub use wireguard::{WireGuardNode, WireGuardConfig, PeerConfig};
