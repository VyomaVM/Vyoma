# Ignite Roadmap 🚀

This document outlines the development path for Ignite, tracking completed milestones and future goals.

## ✅ Completed Milestones

### v0.1.0: The Foundation ("Hello World")
- **Core Engine**: OCI Pull, Layer Flattening, Firecracker VMM integration.
- **Storage**: Device Mapper snapshots (Instant Clones).
- **Basic CLI**: `ign run`, `ign ps`, `ign stop`.

### v0.2.0: The Developer Experience ("Localhost Gap")
- **Volume Mounts**: VirtioFS support (`-v /host:/vm`) for hot-reloading code.
- **Port Mapping**: Userspace TCP proxy (`-p 8080:80`).
- **Telemetry**: Log streaming (`ign logs -f`) and OOM Monitoring.
- **Building**: `Ignitefile` support (`ign build`) and `ign import/export` (Teleportation).

### v0.3.0: The Maturity Update (Current)
- **Rootless Mode**: Running `ign` without `sudo` (via `slirp4netns` and `debugfs`).
- **Networking APIs**: `ign network create/ls` for managing CNI bridges.
- **Reliability**: Daemon recovery, graceful shutdown, and robust error handling.

---

## 🚧 Upcoming Roadmap

### v0.4.0: The Composer Edition (Completed)
- **Private Registry Auth**: Support for authenticated pulls (`~/.docker/config.json`).
- **Ignite Compose**: `ign up/down` with dependency resolution (`depends_on`).
- **Service Discovery**: Hostname-based resolution (e.g., `ping web`) via internal DNS.

---

## 🚧 Upcoming Roadmap

### v0.5.0: The Scale Edition ("Ignite Scale")
**Focus**: Robustness, metadata management, and horizontal scaling.

#### 1. Robust Metadata
- **Goal**: Remove dependency on local state files.
- **Strategy**: Store VM tags/labels (`com.ignite.stack=myapp`) in the daemon.

#### 2. Horizontal Scaling
- **Goal**: Run multiple instances of a service.
- **Spec**: `ign scale web=3`.
- **Requirements**: Round-Robin DNS for Load Balancing.

### v0.6.0: The Cluster Edition ("Ignite Swarm")
**Focus**: Multi-host networking and node orchestration.

#### 1. Overlay Networking
- **Goal**: Seamless L3 connectivity between VMs on different hosts.
- **Strategy**: Integrate `flannel` CNI (VXLAN backend).

#### 2. Basic Federation
- **Goal**: Schedule VMs across multiple nodes.
- **Strategy**: Simple round-robin scheduler via CLI remote control.

---

## v1.0.0: The Stable Release
**Focus**: Security auditing, performance optimization, and extensive documentation.

- [ ] **Seccomp Hardening**: Strict syscall filtering.
- [ ] **Signed Images**: Cosign integration.
- [ ] **Web UI**: A simple dashboard for managing VMs.
