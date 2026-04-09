//! ignite-storage - Storage layer for Ignite VM management
//! 
//! Provides Rust-native bindings for device mapper and loop device operations.

pub mod error;
pub mod dm;
pub mod cow;
pub mod ext4;
pub mod snapshot_tree;

pub use error::{StorageError, Result};
pub use dm::{DmManager, DmDevice};
pub use cow::{LoopManager, LoopDevice};
pub use ext4::Ext4Manager;
pub use snapshot_tree::{SnapshotTree, SnapshotNode, SnapshotDiff, DiffEntry};
