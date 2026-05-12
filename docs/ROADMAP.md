# Vyoma Roadmap 🚀

This document outlines the development path for Vyoma, tracking completed milestones and future goals.

## ✅ Completed Milestones

### v0.1.0: The Foundation ("Hello World")
- **Core Engine**: OCI Pull, Layer Flattening, Firecracker VMM integration.
- **Storage**: Device Mapper snapshots (Instant Clones).
- **Basic CLI**: `vyoma run`, `vyoma ps`, `vyoma stop`.

### v0.2.0: The Developer Experience ("Localhost Gap")
- **Volume Mounts**: VirtioFS support (`-v /host:/vm`) for hot-reloading code.
- **Port Mapping**: Userspace TCP proxy (`-p 8080:80`).
- **Telemetry**: Log streaming (`vyoma logs -f`) and OOM Monitoring.
- **Building**: `Vyomafile` support (`vyoma build`) and `vyoma import/export` (Teleportation).

### v0.3.0: The Maturity Update (Current)
- **Rootless Mode**: Running `vyoma` without `sudo` (via `slirp4netns` and `debugfs`).
- **Networking APIs**: `vyoma network create/ls` for managing CNI bridges.
- **Reliability**: Daemon recovery, graceful shutdown, and robust error handling.

---



### v0.4.0: The Composer Edition (Completed)
- **Private Registry Auth**: Support for authenticated pulls (`.vyoma/.docker/config.json`).
- **Vyoma Compose**: `vyoma up/down` with dependency resolution (`depends_on`).
- **Service Discovery**: Hostname-based resolution (e.g., `ping web`) via internal DNS.

---

### v0.5.0: The Scale Edition (Completed)
- **Robust Metadata**: Labels support, stateless `vyoma down` and `vyoma up`.
- **Horizontal Scaling**: `vyoma scale web=3`.
- **Load Balancing**: Round-Robin DNS for multiple instances.

---

### v0.6.0: The Polish Update (Completed)
- **Lifecycle Management**: `vyoma restart` implemented.
- **Enhanced Logging**: `vyoma logs <service>` implemented.
- **Inspect API**: `/vms/:id` endpoint implemented.

---

## 🚧 Upcoming Roadmap

### v0.7.0: The Cluster Edition ("Vyoma Swarm") - [ALPHA RELEASED]
**Focus**: Multi-host networking and node orchestration.

#### 1. Overlay Networking
- **Goal**: Seamless L3 connectivity between VMs on different hosts.
- **Status**: CLI/API Config generation implemented. Data Plane (Flanneld integration) is **Pending**.

#### 2. Basic Federation
- **Goal**: Schedule VMs across multiple nodes.
- **Status**: Swarm Init/Join Skeleton implemented. Gossip/Sync logic is **Pending**.

---

### v0.8.0: The Desktop Experience
**Focus**: User Interface and Distribution.
- **Web UI**: Embedded dashboard for managing VMs.
- **Installers**: Native installers (MSI, DMG, Deb) bundling CLI+Daemon.

### v0.9.0: Pre-Release Polish
**Focus**: Stability and Security.

## v1.0.0: The Stable Release
**Focus**: Security auditing, performance optimization, and extensive documentation.

- [ ] **Seccomp Hardening**: Strict syscall filtering.
- [ ] **Signed Images**: Cosign integration.

---

## 🚀 Future Ecosystem (Post-v1.0)

### 1. Workload Expansion
- **Kubernetes**: Implement CRI Shim (`containerd` integration) to run Pods as VMs.
- **AI/LLMs**: Specialized support for running AI models (GPU passthrough, MCP Servers).
