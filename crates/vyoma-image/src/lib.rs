pub mod vmif;
pub mod hub_bridge;
pub mod signing;

pub use vmif::{VmifManifest, VmifImage, OciImageConfig, VmifError, FirmwareInfo, MeasuredBootInfo};
pub use hub_bridge::{HubBridge, HubBridgeError};
pub use signing::{
    SignedManifest, SigningKeyPair, TrustPolicy, SigningError,
    BinarySignature, compute_hash, compute_file_hash,
};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
