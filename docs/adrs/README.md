# Architecture Decision Records (ADRs)

This directory tracks the architectural decisions made during the design, development, and evolution of Vyoma.

## ADR Index

| ADR | Title | Status | Date | Target / Area |
|-----|-------|--------|------|---------------|
| [001](adr-001-to-018-foundations.md#001-project-naming--branding) | Project Naming & Branding | Accepted | 2025-12-23 | Ecosystem identity (`vyoma` / `vyomad`) |
| [002](adr-001-to-018-foundations.md#002-cli-wrapper-strategy-for-system-operations) | CLI Wrapper Strategy for System Operations | Accepted | 2025-12-23 | Host utilities (`mkfs`, `losetup`, `dmsetup`) |
| [003](adr-001-to-018-foundations.md#003-oci-image-handling) | Custom OCI Image Handling with Reqwest | Accepted | 2025-12-23 | Image pull & layer extraction |
| [004](adr-001-to-018-foundations.md#004-database--state-management) | Initial State Management Strategy | Superseded | 2025-12-23 | JSON state files (superseded by Sled WAL) |
| [005](adr-001-to-018-foundations.md#005-networking-strategy) | Linux Bridge + TAP + NAT Masquerade | Accepted | 2025-12-23 | Host-to-VM & internet networking |
| [006](adr-001-to-018-foundations.md#006-storage-stategy-device-mapper) | Storage Strategy: Device Mapper Snapshots | Accepted | 2025-12-23 | Sub-millisecond instant clones |
| [007](adr-001-to-018-foundations.md#007-verification-strategy--known-gaps) | Verification Strategy & Test Isolation | Accepted | 2025-12-23 | Privileged integration testing |
| [008](adr-001-to-018-foundations.md#008-daemon-state-management) | In-Memory VMM Process Lifecycle Tracking | Superseded | 2025-12-24 | Memory map (superseded by ADR-023) |
| [009](adr-001-to-018-foundations.md#009-port-mapping-strategy-phase-8) | Userspace TCP Proxying for Port Mapping | Accepted | 2025-12-24 | `-p host:vm` port forwarding |
| [010](adr-001-to-018-foundations.md#010-log-streaming-strategy) | Log Streaming via Tokio Broadcast + SSE | Accepted | 2025-12-24 | VM console log streaming |
| [011](adr-001-to-018-foundations.md#011-volume-mount-strategy-virtiofs) | VirtioFS Volume Sharing | Accepted | 2025-12-24 | Host directory sharing via `virtiofsd` |
| [012](adr-001-to-018-foundations.md#012-builder-strategy-vyomafile) | Vyomafile Image Builder Architecture | Accepted | 2025-12-24 | `vyoma build` pipeline |
| [013](adr-001-to-018-foundations.md#013-resource-limits-strategy-cgroups-v2) | Cgroups v2 Resource Limiting | Accepted | 2025-12-24 | CPU and memory constraints |
| [014](adr-001-to-018-foundations.md#014-rootless-strategy-future) | Rootless Mode Exploratory Strategy | Accepted | 2025-12-25 | `slirp4netns` unprivileged runtime |
| [015](adr-001-to-018-foundations.md#015-monitor-vm-health-and-oom-events) | VM Health & Cgroup OOM Event Monitoring | Accepted | 2025-12-27 | Background process & OOM reaper |
| [016a](adr-001-to-018-foundations.md#016-internal-dns-for-service-discovery) | Embedded UDP DNS for Service Discovery | Accepted | 2025-12-27 | `.vyoma` TLD resolver on gateway IP |
| [016b](adr-016-cluster-architecture.md) | Cluster Architecture (Vyoma Swarm) | Accepted | 2026-01-24 | Multi-node cluster design |
| [017a](adr-001-to-018-foundations.md#017-rootless-architecture-strategy) | Rootless Architecture Strategy | Accepted | 2025-12-27 | Hybrid user-mode network & debugfs |
| [017b](adr-017-overlay-networking.md) | Swarm Overlay Networking Strategy | Accepted | 2026-01-24 | VXLAN overlay routing |
| [018a](adr-001-to-018-foundations.md#018-vyoma-compose-strategy) | Vyoma Compose Orchestrator | Accepted | 2026-01-23 | Multi-VM stack orchestration |
| [018b](adr-018-future-ecosystem-strategy.md) | Future Ecosystem Strategy | Accepted | 2026-01-24 | Kubernetes CRI, AI workloads, Web UI |
| [019](adr-019-privileged-service-model.md) | Privileged Service Model (The Docker Model) | Accepted | 2026-01-24 | Background service + unprivileged CLI |
| [020a](adr-020a-oci-image-config.md) | OCI Image Configuration Injection | Accepted | 2026-02-15 | `CMD`, `ENTRYPOINT`, `ENV` injection |
| [020b](adr-020b-initramfs-agent-injection.md) | Initramfs-Based Agent Injection | Accepted | 2026-05-10 | Fast CPIO boot injection vs mounts |
| [021](adr-021-virtiofsd-bundling.md) | Virtiofsd Bundling in Packages | Accepted | 2026-02-18 | Out-of-the-box volume mounting |
| [022](adr-022-privilege-model.md) | Daemon Privilege Model & Socket Hardening | Accepted | 2026-02-20 | Dedicated `vyoma` user & capabilities |
| [023](adr-023-wal-sled.md) | Write-Ahead Log (WAL) & Crash Recovery | Accepted | 2026-02-22 | Sled embedded database durability |
| [024](adr-024-compose-schema-v3.md) | Docker Compose v3 Schema Compatibility | Accepted | 2026-02-25 | Standard v3 compose interoperability |
| [025](adr-025-resource-teardown.md) | Complete Resource Teardown on VM Removal | Accepted | 2026-02-28 | Leak-free loop/DM/TAP cleanup |
| [026](adr-026-ignite-storage.md) | Storage Layer Modularization (`vyoma-storage`) | Accepted | 2026-03-02 | Independent storage engine crate |
| [027](adr-027-ignite-net.md) | Network Layer Modularization (`vyoma-net`) | Accepted | 2026-03-05 | Independent networking engine crate |
| [028](adr-028-snapshot-tree.md) | Snapshot Tree Engine | Accepted | 2026-03-08 | Delta-based snapshot tree replacing Git |
| [029](adr-029-wsl2-kvm-fixes.md) | WSL2 Nested KVM Compatibility | Accepted | 2026-03-12 | Nested virtualization support |
| [030](adr-030-wireguard.md) | WireGuard Mesh Overlay Encryption | Accepted | 2026-03-15 | Encrypted inter-node cluster traffic |
| [031](adr-031-raft.md) | Raft Consensus for Swarm State Management | Accepted | 2026-03-18 | Openraft decentralized cluster state |
| [032](adr-032-teleport.md) | VM Teleportation (Pre-Copy Live Migration) | Accepted | 2026-03-22 | Live memory + storage state migration |
| [033](adr-033-grpc.md) | gRPC Control Interface (`vyoma-proto`) | Accepted | 2026-03-25 | High-performance gRPC & CRI bridge |
| [034](adr-034-prometheus.md) | Prometheus Metrics Exporter | Accepted | 2026-03-28 | `/metrics` telemetry endpoint |
| [035](adr-035-vmif.md) | VMIF (VM Image Format) Specification | Accepted | 2026-04-01 | OCI-compatible compressed squashfs image |
| [036](adr-036-timemachine.md) | TimeMachine CLI & Rollback Workflow | Accepted | 2026-04-05 | Instant VM checkpointing and branching |
| [037](adr-037-hibernation.md) | VM State Hibernation | Accepted | 2026-04-10 | State-to-disk with complete resource release |
| [038](adr-038-vk8s.md) | vk8s: Kubernetes CRI Plugin | Accepted | 2026-04-15 | Running Micro-VMs as Kubernetes Pods |
| [039](adr-039-trustedboot.md) | Trusted Boot & vTPM Attestation | Accepted | 2026-04-20 | Hardware root-of-trust with PCR verification |
| [040](adr-040-agent.md) | In-VM Agent Injection via Initramfs | Accepted | 2026-04-25 | CPIO archive delivery to guest VM |
| [041](adr-041-sdk.md) | Vyoma Client SDK (`vyoma-sdk`) | Accepted | 2026-05-01 | Rust client SDK for developer automation |
| [042](adr-042-studio-v2.md) | Vyoma Studio v2 Web Dashboard | Accepted | 2026-05-05 | Monaco editor, topology graph, timeline UI |
| [043](adr-043-in-vm-agent.md) | In-VM Agent Implementation (`vyoma-agent-vm`) | Accepted | 2026-05-15 | Guest introspection over vsock/TCP |
| [044](adr-044-connection-architecture.md) | Client-Daemon Connection Architecture | Accepted | 2026-05-20 | Unix socket fallback and HTTP endpoints |
| [045](adr-045-rootless-vsock-port-forwarding.md) | Rootless MicroVM Networking via Vsock L4 Stream Forwarding | Accepted | 2026-10-25 | 100% rootless port forwarding with zero sudo |

