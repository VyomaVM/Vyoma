# ADR-027: vyoma-net Crate - Network Layer Refactor

## Status
Accepted | Phase 2 (v1.3)

## Context
Currently (v1.2), the network layer uses subprocess calls to `ip link`, `brctl`, and `iptables` CLI tools. This approach:
- Error handling is string parsing
- Brittle - breaks if CLI output format changes
- No type safety for network operations
- External dependency on specific CLI tools

## Decision
Create a new `vyoma-net` crate with Rust-native bindings to `rtnetlink` crate.

### Crate Structure
```
crates/vyoma-net/
├── Cargo.toml
└── src/
    ├── lib.rs          # Re-exports
    ├── bridge.rs       # Bridge operations
    ├── tap.rs          # TAP device operations
    └── error.rs        # Custom error types
```

### Dependencies
```toml
[dependencies]
rtnetlink = "0.13"           # Async netlink operations
netlink-packet-route = "0.17" # Netlink packet types
ipnetwork = "0.20"           # IP network utilities
thiserror = "1.0"
anyhow = "1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

### API Desvyoma

#### Bridge Manager (bridge.rs)
```rust
pub struct BridgeManager {
    handle: Handle,
}

impl BridgeManager {
    pub async fn new() -> Result<Self>;
    
    /// Create a bridge interface
    pub async fn create_bridge(&self, name: &str) -> Result<u32>;
    
    /// Delete a bridge interface
    pub async fn delete_bridge(&self, name: &str) -> Result<()>;
    
    /// Set bridge up
    pub async fn set_up(&self, name: &str) -> Result<()>;
    
    /// Add a TAP device to bridge
    pub async fn add_tap_to_bridge(&self, tap_name: &str, bridge_name: &str) -> Result<()>;
    
    /// List all bridges
    pub async fn list_bridges(&self) -> Result<Vec<BridgeInfo>>;
}
```

#### TAP Manager (tap.rs)
```rust
pub struct TapManager {
    handle: Handle,
}

impl TapManager {
    pub async fn new() -> Result<Self>;
    
    /// Create a TAP device
    pub async fn create_tap(&self, name: &str) -> Result<String>;
    
    /// Delete a TAP device
    pub async fn delete_tap(&self, name: &str) -> Result<()>;
    
    /// Set TAP up
    pub async fn set_up(&self, name: &str) -> Result<()>;
    
    /// Get TAP interface info
    pub async fn get_info(&self, name: &str) -> Result<TapInfo>;
}
```

## Consequences
**Positive:**
- Type-safe network operations
- Async/await API
- Better error handling
- No external CLI dependency

**Negative:**
- Additional crate to maintain
- Native library dependency (libnl)
- More complex async code

## Migration Strategy
1. Create `vyoma-net` crate alongside existing code
2. Add to workspace Cargo.toml
3. Migrate one function at a time
4. Remove subprocess calls once migration complete
