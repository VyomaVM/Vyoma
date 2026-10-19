# Vyoma Documentation Portal

Welcome to the documentation for **Vyoma**, a production-grade Micro-VM management ecosystem combining the isolation and speed of Cloud Hypervisor with the developer ergonomics of Docker.

---

## 📚 Documentation Index

### 🚀 [Getting Started](getting-started/)
Guides to help you install, configure, and use Vyoma quickly:
- **[Installation Guide](getting-started/installation.md)**: System prerequisites (KVM, cgroups v2), `.deb`/`.rpm` package installation, systemd service, and group permissions.
- **[Quickstart Tutorial](getting-started/quickstart.md)**: Pulling images, running Micro-VMs, port forwarding, volume mounts, running commands, and inspecting logs.
- **[CLI Reference](getting-started/cli-reference.md)**: Full reference for all 20+ commands (`run`, `ps`, `exec`, `snapshot`, `swarm`, etc.).
- **[Compose Guide](getting-started/compose.md)**: Multi-VM orchestration using `vyoma-compose.yml` (Docker Compose v3 compatible).

---

### 🏛️ [Architecture Deep Dives](architecture/)
Technical internals and design of the core subsystems:
- **[Architecture Overview](architecture/overview.md)**: High-level topology, crate separation, and communication matrix.
- **[Storage Engine](architecture/storage-engine.md)**: OCI-to-Block JIT conversion, Device Mapper instant clones (`dm-snapshot`), sparse CoW deltas, and VirtioFS.
- **[Networking Subsystem](architecture/networking.md)**: Linux bridges, TAP interfaces, IPAM, embedded UDP DNS server, and WireGuard mesh.
- **[Security & Privilege Model](architecture/privilege-model.md)**: Unprivileged `vyoma` service user, Linux Ambient Capabilities, and socket permission enforcement.
- **[Initramfs Agent Injection](architecture/initramfs-injection.md)**: Dynamic in-memory CPIO archive generation replacing slow and race-prone host disk mounts.
- **[Crash Recovery & WAL](architecture/crash-recovery-wal.md)**: Sled-backed Write-Ahead Log, startup recovery sequence, and zombie process cleanup.
- **[Trusted Boot Architecture](architecture/trusted-boot-architecture.md)**: vTPM (`swtpm`), OVMF UEFI Secure Boot, PCR measurements, and remote attestation.

---

### ✨ [Advanced Features](features/)
Enterprise and power capabilities built into Vyoma:
- **[TimeMachine](features/timemachine.md)**: Runtime versioning, snapshot trees, non-linear branching, and instant time-travel rollback.
- **[Vyoma Swarm](features/swarm.md)**: Multi-node clustering powered by Raft consensus (`openraft`), VXLAN overlays, and WireGuard encryption.
- **[Swarm Migration Guide](features/swarm-migration.md)**: Upgrading from legacy seed nodes to decentralized Raft-based clusters.
- **[VM Teleportation](features/teleportation.md)**: Pre-copy live migration transferring dirty memory pages and disk state across physical hosts.
- **[VM Hibernation](features/hibernation.md)**: Freezing VMs to disk, completely releasing host vCPU and RAM, while retaining network leases.
- **[In-VM Agent](features/in-vm-agent.md)**: Musl-compiled static agent (`vyoma-agent-vm`) providing guest introspection and execution over vsock.
- **[vk8s (Kubernetes CRI)](features/vk8s.md)**: Running Kubernetes Pods as isolated Micro-VMs with full CRI compliance.
- **[Trusted Boot & Attestation](features/trusted-boot.md)**: Operational guide for building measured images and verifying TPM quotes.

---

### 🛠️ [Operations & Administration](operations/)
Operator procedures, diagnostics, and deployment:
- **[Troubleshooting Guide](operations/troubleshooting.md)**: Resolving orphaned loop devices, KVM access, socket errors, and using cleanup utilities.
- **[Testing & Quality Assurance](operations/testing-guide.md)**: Unit tests, integration test suites, chaos testing, and Docker Hub compatibility matrices.
- **[Release Process SOP](operations/release-process.md)**: Synchronized multi-component version bump procedure across Cargo, bash packages, and Web UI.

---

### 📐 [Specifications & Roadmap](specifications/)
Technical blueprints and development milestones:
- **[Cloud Hypervisor v54.0 Upgrade Specification](specifications/cloud-hypervisor-v54-upgrade.md)**: Architectural analysis, migration roadmap, and implementation plan for upgrading to Cloud Hypervisor v54.0.
- **[Engineering Specification v2.0](specifications/technical-spec-v2.md)**: Complete 2,400+ line implementation guide spanning Phase 1 to Phase 4.
- **[Project Roadmap](roadmap.md)**: Version milestones tracking completed features from v0.1.0 to upcoming goals.

---

### 📝 [Architecture Decision Records (ADRs)](adrs/)
Explore the [Master ADR Index](adrs/README.md) containing 40+ Architectural Decision Records documenting key technical choices:
- Foundational decisions (ADR 001 - 018)
- Privilege models & Capabilities (ADR 019, 022)
- Initramfs & OCI Injection (ADR 020a, 020b)
- Sled WAL & Durability (ADR 023)
- Compose v3 Schema (ADR 024)
- Snapshot Trees (ADR 028)
- Raft Consensus & Swarm Mesh (ADR 030, 031)
- Live Teleportation (ADR 032)
- VMIF Image Specification (ADR 035)
- Kubernetes CRI vk8s (ADR 038)
- Trusted Boot (ADR 039)
- In-VM Agent & Connection Architecture (ADR 043, 044)

---

### 🗄️ [Historical Archive](archive/)
Preserved documentation from early project phases:
- [Legacy v0.1 Project Summary](archive/legacy-v0.1-summary.md)
- [Early Implementation Scratchpad](archive/legacy-implementation-plan.md)
- [Naming & Branding Research](archive/branding-research.md)
