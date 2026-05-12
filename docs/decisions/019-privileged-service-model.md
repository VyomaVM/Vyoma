# 019. Privileged Service Model (The Docker Model)

## Context
We initially aimed for "True Rootless" operation to maximize security and ease of use. However, this introduced significant limitations:
1. **Performance**: "Instant Clones" (Device Mapper) require root. Rootless fallback (file copy) is too slow (seconds vs ms).
2. **Networking**: Rootless networking (slirp4netns) prevents standard Overlay/VXLAN implementation for Clustering (Vyoma Swarm).
3. **Stability**: Environment restrictions (e.g., restricted `unshare`) caused crashes in standard user shells.

## Decision
For v1.0, we will adopt the **Privileged Service Model** (aka "The Docker Model"):
- **Daemon (`vyomad`)**: Runs as `root` (managed by systemd). This grants access to KVM, Device Mapper, and Kernel Networking (Bridges/VXLAN).
- **CLI (`ign`)**: Runs as an unprivileged user. It communicates with the daemon via a Unix Socket (`/var/run/vyoma.sock`).

"True Rootless" mode is demoted to **Experimental/Alpha** status and is not a blocker for v1.0.

## Consequences
- **Pros**:
    - **Performance**: Instant Clones are guaranteed.
    - **Simplicity**: Swarm Networking can use standard Linux VXLAN/Bridges.
    - **Robustness**: Eliminates complex user-namespace mapping issues.
- **Cons**:
    - Security: Daemon runs as root (standard for Hypervisors/Container Engines).
    - UX: User must have sudo to install/start the daemon service.
