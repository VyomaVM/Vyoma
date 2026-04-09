# Ignite E2E Tests

These tests validate the full functionality of the Ignite ecosystem.

## Prerequisities
*   Root privileges (sudo).
*   Built release binaries (`cargo build --release`).
*   Internet connection (for pulling images).

## Running Tests
Run individual tests or the full suite:

```bash
# Run all
sudo ./tests/e2e/run_all.sh

# Run specific
sudo ./tests/e2e/01_lifecycle.sh
```

## Coverage
*   01 Lifecycle: Image Pull, Run, Ps, Logs, Stop, Pause, Resume, Restart.
*   02 Volumes & Ports: Mounts (-v) and Bindings (-p).
*   03 Builder: Ignitefile build process.
*   04 Compose: Up, Down, Scale.
*   05 Swarm: Init, Join, Ls (Multi-node).
*   06 Network: Create, Ls, Rm (CNI).
*   07 Snapshot: Snapshot, Export, Import (Teleportation).

## Phase 2: Expanded Matrices

### Chaos Engineering (`tests/chaos/`)
Simulates intense physical environment tearing under active Ignite VM loads.
*   `chaos_net.sh`: External processes deleting active VM `rtnetlink` structures.
*   `chaos_storage.sh`: Hardware suspension of Devicemapper loops locally via `dmsetup suspend`.

### Compatibility Matrix (`tests/compatibility/`)
Automated mapping verifying the rootfs unpacker against complex OCI registries.
*   `run_matrix.sh`: Checks regressions against `alpine`, `ubuntu`, `python-slim`, `node`, `nginx`.
