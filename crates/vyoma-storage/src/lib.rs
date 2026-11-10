//! vyoma-storage - Storage layer for Vyoma VM management
//!
//! Provides native QCOW2 backing-file storage, ext4 provisioning, and snapshot tree tracking.

pub mod error;
pub mod ext4;
pub mod snapshot_tree;
pub mod manager;
pub mod qcow2;

pub use error::{StorageError, Result};
pub use ext4::Ext4Manager;
pub use snapshot_tree::{SnapshotTree, SnapshotNode, SnapshotDiff, DiffEntry};
pub use manager::StorageManager;
pub use qcow2::{Qcow2Manager, Qcow2Info};

