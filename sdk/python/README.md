# Vyoma Python SDK

The official Python client library for the **Vyoma MicroVM Runtime** (`vyomad`).

## Installation

```bash
pip install vyoma
```

## Quickstart

```python
from vyoma import VyomaClient

# Connect via default Unix socket (/run/vyoma/vyoma.sock) or HTTP
client = VyomaClient(base_url="http://localhost:8080")

# 1. Run a new MicroVM
vm = client.vms.run(
    image="alpine:latest",
    vcpu=1,
    memory="256M",
    hostname="worker-node-1"
)
print(f"Booted VM: {vm.id} (IP: {vm.ip_address})")

# 2. Create a TimeMachine Snapshot
snap = client.vms.snapshot(vm.id, name="pre-update")
print(f"Created snapshot: {snap.id}")

# 3. Branch Snapshot into New MicroVM
branch_vm = client.vms.branch(vm.id, from_snapshot="pre-update", new_name="worker-exp")
print(f"Branched VM: {branch_vm.id}")

# 4. Scale-to-Zero Hibernation
client.vms.hibernate(vm.id)
print("VM hibernated: 0 vCPUs, 0 MB host memory")

# 5. Fast Resume
client.vms.resume(vm.id)
print("VM resumed in sub-200ms")

# 6. Stop MicroVM
client.vms.stop(vm.id)
```

## Features

- **Zero Third-Party Dependencies**: Built directly on standard Python runtime libraries.
- **Dual Transport**: Supports both Unix domain sockets (`/run/vyoma/vyoma.sock`) and HTTP (`localhost:8080`).
- **Full API Coverage**: Run, Inspect, TimeMachine snapshot, QCOW2 branching, Hibernation, Resume, and Swarm management.
- **Type Annotated**: Fully typed for IDE autocompletion and static analysis.
