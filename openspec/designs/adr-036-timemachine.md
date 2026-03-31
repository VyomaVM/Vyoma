# ADR-036: TimeMachine - Full Implementation

**Status**: Accepted | Phase 4.1 (v1.7)

## Summary
Implement full TimeMachine functionality - wire the snapshot tree (built in Phase 2) to CLI commands for git-like version control of VMs.

## Context
As part of Phase 4, we need to implement the TimeMachine feature that provides git-for-runtime functionality. This allows users to:
- View snapshot history
- Restore to any previous snapshot
- Set up auto-snapshot policies

## Decision
Implement TimeMachine as per the technical spec:

### CLI Commands
1. `ign history <vm-id>` - View snapshot timeline
2. `ign time-travel <vm-id> --to snap:N` - Restore to snapshot

### Auto-Snapshot Policy
Support automatic snapshots from Ignitefile with configurable interval and retention.

## Implementation

### Location
- `crates/ign/src/commands/snapshot.rs` - CLI commands
- `crates/ignited/src/timemachine.rs` - Core implementation
- `crates/ignited/src/auto_snapshot.rs` - Auto-snapshot task

### SnapshotEntry Structure
```rust
pub struct SnapshotEntry {
    pub id: String,
    pub vm_id: String,
    pub created_at: DateTime<Utc>,
    pub cow_delta_size: u64,
    pub label: Option<String>,
    pub parent_id: Option<String>,
}
```

### Key Functions
- `get_snapshot_history(vm_id)` - Get all snapshots for a VM
- `time_travel(vm_id, target)` - Restore to target snapshot
- `AutoSnapshotTask` - Background task for automatic snapshots

## Consequences
- Git-like version control for VMs
- Easy rollback to previous states
- Automated backup policies
- Timeline visualization
