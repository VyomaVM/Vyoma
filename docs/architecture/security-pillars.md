# The 4 Enterprise Security Pillars of Vyoma

Vyoma replaces generic "7-layer defense" claims with four concrete, hardware-rooted enterprise security pillars engineered into the microVM lifecycle.

---

## 1. Pillar 1: Hardware-Enforced MicroVM Isolation (KVM + Cloud Hypervisor)

```
┌────────────────────────────────────────────────────────┐
│                      Host Machine                      │
│                                                        │
│  ┌────────────────────────┐  ┌───────────────────────┐ │
│  │     MicroVM #1         │  │     MicroVM #2        │ │
│  │ ┌────────────────────┐ │  │ ┌───────────────────┐ │ │
│  │ │ User Application   │ │  │ │ User Application  │ │ │
│  │ ├────────────────────┤ │  │ ├───────────────────┤ │ │
│  │ │ Linux 6.16.9 Kernel│ │  │ │Linux 6.16.9 Kernel│ │ │
│  │ └─────────┬──────────┘ │  │ └─────────┬─────────┘ │ │
│  └───────────┼────────────┘  └───────────┼───────────┘ │
│              ▼ Hardware Ring-1           ▼             │
│  ════════════════════════════════════════════════════  │
│         KVM Hypervisor (Hardware Virtualization)       │
│  ════════════════════════════════════════════════════  │
│                   Bare-Metal CPU                       │
└────────────────────────────────────────────────────────┘
```

- **Dedicated Guest Kernel Boundary**: Every microVM executes its own isolated Linux micro-kernel (`vmlinux`). Unlike Docker or containerd where all containers share the host Linux kernel and host `/sys` / `/proc` filesystems, Vyoma workloads have an absolute hardware-enforced syscall boundary. Kernel vulnerabilities and container escapes cannot cross into the host or adjacent VMs.
- **Virtio-Only Device Model**: Vyoma builds exclusively on Cloud Hypervisor with a strictly modern virtio device architecture (`virtio-block`, `virtio-net`, `virtio-fs`, `virtio-vsock`). All legacy PC peripheral emulation (no legacy PCI, IDE controllers, BIOS, floppy drives, or programmable interrupt controllers) is completely eliminated, reducing the hypervisor attack surface by over 95% compared to QEMU.

---

## 2. Pillar 2: Least-Privilege Host Daemon (`privdrop.rs`)

The Vyoma host daemon (`vyomad`) implements strict POSIX privilege separation:
- **Unprivileged Execution**: When started with elevated permissions, `vyomad` immediately drops root privileges via `privdrop.rs` to run as the unprivileged `vyoma` service user (`UID/GID 999:999`).
- **Minimal Ambient Capabilities**: It retains only the minimal set of ambient capabilities strictly required for network and storage operations:
  - `CAP_NET_ADMIN`: Required solely for configuring TAP interfaces and the `vyoma0` bridge.
  - `CAP_SYS_ADMIN`: Required strictly for loop device attachments and kernel mount controls.
- **Root-Safe Architecture**: If an attacker were to exploit a flaw in the API layer, they obtain an unprivileged process unable to write to `/etc`, alter host system binaries, or access host secrets.

---

## 3. Pillar 3: Measured Boot & Cryptographic Attestation

```
┌────────────────────────────────────────────────────────┐
│                   Measured Boot Chain                  │
│                                                        │
│  ┌──────────────┐     ┌──────────────┐     ┌─────────┐ │
│  │ Kernel Hash  │ ──► │  Initramfs   │ ──► │ Boot    │ │
│  │    (PCR 0)   │     │    (PCR 1)   │     │ Command │ │
│  └──────────────┘     └──────────────┘     └─────────┘ │
│         │                    │                  │      │
│         ▼                    ▼                  ▼      │
│  ┌───────────────────────────────────────────────────┐ │
│  │          Virtual TPM 2.0 (swtpm daemon)           │ │
│  │              Quote Generation & PCRs              │ │
│  └───────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────┘
```

- **vTPM 2.0 Integration**: Each microVM can be provisioned with an isolated virtual TPM 2.0 device powered by `swtpm`.
- **PCR Measurement**: The microVM kernel image (PCR 0), initramfs guest agent (PCR 1), and kernel boot parameters are cryptographically measured prior to execution.
- **Remote Attestation**: Workloads can be attested on demand via `vyoma attest <vm-id>`, generating a hardware-anchored TPM quote that guarantees the guest environment has not been tampered with or modified.
- **Ed25519 Image Signatures**: Enterprise VMIF images support cryptographic signature verification using Ed25519 keys, rejecting unauthorized or unsigned layers before execution.

---

## 4. Pillar 4: Isolated Network Mesh & Authenticated Control Plane

- **Microsegmentation**: Every microVM is isolated in its own network boundary connected via a private TAP device. Bridge firewall rules prevent unauthorized cross-VM packet sniffing and ARP spoofing.
- **Dual-Mode Cluster Security**:
  - **Local Clusters**: Fast VXLAN overlay networks for low-latency node-to-node routing.
  - **Cross-WAN / Untrusted Links**: Automated WireGuard encrypted tunnels (`vyoma-net`) ensuring zero plain-text packet exposure across public internet or multi-cloud boundaries.
- **Capability-Based API Authorization**: All daemon control plane endpoints (REST and gRPC) enforce Bearer token authentication via `--api-token` or `VYOMA_API_TOKEN`, securing administrative APIs from unauthorized access.
