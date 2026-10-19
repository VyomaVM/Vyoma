# TimeMachine: VM Runtime Versioning

**TimeMachine** brings git-like version control to running Micro-VMs. It enables developers and automated pipelines to create instant checkpoints, view execution history, and roll back runtime state without restarting from scratch.

---

## 1. How It Works

Traditional snapshotting stores full memory and disk dumps, which can be gigabytes per snapshot. TimeMachine uses a **Snapshot Tree** backed by `sled`:

1. **Memory & CPU State**: Pauses the VM and serializes the vCPU register state and memory pages.
2. **CoW Disk Delta**: Forks the active Device Mapper Copy-on-Write layer. Subsequent writes go to a new sparse delta, keeping previous snapshots immutable.
3. **Tree Storage**: Every snapshot records its parent UUID, timestamp, delta size, and user label.

---

## 2. CLI Usage

### Taking a Snapshot

```bash
vyoma snapshot my-server --label "before-database-migration"
```

Output:
```text
✓ Snapshot created: snap_a1b2c3d4 (Delta size: 12.4 MB)
```

### Viewing History

Inspect the full timeline of snapshots:

```bash
vyoma history my-server
```

Output:
```text
TAG     ID                    DELTA       CREATED         LABEL
snap:0  8f3a12bc              4.2 MB      10 minutes ago  initial-boot
snap:1  a1b2c3d4              12.4 MB     3 minutes ago   before-database-migration
snap:2  c9d8e7f6              2.1 MB      Just now        post-deploy
```

### Rolling Back (Time Travel)

Roll back the VM to any point in its history:

```bash
vyoma time-travel my-server --to snap:1
```

The daemon will:
1. Stop the current VM process.
2. Repoint the Device Mapper layer to `snap:1`'s CoW state.
3. Restore vCPU registers and memory pages.
4. Resume VM execution.

**Total time**: <1 second.
