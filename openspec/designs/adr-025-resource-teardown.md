# ADR-025: Comprehensive Resource Teardown

## Status
Accepted | Phase 1.6 (v1.2)

## Context
When VMs are destroyed or the daemon crashes, proper cleanup of all resources is critical to avoid:
- Zombie processes (virtiofsd, firecracker)
- Orphaned device mapper snapshots
- Stale loop devices
- Leftover TAP interfaces
- Dangling network namespaces

## Decision
The `VmInstance::cleanup()` method in `crates/ignited/src/state.rs` will follow a strict 8-step cleanup process:

### Cleanup Steps (in order)

1. **Kill VMM** - Terminate Firecracker process
2. **Remove Network Interface / CNI** - Delete TAP device and CNI configuration
3. **Remove DM Device** - Clean up device mapper snapshot
4. **Detach Loop Devices** - Release loop device references
5. **Remove COW file** - Delete the copy-on-write differential file
6. **Abort Proxy Tasks** - Stop any port forwarding tasks
7. **Remove Cgroup** - Clean up cgroup controllers
8. **Kill VirtioFs Managers** - Terminate virtiofsd processes (NEW - ADR-025)

### Implementation
```rust
// Step 8: Kill VirtioFs Managers
for fs_mgr in &mut self.fs_managers {
    if let Err(e) = fs_mgr.kill() {
        error!("Failed to kill virtiofsd: {}", e);
    }
}
```

## Consequences
**Positive:**
- No more zombie virtiofsd processes after VM destruction
- Consistent cleanup regardless of how VM was terminated
- Helps with recovery - stale virtiofsd sockets no longer confuse restarts

**Negative:**
- Slight additional latency on VM destroy (minimal)
- Requires VirtioFsManager::kill() to be robust

## Verification
Run `ign rm` on a VM with volume mounts and verify:
1. No stray firecracker process (`pgrep firecracker`)
2. No virtiofsd processes (`pgrep virtiofsd`)
3. No DM devices (`dmsetup ls`)
4. No loop devices (`losetup -a`)
5. No TAP interfaces (`ip link show`)
