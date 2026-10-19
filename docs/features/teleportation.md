# VM Teleportation (Live Migration)

**Teleportation** allows running Micro-VMs to migrate between physical hosts with near-zero downtime.

---

## 1. Architecture

Teleportation is implemented in the [`crates/vyoma-teleport`](file:///home/subesh/Startup_Projects/Vyoma/vyoma/crates/vyoma-teleport) crate. It uses a **Pre-Copy Live Migration** protocol:

```
Source Host (Node 1)                                   Destination Host (Node 2)
┌──────────────────────┐                               ┌──────────────────────┐
│  Active Micro-VM     │                               │  Standby Container   │
│  - Running RAM       │                               │  - Base Image Cached │
└──────────┬───────────┘                               └──────────▲───────────┘
           │                                                      │
           │  Round 1: Stream All RAM Pages                       │
           ├─────────────────────────────────────────────────────►│
           │                                                      │
           │  Round 2+: Iterative Dirty Page Tracking             │
           ├─────────────────────────────────────────────────────►│
           │                                                      │
           │  Final Step: Pause (<50ms), Sync Final Dirty Pages   │
           │              and Disk CoW Delta                      │
           └─────────────────────────────────────────────────────►│
                                                                  │
                                                       Resume Execution
```

---

## 2. Process Flow

1. **Base Image Verification**: The destination node verifies that the target base image is already pulled locally.
2. **Phase 1 (Pre-Copy Memory)**: Memory pages are streamed to the destination while the VM remains running. Dirty page tracking flags blocks modified during transfer.
3. **Phase 2 (Stop-and-Copy)**: Once the dirty rate drops below the network threshold, the source VM is briefly paused (<50ms).
4. **Final Sync**: The remaining dirty memory pages and disk CoW delta are transferred.
5. **Switchover**: The destination node resumes the VM with the exact same CPU registers, memory state, and IP address.
