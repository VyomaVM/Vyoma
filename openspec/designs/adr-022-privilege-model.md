# ADR 022: Daemon Privilege Model & Socket Hardening

## Status
Proposed -> Approved

## Context
Currently, the `ignited` daemon runs entirely as `root`. This gives the process over-privileged access to the host ecosystem, violating the principle of least privilege. While managing VMs (creation, networking, storage mapping, cgroups) fundamentally requires root capabilities (e.g. `CAP_SYS_ADMIN`, `CAP_NET_ADMIN`), running the entire Daemon API under blanket `root` exposes the system to catastrophic failure or breakout if a handler exploits a buffer overflow.

## Decision
The daemon will execute as an unprivileged service user (`ignite`).
We will utilize systemd's robust set of capability mappings, granting the process only what it explicitly requires via `AmbientCapabilities=...`.

### Daemon Execution Refactor
- Modify `packaging/systemd/ignited.service` to enforce:
  - `User=ignite`
  - `Group=ignite`
  - `AmbientCapabilities=CAP_SYS_ADMIN CAP_NET_ADMIN ...`
  - `CapabilityBoundingSet=...`

### Unix Socket Ownership
The daemon listens on `/var/run/ignite.sock`. 
- By default, `/var/run` is writable only by `root`. We must update our code so the systemd service or code explicitly allows `ignite` to place a socket there, or have systemd manage the socket, or simply shift the socket directory to `/opt/ignite/run/`.
- **Selected approach**: Bind socket locally where the user (`ignite`) has runtime rights, e.g. `/run/ignite/ignite.sock`. 
- Post-install scripts (`.deb`, `.rpm`) will:
  - Create the `ignite` group.
  - Create the `ignite` user.
  - Instruct the installer to add themselves (e.g. `$SUDO_USER`) to the `ignite` group to bypass sudo for CLI usage.

## Consequences
- Requires `ign` daemon source code to tolerate paths like `~/.ignite` mapping to the explicit Unix user rather than mapping strictly to `$HOME`.
- Reduces blast radius inside the network handling router of Axum.
