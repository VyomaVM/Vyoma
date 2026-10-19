# ADR-030: WireGuard Integration for Encrypted Swarm Communication

## Status
Accepted | Phase 3.1 (v1.4)

## Context
Currently, all Swarm/VXLAN traffic is plaintext. For multi-tenant deployments, we need encrypted communication between nodes. WireGuard provides modern, efficient encryption with minimal overhead.

## Decision
Integrate WireGuard using `boringtun` (pure Rust implementation) in the vyoma-net crate.

### Dependencies
```toml
# crates/vyoma-net/Cargo.toml
boringtun = "0.6"
base64 = "0.22"
```

### API Desvyoma

```rust
// crates/vyoma-net/src/wireguard.rs

pub struct WireGuardNode {
    secret_key: X25519SecretKey,
    public_key: X25519PublicKey,
    handle: Option<DeviceHandle>,
    listen_port: u16,
}

impl WireGuardNode {
    /// Create new WireGuard node with auto-generated keypair
    pub fn new(listen_port: u16) -> Result<Self>;
    
    /// Get base64-encoded public key for sharing
    pub fn public_key_base64(&self) -> String;
    
    /// Add peer for Swarm communication
    pub fn add_peer(&self, public_key_b64: &str, endpoint: SocketAddr, allowed_ips: &[IpNetwork]) -> Result<()>;
    
    /// Remove peer
    pub fn remove_peer(&self, public_key_b64: &str) -> Result<()>;
    
    /// Start listening for WireGuard traffic
    pub fn start(&mut self) -> Result<()>;
    
    /// Stop WireGuard
    pub fn stop(&mut self) -> Result<()>;
}
```

### Integration with Swarm
- `vyoma swarm init`: Generate keypair, store in `/var/lib/vyoma/wg.key`, listen on UDP 51820
- `vyoma swarm join`: Exchange public keys via HTTP, add as peers

## Consequences
**Positive:**
- Encrypted node-to-node communication
- Modern cryptography (Noise protocol)
- Lower CPU overhead than TLS

**Negative:**
- Additional dependency
- Requires UDP port 51820

## Testing Strategy
- Unit tests: Key generation, peer add/remove
- Integration tests: Full WireGuard tunnel setup
