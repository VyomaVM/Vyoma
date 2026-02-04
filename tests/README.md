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
*   01 Lifecycle: Image Pull, Run, Ps, Logs, Stop.
*   05 Swarm: Init, Join, Ls (Multi-node).
