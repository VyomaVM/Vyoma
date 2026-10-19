# Crash Recovery & Write-Ahead Log (WAL)

To ensure high availability and prevent resource leaks across daemon restarts or host crashes, Vyoma employs an embedded Write-Ahead Log backed by `sled`.

---

## 1. The WAL Design

The state database is located at `~/.vyoma/state/vyoma.db` (or `/var/lib/vyoma/state/vyoma.db` in production).

### Database Trees

1. **`wal` Tree (Append-Only Log)**:
   - **Key**: `{timestamp_nanos}:{uuid}` (chronologically sortable).
   - **Value**: JSON-encoded `WalEntry` record.
   - **Durability**: Every write invokes `fsync()` to ensure persistent storage on disk before hypervisor operations take place.

2. **`vm_state` Tree (Current Materialized View)**:
   - **Key**: VM UUID string.
   - **Value**: Latest `VmState` snapshot (status, assigned IP, loop devices, Cgroup path).

### Log Entry Types
```rust
pub enum WalEntry {
    VmCreate { id: String, timestamp: u64 },
    VmStart { id: String, timestamp: u64 },
    VmStop { id: String, timestamp: u64 },
    VmDestroy { id: String, timestamp: u64 },
    VmCheckpoint { id: String, snapshot_path: String, timestamp: u64 },
}
```

---

## 2. Startup Recovery Procedure

When `vyomad` boots up:
1. **WAL Scan**: The daemon opens the `sled` database and replays uncommitted transactions.
2. **Process Adoption**:
   - For every registered VM, the daemon inspects `/proc/<pid>` or queries the Cloud Hypervisor API socket.
   - If the hypervisor process is still running, the daemon re-attaches its monitoring handle.
   - If the process terminated unexpectedly while marked `Running`, the daemon updates its state to `Crashed` and records the event.
3. **Zombie Resource Cleanup**:
   - Detaches unreferenced loopback devices (`/dev/loopX`).
   - Removes orphaned Device Mapper targets (`dmsetup remove`).
   - Releases inactive TAP interfaces back to the IPAM pool.
