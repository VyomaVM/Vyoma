# ADR-031: Raft Consensus for Swarm State Management

## Status
Accepted | Phase 3.2 (v1.4)

## Context
Currently, Swarm uses a seed-based model where a single node holds the "truth". This is fragile - if the seed goes down, the cluster loses state. We need consensus to ensure state is replicated.

## Decision
Replace seed-based approach with Raft consensus using `openraft` crate.

### Dependencies
```toml
# crates/ignited/Cargo.toml
openraft = { version = "0.10", features = ["serde"] }
```

### Data Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmCommand {
    RegisterNode { node_id: u64, addr: String, public_key: String },
    DeregisterNode { node_id: u64 },
    UpdateVmPlacement { vm_id: String, node_id: u64 },
    RemoveVmPlacement { vm_id: String },
    CreateService { name: String, spec: ServiceSpec },
    UpdateService { name: String, spec: ServiceSpec },
    DeleteService { name: String },
}

pub struct IgniteRaft {
    node_id: u64,
    raft: openraft::Raft<IgniteRaft>,
}

impl IgniteRaft {
    pub async fn new(node_id: u64, config: RaftConfig) -> Result<Self>;
    
    /// Bootstrap a single-node cluster (ign swarm init)
    pub async fn bootstrap(&self, addr: String) -> Result<()>;
    
    /// Join existing cluster (ign swarm join)
    pub async fn join(&self, leader_addr: String) -> Result<()>;
    
    /// Submit command to cluster
    pub async fn submit(&self, cmd: SwarmCommand) -> Result<()>;
    
    /// Get cluster metrics
    pub fn metrics(&self) -> RaftMetrics;
}
```

## Consequences
**Positive:**
- Fault-tolerant state management
- Automatic leader election
- State replicated across all nodes

**Negative:**
- Increased latency for state operations
- Requires odd number of nodes for majority

## Testing Strategy
- Unit tests: Command serialization, state machine
- Integration tests: 3-node cluster formation
