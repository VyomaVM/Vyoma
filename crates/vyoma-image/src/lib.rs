pub mod vmif;
pub mod hub_bridge;
pub mod signing;
pub mod converter;
pub mod kernel_catalog;
pub mod canonical;
pub mod verity;

pub use vmif::{VmifManifest, VmifImage, OciImageConfig, VmifError, FirmwareInfo, MeasuredBootInfo};
pub use hub_bridge::{HubBridge, HubBridgeError};
pub use signing::{
    SignedManifest, SigningKeyPair, TrustPolicy, SigningError,
    BinarySignature, compute_hash, compute_file_hash,
};
pub use canonical::{to_canonical_json, canonicalize_value, CanonicalJsonError};
pub use verity::{
    generate_verity_from_bytes, generate_verity_from_file, verify_verity_file,
    VeritySpec, VerityTreeResult, VerityError, DEFAULT_VERITY_BLOCK_SIZE,
};
pub use converter::{VmifConverter, ConverterError, SquashfsCompression, VmifMigration, CacheInfo};
pub use kernel_catalog::{KernelCatalog, KernelProfile, WorkloadType};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
