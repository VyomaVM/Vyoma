# Changelog

## [0.6.0] - Upcoming (The Polish Update)
- **Lifecycle**: `ign restart` (Stop/Run replacement), `ign exec` (SSH wrapper).
- **Logging**: `ign logs <service>` support.
- **Inspect**: API for inspecting VM details.

## [0.5.0] - 2026-01-23 (The Scale Edition)
- **Scaling**: `ign scale web=3`.
- **Metadata**: Labels support, robust `ign down`/`ps`.
- **Load Balancing**: Round-Robin DNS.

## [0.3.0] - 2026-01-23

### Major Feature: True Rootless Mode
- **Security**: Complete implementation of Rootless Runtime (`ign run` without sudo).
- **Networking**: Integrated `slirp4netns` for unprivileged user-mode networking.
- **Build**: Implemented `debugfs`-based image population (replacing mount), allowing `ign build` and `ign pull` to run without root.

### Major Feature: Network Management CLI
- **Management**: Added `ign network create`, `ign network ls`, and `ign network rm` commands.
- **Flexibility**: Users can now create isolated CNI bridge networks with custom subnets.
- **API**: Added Daemon endpoints (`/networks`) for managing CNI configurations dynamically.

## [0.2.0-rc1] - 2025-12-27

### Feature: Daemon Robustness
- **State Persistence**: VMs now persist their state across daemon restarts. `ignited` automatically adopts running VMs on startup.
- **Graceful Shutdown**: `ignited` handles SIGINT/SIGTERM to cleanly stop VMs and detach resources (Loop/DM).
- **Zombie Reaping**: Implemented background process monitor to reap zombie Firecracker/VirtioFS processes.
- **Auto-Recovery**: Daemon automatically restarts `virtiofsd` processes if they crash.
- **OOM Monitoring**: Daemon logs Kernel OOM Kill events for monitored VMs.

### Feature: Networking Maturity
- **Internal DNS**: `ignited` now runs an embedded DNS server on the gateway IP (172.16.0.1:53).
- **Service Discovery**: VMs can resolve each other by name (e.g., `my-vm.ignite`).
- **CNI Integration**: Full integration with CNI plugins (`ptp`/`bridge`) for network setup (Hybrid mode).

### Feature: Storage
- **Copy Instruction**: `ign build` now supports `COPY` instruction to inject files into the image.
- **Snapshot/Restore**: Initial implementation of VM Snapshotting (Pause/Snapshot/Resume).

### Documentation
- Added `TROUBLESHOOTING.md`.
- Added `ADR.md` entries for Robustness and DNS.

### Changes
- Updated Kernel Boot Args to configure Hostname and DNS.
