# ADR-037: Hibernation - State-to-Disk

**Status**: Accepted | Phase 4.2 (v1.7)

## Summary
Implement VM hibernation - save VM state to disk and release all resources, allowing later resume with preserved state.

## Context
As part of Phase 4, we need to implement hibernation for VMs. This allows:
- Save VM memory and CPU state to disk
- Release all resources (vCPUs, memory, network)
- Resume VM with same IP and state
- Efficient resource utilization

## Decision
Implement hibernation as per the technical spec:

### Hibernation Flow
1. Pause VM and create Firecracker snapshot (CPU + memory)
2. Stop Firecracker process (release vCPUs and memory)
3. Detach TAP device (release network slot)
4. Keep IP reserved for fast resume
5. Update WAL state with hibernation info
6. Remove from in-memory VM map

### Resume Flow
1. Re-enable TAP device
2. Start new Firecracker process
3. Load snapshot
4. Resume execution
5. Update WAL

## Implementation

### Location
- `crates/ignited/src/hibernation.rs`

### Key Structures
```rust
pub struct HibernationInfo {
    pub vm_id: String,
    pub hib_dir: PathBuf,
    pub preserved_ip: IpAddr,
}

pub enum VmStatus {
    Hibernated {
        hib_dir: PathBuf,
        snap_path: PathBuf,
        mem_path: PathBuf,
    },
    // ...
}
```

### Key Functions
- `hibernate_vm(vm_id)` - Save state and release resources
- `resume_vm_from_hibernation(vm_id)` - Restore from hibernation

## Consequences
- Efficient resource management
- Preserve VM state across host reboots
- Fast resume with same IP
- Works with existing snapshot infrastructure
