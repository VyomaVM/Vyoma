pub mod api;
pub mod dns;
pub mod grpc;
pub mod hibernation;
pub mod metrics;
pub mod privdrop;
pub mod state;
pub mod swarm;
pub mod timemachine;
pub mod ui;
pub mod vm_service;

pub use state::{AppState, VmInstance, VmState};

#[cfg(feature = "chaos")]
pub mod chaos;
#[cfg(feature = "chaos")]
pub mod chaos_tests;

pub mod auto_snapshot;