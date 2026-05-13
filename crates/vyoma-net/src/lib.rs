//! vyoma-net - Network layer for Vyoma VM management
//!
//! Provides Rust-native bindings for bridge and TAP device operations.

pub mod error;
pub mod bridge;
pub mod tap;
pub mod wireguard;
pub mod netns;

pub use error::{NetworkError, Result};
pub use bridge::{BridgeManager, BridgeInfo};
pub use tap::{TapManager, TapInfo};
pub use wireguard::{WireGuardNode, WireGuardConfig, PeerConfig, add_route_to_peer_endpoint, add_route_to_subnet, remove_route_to_subnet, get_interface_mtu};
pub use netns::{NetNsManager, create_netns, delete_netns};
