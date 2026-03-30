# ADR-028: Snapshot Tree - Replace Git-Based Time Travel

## Status
Accepted | Phase 2 (v1.3)

## Context
Currently (v1.2), time travel and snapshot features use `git` as the backing store:
- Snapshots are stored as git commits
- History is git log
- Branching is git branch
- Diff is git diff

This approach has fundamental problems:
- Git is designed for source code, not binary VM images
- No delta storage - every snapshot is a full copy
- Git checkout semantics are wrong for VM state
- External git binary dependency
- Not atomic

## Decision
Replace git-based time travel with a proper snapshot tree backed by sled.

### Data Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotNode {
    pub id: String,              // UUID
    pub vm_id: String,            // Parent VM ID
    pub parent_id: Option<String>, // None = root snapshot
    pub created_at: u64,          // Unix timestamp
    pub label: Option<String>,    // User-defined e.g. "pre-deploy"
    pub tag: Option<String>,      // e.g. "snap:6"
    pub memory_path: PathBuf,    // Firecracker .mem file
    pub snapshot_path: PathBuf,  // Firecracker .snap file
    pub cow_delta_path: PathBuf,  // COW diff since parent
    pub cow_delta_size: u64,       // Bytes
    pub memory_size: u64,
}
```

### Storage Structure
```
~/.ignite/snapshots/
├── snapshots.db  (sled database)
└── <vm-id>/
    ├── <snap-id>.mem   (memory state)
    ├── <snap-id>.snap  (disk state)
    └── <snap-id>.cow   (COW delta)
```

### API Design

```rust
pub struct SnapshotTree {
    db: sled::Tree,
    base_path: PathBuf,
}

impl SnapshotTree {
    /// Create a new snapshot
    pub fn create(&self, node: &SnapshotNode) -> Result<()>;
    
    /// Get a snapshot by ID
    pub fn get(&self, id: &str) -> Result<SnapshotNode>;
    
    /// List all snapshots for a VM
    pub fn history(&self, vm_id: &str) -> Result<Vec<SnapshotNode>>;
    
    /// Fork a new VM from a snapshot (like git checkout -b)
    pub fn branch(&self, snap_id: &str, new_vm_id: &str) -> Result<SnapshotNode>;
    
    /// Diff between two snapshots
    pub fn diff(&self, snap_a: &str, snap_b: &str) -> Result<SnapshotDiff>;
    
    /// Tag a snapshot
    pub fn tag(&self, snap_id: &str, tag: &str) -> Result<()>;
    
    /// Get snapshot by tag
    pub fn get_by_tag(&self, vm_id: &str, tag: &str) -> Result<Option<SnapshotNode>>;
    
    /// Delete a snapshot
    pub fn delete(&self, id: &str) -> Result<()>;
}
```

## Consequences
**Positive:**
- Proper delta storage (only changes since parent)
- Atomic operations
- No external git dependency
- Designed for binary VM images
- Queryable metadata and tags

**Negative:**
- More complex implementation
- Migration path from git-based snapshots

## Migration Strategy
1. Add SnapshotTree to ignite-storage crate
2. When taking first snapshot after upgrade, convert existing git commits
3. Keep git as optional export (for compatibility)
