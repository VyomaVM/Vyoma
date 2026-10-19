# Architecture Overview

Vyoma is designed around a decoupled client-daemon model optimized for low-latency, hardware-isolated Micro-VM management.

---

## 1. High-Level System Topology

```
┌────────────────────────────────────────────────────────┐
│                      Client Layer                      │
│                                                        │
│   ┌──────────────┐   ┌────────────────┐   ┌────────┐   │
│   │  vyoma CLI   │   │  Web Dashboard │   │  vk8s  │   │
│   │ (Unprivileged│   │  (React/Vite)  │   │  (CRI) │   │
│   └───────┬──────┘   └────────┬───────┘   └───┬────┘   │
└───────────┼───────────────────┼───────────────┼────────┘
            │ Unix Socket       │ HTTP (:3000)  │ gRPC
            ▼                   ▼               ▼
┌────────────────────────────────────────────────────────┐
│                   vyomad Daemon                        │
│             (User: vyoma + Ambient Caps)               │
│                                                        │
│   ┌──────────────┐   ┌────────────────┐   ┌────────┐   │
│   │ REST & gRPC  │   │  Sled WAL      │   │  Raft  │   │
│   │ API Handlers │   │  State Store   │   │ Engine │   │
│   └───────┬──────┘   └────────┬───────┘   └───┬────┘   │
│           │                   │               │        │
│   ┌───────▼───────────────────▼───────────────▼────┐   │
│   │                 VM Lifecycle FSM                │   │
│   └───────────────────────────┬────────────────────┘   │
└───────────────────────────────┼────────────────────────┘
                                │
          ┌─────────────────────┼─────────────────────┐
          ▼                     ▼                     ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│   vyoma-storage  │  │    vyoma-net     │  │    Cloud         │
│                  │  │                  │  │    Hypervisor    │
│  - Device Mapper │  │  - Linux Bridge  │  │                  │
│  - Sparse COW    │  │  - TAP & NAT     │  │  - KVM Engine    │
│  - Snapshot Tree │  │  - Embedded DNS  │  │  - vCPU / Memory │
│  - VirtioFS      │  │  - WireGuard     │  │  - Guest vsock   │
└──────────────────┘  └──────────────────┘  └──────────────────┘
```

---

## 2. Core Components

### 1. `vyomad` (The System Daemon)
The daemon manages process lifecycles, block devices, and network namespaces.
- **Privilege Separation**: Executes as the unprivileged `vyoma` system user rather than root. It holds only the necessary Linux Ambient Capabilities (`CAP_NET_ADMIN`, `CAP_SYS_ADMIN`).
- **Crash Durability**: Uses a Write-Ahead Log (WAL) backed by `sled`. Every lifecycle event is fsynced to disk prior to mutating running state.

### 2. `vyoma-core` & Virtual Machine Monitor (VMM)
- Wraps the Cloud Hypervisor API via local Unix domain sockets.
- Configures kernel boot arguments (`console=ttyS0`, `root=/dev/vda`, static IP injection).
- Prepares and injects the initramfs containing `/sbin/vyoma-init` and `/sbin/vyoma-agent-vm`.

### 3. `vyoma-storage` (Storage Subsystem)
- **Base Images**: Extracted OCI layers formatted into ext4 or SquashFS read-only loop devices.
- **Instant Snapshots**: Uses Linux Device Mapper (`dm-snapshot`) combined with sparse CoW delta files to spawn VMs in milliseconds with near-zero disk usage.
- **VirtioFS**: Spawns managed `virtiofsd` processes for zero-copy host directory sharing.

### 4. `vyoma-net` (Networking Subsystem)
- **L2/L3 Networking**: Sets up Linux bridges (`vyoma0`), TAP interfaces for each VM, and `iptables` NAT masquerade rules.
- **Embedded DNS Server**: Listens on the bridge gateway (`172.16.0.1:53`) to provide automatic name resolution for running VMs and Compose services.
- **Swarm Overlay**: Provides VXLAN tunnels with WireGuard encryption for secure multi-host communication.

---

## 3. Communication Architecture

| Client Interface | Transport | Target Path / Port | Purpose |
|------------------|-----------|--------------------|---------|
| **CLI (`vyoma`)** | Unix Socket | `/run/vyoma/vyoma.sock` | Fast, secure local administrative commands. |
| **Web UI** | HTTP REST | `http://localhost:8080` | Browser dashboard management and metrics. |
| **Kubernetes CRI (`vk8s`)** | gRPC | `localhost:7071` / Unix Socket | Pod sandbox lifecycle management. |
| **In-VM Agent** | vsock / TCP | Vsock Port `9999` | Guest introspection without requiring SSH. |

---

## 4. Enterprise Security Pillars

Vyoma implements **4 Enterprise Security Pillars** providing true defense-in-depth:
1. **Hardware-Enforced MicroVM Isolation**: KVM ring-1 hardware virtualization with dedicated guest kernel and virtio-only devices.
2. **Least-Privilege Host Daemon**: Root privilege dropped to `vyoma` user with minimal ambient capabilities (`CAP_NET_ADMIN`, `CAP_SYS_ADMIN`).
3. **Measured Boot & Cryptographic Attestation**: TPM 2.0 PCR registers and remote attestation quotes.
4. **Isolated Network Mesh & Authenticated Control Plane**: WireGuard / VXLAN encryption and Bearer token API authorization.

See the complete specification in [The 4 Enterprise Security Pillars](file:///home/subesh/Startup_Projects/Vyoma/vyoma/docs/architecture/security-pillars.md).

---

## 5. Storage Branching & Hibernation

- **QCOW2 Branching**: Instant MicroVM branching from snapshots without duplicating backing disks.
- **Scale-to-Zero Hibernation**: Drops host RAM overhead to 0 MB and resumes in `<200ms`.

See [QCOW2 Branching & Scale-to-Zero Hibernation](file:///home/subesh/Startup_Projects/Vyoma/vyoma/docs/architecture/hibernation-and-branching.md).

