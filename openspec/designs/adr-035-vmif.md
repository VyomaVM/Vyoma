# ADR-035: VMIF Image Format - ignite-image Crate

**Status**: Accepted | Phase 3.6 (v1.6)

## Summary
Implement VMIF (VM Image Format) - the stable on-disk format for Ignite images with Docker Hub bridge support.

## Context
As part of Phase 3, we need to implement a proper image format for Ignite. Currently images are raw OCI converted at runtime every pull. VMIF provides:
- Stable on-disk format
- OCI-compatible artifact stored in any OCI registry
- Compression with squashfs
- Image signing support

## Decision
Implement VMIF as per the technical spec:

### VMIF Layout
```
ignite.toml  — image metadata
rootfs.sqfs  — squashfs root filesystem (read-only, compressed)
kernel.vmlinuz — guest kernel (optional, uses bundled default if absent)
```

### VmifManifest Structure
```rust
pub struct VmifManifest {
    pub schema_version: u32,        // 1
    pub created: String,            // RFC3339 timestamp
    pub arch: String,               // "amd64", "arm64"
    pub kernel: Option<String>,     // OCI digest of kernel layer
    pub rootfs: String,             // OCI digest of rootfs layer
    pub config: OciImageConfig,     // CMD, ENTRYPOINT, ENV, etc.
    pub labels: HashMap<String, String>,
    pub size_bytes: u64,            // Uncompressed rootfs size
}
```

## Implementation

### Crate Structure
- `crates/ignite-image/src/vmif.rs` - VMIF struct and manifest
- `crates/ignite-image/src/hub_bridge.rs` - Docker Hub → VMIF conversion
- `crates/ignite-image/src/signing.rs` - Image signing (future)

### Hub Bridge
The bridge converts Docker Hub OCI images to VMIF:
1. Pull OCI layers from Docker Hub
2. Unpack layers into staging directory
3. Convert ext4 → squashfs for compression
4. Build ignite.toml metadata
5. Cache result

## Consequences
- Images cached in efficient format
- Read-only root filesystem with compression
- Ready for image signing in Phase 4
- OCI-compatible for registry storage
