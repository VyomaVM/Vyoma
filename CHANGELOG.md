# Changelog

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
