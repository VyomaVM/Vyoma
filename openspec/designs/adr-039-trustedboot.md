# ADR-039: Trusted Boot Chain - Image Signing and Verification

**Status**: Accepted | Phase 4.4 (v1.8)

## Summary
Implement trusted boot chain with Ed25519 signing for VMIF images to ensure image integrity and authenticity.

## Context
As part of Phase 4, we need to implement image signing and verification. This ensures:
- Image integrity - images haven't been tampered with
- Image authenticity - images come from trusted sources
- Secure boot chain - verify before running

## Decision
Implement signing and verification as per the technical spec:

### Key Components
1. **Signing**: Svyoma VMIF manifest with Ed25519 key
2. **Verification**: Verify signature before booting
3. **Trust Policy**: Configure trusted keys

### Structures
```rust
pub struct SignedManifest {
    pub manifest: VmifManifest,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
}
```

## Implementation

### Location
- `crates/vyoma-image/src/signing.rs`

### Key Functions
- `sign_manifest(manifest, key)` - Svyoma manifest with Ed25519
- `verify_manifest(signed, trusted_key)` - Verify signature

### Configuration
```toml
[security]
require_signed_images = true
trusted_keys = [
    "/etc/vyoma/trusted-keys/ci.pub",
    "/etc/vyoma/trusted-keys/hub.pub",
]
```

## Consequences
- Secure image distribution
- Tamper detection
- Trust chain from CI/CD to production
- Integration with VMIF format
