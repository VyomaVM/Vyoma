# ADR-023: Write-Ahead Log (WAL) & Crash Recovery using Sled

## Status
Accepted | Phase 1.4 (v1.2)

## Context
Currently (v1.1), daemon restart loses running VM control handles. While JSON state files provide partial persistence, they have no crash-safe write path — a crash during state update leaves corrupted/inconsistent state. This blocks production adoption.

## Decision
We will implement a crash-safe write-ahead log using the sled embedded database, combined with a recovery mechanism to re-adopt VMs on daemon restart.

### Data Layout
- **Database**: `~/.vyoma/state/vyoma.db` (sled)
- **Tree "wal"**: Append-only write-ahead log
  - Key: `{timestamp_nanos}:{uuid}` (for ordering)
  - Value: JSON-encoded WAL entry
- **Tree "vm_state"**: Current VM states
  - Key: VM ID
  - Value: JSON-encoded VmState

### Entry Types
```rust
enum WalEntry {
    VmCreate { id: String, timestamp: u64 },
    VmStart { id: String, timestamp: u64 },
    VmStop { id: String, timestamp: u64 },
    VmDestroy { id: String, timestamp: u64 },
    VmCheckpoint { id: String, snapshot_path: String, timestamp: u64 },
}
```

### Flush Strategy
- **fsync()** after every WAL append (critical for durability)
- Batch commits where possible for performance

### Recovery Algorithm
On startup:
1. Open sled database
2. Scan `~/.vyoma/vms/` directories for `state.json` files
3. For each VM directory:
   - Check if VM process is still running (by PID file or socket)
   - If running but not in WAL → mark as recovered
   - If not running but was running → mark as crashed
4. Re-register recovered VMs in AppState
5. Emit recovery event to subscribers

## Consequences
**Positive:**
- Crash-safe state updates (WAL)
- Automatic VM recovery on daemon restart
- No more dangling resources after crashes
- Queryable history of VM lifecycle events

**Negative:**
- Slight overhead on every state change (~1-2ms)
- Additional disk space for WAL
- Recovery logic complexity

## Implementation Notes
- Use `sled::Config` with `mode(Open)` and proper path
- Configure adequate cache size (e.g., 1GB)
- Consider compaction strategy for long-running deployments
- WAL entries are append-only; no updates/deletes

## Diagram
```
┌─────────────────────────────────────────────────────────────┐
│                      Vyoma Daemon                          │
├─────────────────────────────────────────────────────────────┤
│  Startup                                                    │
│    │                                                        │
│    ▼                                                        │
│  ┌──────────────────┐    ┌─────────────────┐               │
│  │ Recovery Module │───▶│ Re-adopt VMs    │               │
│  └──────────────────┘    └─────────────────┘               │
│           │                                                  │
│           ▼                                                  │
│  ┌──────────────────────────────────────────┐              │
│  │              Sled Database                │              │
│  │  ┌─────────────┐  ┌───────────────────┐  │              │
│  │  │ Tree: wal  │  │ Tree: vm_state    │  │              │
│  │  │ (append)   │  │ (latest state)    │  │              │
│  │  └─────────────┘  └───────────────────┘  │              │
│  └──────────────────────────────────────────┘              │
│           │                                                  │
│           ▼                                                  │
│  Normal Operation                                            │
│    │                                                        │
│    ▼                                                        │
│  ┌──────────────────────────────────────────┐              │
│  │  Handlers (run_vm, stop_vm, etc.)        │              │
│  │    │                                     │              │
│  │    ▼                                     │              │
│  │  WAL.append() ──▶ fsync()                │              │
│  │    │                                     │              │
│  │    ▼                                     │              │
│  │  Update vm_state tree                    │              │
│  └──────────────────────────────────────────┘              │
└─────────────────────────────────────────────────────────────┘
```
