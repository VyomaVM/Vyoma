# ADR-026: vyoma-storage Crate - Storage Layer Refactor

## Status
Accepted | Phase 2 (v1.3)

## Context
Currently (v1.2), the storage layer uses subprocess calls to `dmsetup` and `losetup` CLI tools. This approach has several problems:
- Error handling is string parsing of stderr output
- Brittle - breaks if CLI output format changes
- No type safety
- External dependency on specific CLI tools

## Decision
Create a new `vyoma-storage` crate with Rust-native bindings to `devicemapper` and `loopdev` crates.

### Crate Structure
```
crates/vyoma-storage/
├── Cargo.toml
└── src/
    ├── lib.rs          # Re-exports
    ├── dm.rs           # Device Mapper operations
    ├── cow.rs          # Copy-on-Write operations
    └── error.rs        # Custom error types
```

### Dependencies
```toml
[dependencies]
devicemapper = "0.34"   # Safe Rust bindings for libdevmapper
loopdev = "0.4"         # Safe Rust bindings for loop devices
thiserror = "1.0"
anyhow = "1.0"
```

### API Desvyoma

#### Device Mapper (dm.rs)
```rust
pub struct DmManager {
    dm: DM,
}

impl DmManager {
    pub fn new() -> Result<Self>;
    
    /// Create a snapshot device
    pub fn create_snapshot(
        &self,
        name: &str,
        base_dev: &Path,
        cow_dev: &Path,
    ) -> Result<DmDevice>;
    
    /// Remove a snapshot device
    pub fn remove_snapshot(&self, name: &str) -> Result<()>;
    
    /// List all devices
    pub fn list_devices(&self) -> Result<Vec<DmDevice>>;
}
```

#### Loop Device (cow.rs)
```rust
pub struct LoopManager {
    control: LoopControl,
}

impl LoopManager {
    pub fn new() -> Result<Self>;
    
    /// Attach a loop device to a file
    pub fn attach(&self, file: &Path) -> Result<LoopDevice>;
    
    /// Detach a loop device
    pub fn detach(&self, device: &LoopDevice) -> Result<()>;
    
    /// Create a sparse COW file
    pub fn create_cow_file(path: &Path, size_mb: u64) -> Result<()>;
}
```

## Consequences
**Positive:**
- Type-safe storage operations
- Better error handling with custom error types
- No external CLI dependency for storage operations
- Easier to test and maintain

**Negative:**
- Additional crate to maintain
- Native library dependencies (libdevmapper, libloop)

## Migration Strategy
1. Create `vyoma-storage` crate alongside existing code
2. Add to workspace Cargo.toml
3. Migrate one function at a time
4. Remove subprocess calls once migration complete
