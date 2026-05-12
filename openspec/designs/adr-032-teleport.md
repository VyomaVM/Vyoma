# ADR-032: Teleport - Live VM Migration

**Status**: Accepted | Phase 3.3 (v1.5)

## Summary
Implement pre-copy live VM migration (Teleport) for the Vyoma Swarm, enabling VMs to migrate between nodes with minimal downtime.

## Context
As part of Phase 3, we need to support live VM migration in the Vyoma Swarm. This enables:
- Load balancing across nodes
- Node maintenance without VM downtime
- High availability deployment

## Decision
Implement pre-copy memory migration using the protocol defined in the technical spec:

1. **Dirty Page Tracking**: Use KVM_GET_DIRTY_LOG ioctl to track modified memory pages
2. **Iterative Transfer**: Initial bulk copy followed by incremental dirty page transfers
3. **Pause & Finalize**: Pause VM, transfer final delta and CPU state
4. **Resume**: Destination resumes VM, update routing

## Implementation

### Crate Structure
Create `crates/vyoma-teleport/` with:
- `sender.rs` - Source node migration logic
- `receiver.rs` - Destination node receive logic
- `protocol.rs` - Wire protocol for memory transfer

### Key Components

#### MigrationSender
```rust
pub struct MigrationSender {
    vm_fd: VmFd,
    fc_client: FirecrackerClient,
    wg_stream: TcpStream,
}
```

#### Migration Phases
1. **Phase 1**: Enable dirty tracking with KVM
2. **Phase 2**: Initial bulk transfer of all pages
3. **Phase 3**: Iterative dirty page transfer until threshold
4. **Phase 4**: Pause VM, transfer final delta + snapshot
5. **Phase 5**: Signal destination to resume, update routing

## Consequences
- Enables live migration in Swarm deployments
- Requires WireGuard for encrypted migration traffic
- Depends on KVM for dirty page tracking
