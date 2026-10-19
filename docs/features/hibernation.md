# VM Hibernation

**Hibernation** serializes the execution state of an active Micro-VM to disk and completely tears down the running hypervisor process, freeing 100% of the host vCPU and RAM allocations.

---

## 1. Hibernation vs Stopping

| Operation | Memory / vCPU | Disk State | IP Address | Boot Latency |
|-----------|---------------|------------|------------|--------------|
| **Stop** (`vyoma stop`) | Released | Saved to CoW | Released to IPAM | Cold boot (~50ms) |
| **Pause** (`vyoma pause`) | Retained in Host RAM | Frozen | Retained | Instant (<5ms) |
| **Hibernate** (`vyoma hibernate`) | **Released** | **Saved to Disk** | **Retained** | Warm resume (~80ms) |

---

## 2. Process Lifecycle

1. **State Snapshot**: Cloud Hypervisor pauses the VM and writes RAM pages to `~/.vyoma/hibernation/<vm_id>/vm.mem` and registers to `vm.snap`.
2. **Process Termination**: The hypervisor child process is cleanly terminated, returning memory to the Linux kernel.
3. **Network Hold**: The TAP device is administratively brought down (`ip link set <tap> down`), but its IP lease in the IPAM allocator remains reserved.
4. **WAL Update**: The VM status is committed to the Sled Write-Ahead Log as `VmStatus::Hibernated`.
5. **Resume**: When resumed (`vyoma resume <vm_id>`), a new Cloud Hypervisor instance is spawned directly from the snapshot files, bringing the TAP device back up without losing network connections.
