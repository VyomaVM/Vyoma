# QCOW2 Branching & Scale-to-Zero Hibernation

Vyoma provides instant branching and scale-to-zero hibernation for Cloud Hypervisor v54.0 microVMs.

---

## 1. QCOW2 Snapshot Branching

Vyoma leverages native QCOW2 copy-on-write backing files for instant branching without duplicating disk images.

### Architecture

```
                  ┌───────────────────────────────┐
                  │       Base OCI Rootfs         │
                  │         (read-only)           │
                  └───────────────┬───────────────┘
                                  │
                                  ▼
                  ┌───────────────────────────────┐
                  │     Source VM Root Disk       │
                  │         (root.qcow2)          │
                  └───────┬───────────────┬───────┘
                          │               │
           Take Snapshot  │               │  Take Snapshot
           (snap-1)       │               │  (snap-2)
                          ▼               ▼
                  ┌───────────────┐ ┌───────────────┐
                  │ Branch VM #1  │ │ Branch VM #2  │
                  │ (root.qcow2)  │ │ (root.qcow2)  │
                  │ backing:      │ │ backing:      │
                  │   snap-1      │ │   snap-2      │
                  └───────────────┘ └───────────────┘
```

1. When a snapshot is created (`vyoma snapshot <id> --name <tag>`), Vyoma records the exact QCOW2 backing chain and CPU/memory state in TimeMachine.
2. When branched (`vyoma branch <id> --from <tag> <new_name>`), Vyoma generates a thin delta overlay (`root.qcow2`) pointing back to the snapshot file as its backing file.
3. The newly branched VM boots independently with its own IP and vCPUs in `<250ms`, while sharing the immutable base layers with zero duplicated disk space.

---

## 2. Scale-to-Zero Hibernation State Machine

```
              ┌───────────────┐
              │    Running    │
              │  (VMM Active) │
              └───┬───────▲───┘
                  │       │
  vyoma hibernate │       │ vyoma resume (<200ms)
                  ▼       │
              ┌───────────┴───┐
              │  Hibernated   │
              │   (0 vCPUs,   │
              │    0 MB RAM)  │
              └───────────────┘
```

### Hibernation Lifecycle (`vyoma hibernate <id>`)
1. **Pause VMM**: Sends pause signal to the Cloud Hypervisor API (`/api/v1/vm.pause`).
2. **State Serialization**: Cloud Hypervisor flushes guest memory pages and vCPU registers to `/var/lib/vyoma/hibernated/<vm_id>/memory.snap`.
3. **Process Termination**: The Cloud Hypervisor process is cleanly terminated, releasing **all** host RAM and vCPU allocations back to the host operating system.
4. **State Tracking**: `vyomad` updates the VM state to `VmStatus::Hibernated`, preserving the assigned IP address, network configuration, and disk overlays.

### Resume Lifecycle (`vyoma resume <id>`)
1. **Process Spawn**: `vyomad` spawns a new Cloud Hypervisor instance with `--restore` pointing to the saved memory snapshot.
2. **Network Reattachment**: The TAP interface is re-bound to the new VMM process.
3. **Execution Resumed**: Guest execution resumes in **<200ms** at the exact CPU instruction where it was hibernated, preserving open sockets and running guest processes.
