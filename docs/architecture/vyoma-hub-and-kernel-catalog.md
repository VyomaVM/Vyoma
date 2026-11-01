# Vyoma Hub & Workload-Tuned Kernel Catalog Architecture

This document specifies the architecture of **Vyoma Hub**, the **VMIF (Vyoma Machine Image Format)** virtual appliance specification, the **Workload-Tuned Kernel Catalog**, and the **Ed25519 Cryptographic Supply Chain**.

---

## 1. Executive Summary & Paradigm Shift

In standard container engines (Docker, Podman, containerd), images are merely serialized tarballs of userspace filesystems executed within Linux namespaces on top of a **single, shared host kernel**. Every container—from a high-write relational database to a stateless reverse proxy—is constrained to the exact same kernel scheduler, memory management policies, dirty-page writeback ratios, and syscall attack surface.

```
[ Traditional Container Model: One Kernel Fits All ]
┌────────────────────────────────────────────────────────────────────────┐
│ Shared Host Linux Kernel (Global Syscall Table, Shared I/O Queue)      │
├──────────────────────┬──────────────────────┬──────────────────────────┤
│ PostgreSQL Container │ NGINX Proxy Container│ Python Analytics Worker  │
│ (Needs Direct I/O)   │ (Needs XDP / Epoll)  │ (Needs General Compute)  │
└──────────────────────┴──────────────────────┴──────────────────────────┘
```

**Vyoma fundamentally changes this model.** Every workload executes inside an isolated microVM with a **dedicated guest Linux micro-kernel**. 

**Vyoma Hub** is engineered not merely as an OCI tarball repository, but as an **Official App Store for Hardware-Isolated Micro-OS Appliances**:
- Organizations (e.g., PostgreSQL, MongoDB, NGINX, Redis) can ship **dedicated, workload-optimized kernels** paired with their software.
- Developers can **mix and match** application root filesystems with specialized, curated kernels.
- **Cryptographic Ed25519 signing** guarantees an unforgeable supply chain from publisher to host execution.
- Hardware-rooted **Measured Boot & Virtual TPM 2.0** guarantees that only untampered kernels and binaries boot.

```
[ Vyoma Hub Model: Workload-Optimized Micro-OS Appliances ]
┌──────────────────────────┐  ┌──────────────────────────┐  ┌──────────────────────────┐
│ PostgreSQL Appliance     │  │ NGINX Edge Appliance     │  │ Confidential ML Appliance│
├──────────────────────────┤  ├──────────────────────────┤  ├──────────────────────────┤
│ vyoma/kernel-db          │  │ vyoma/kernel-edge        │  │ vyoma/kernel-hardened    │
│ • Direct I/O & Hugepages │  │ • XDP / eBPF bypass      │  │ • SEV-SNP / TDX Lockdown │
│ • Deadline I/O Scheduler │  │ • Zero storage drivers   │  │ • vTPM PCR measurements  │
│ • Zero Swap Allocation   │  │ • Sub-10ms packet loop   │  │ • Memory encryption      │
├──────────────────────────┤  ├──────────────────────────┤  ├──────────────────────────┤
│ KVM Hardware Ring -1     │  │ KVM Hardware Ring -1     │  │ KVM Hardware Ring -1     │
└──────────────────────────┘  └──────────────────────────┘  └──────────────────────────┘
```

---

## 2. VMIF (Vyoma Machine Image Format) Appliance Layout

A Vyoma image is encapsulated as a `.vmif` archive (or an OCI-artifact directory structure). Unlike flat container layer tarballs, a VMIF image is an atomic, bootable appliance:

```
my-database.vmif / (~/.vyoma/images/postgres_16/)
├── vyoma.toml          # Appliance manifest (schema, hardware limits, kernel flags)
├── vyoma.toml.sig      # Ed25519 signature of vyoma.toml (optional in dev, required in prod)
├── rootfs.sqfs         # Read-only, highly compressed Squashfs root filesystem
├── kernel.vmlinuz      # Dedicated guest Linux kernel (optional; resolves to catalog if absent)
└── initrd.img          # Micro-initramfs containing Vyoma guest agent (optional)
```

### 2.1 Manifest Specification (`VmifManifest`)

Defined in [`crates/vyoma-image/src/vmif.rs`](file:///home/subesh/Startup_Projects/Vyoma/vyoma/crates/vyoma-image/src/vmif.rs):

```rust
pub struct VmifManifest {
    pub schema_version: u32,             // 1
    pub created: String,                 // RFC3339 timestamp
    pub arch: String,                    // "amd64", "arm64"
    pub kernel: Option<String>,          // Direct kernel path or sha256 hash
    pub kernel_profile: Option<String>,  // Catalog profile: e.g. "vyoma/kernel-db"
    pub kernel_cmdline: Option<Vec<String>>, // Workload-tuned boot arguments
    pub initrd: Option<String>,          // Optional initramfs reference
    pub rootfs: String,                  // Hash or relative path to rootfs.sqfs
    pub config: OciImageConfig,          // Entrypoint, CMD, ENV, WorkingDir
    pub labels: HashMap<String, String>, // Organization, version, license
    pub size_bytes: u64,                 // Uncompressed filesystem size
    pub firmware: FirmwareInfo,          // UEFI / OVMF / Direct PVH boot
    pub measured_boot: MeasuredBootInfo, // Expected PCR 0, PCR 1, PCR 2 values
    pub confidential_computing: ConfidentialComputingInfo, // SEV-SNP / TDX policies
}
```

---

## 3. The Workload-Tuned Kernel Catalog

Implemented in [`crates/vyoma-image/src/kernel_catalog.rs`](file:///home/subesh/Startup_Projects/Vyoma/vyoma/crates/vyoma-image/src/kernel_catalog.rs), Vyoma provides and certifies distinct kernel profiles optimized for specific computing characteristics.

### 3.1 Kernel Matrix

| Profile Name | Target Workload | Key Kernel Subsystems & Flags | Recommended Hardware |
|---|---|---|---|
| **`vyoma/kernel-db`** | Relational & NoSQL Databases (PostgreSQL, MongoDB, MySQL, ScyllaDB) | `transparent_hugepage=always`, `elevator=deadline`, `noswap`, `io_uring`, zero memory overcommit, direct I/O fastpath | Min 2 vCPU, 512 MB RAM |
| **`vyoma/kernel-edge`** | High-concurrency Web Servers & Proxies (NGINX, Envoy, Traefik, HAProxy) | `xdp`, `ebpf`, `epoll_turbo`, multiqueue virtio-net, all block & USB storage drivers stripped out, sub-10ms packet loop | Min 1 vCPU, 128 MB RAM |
| **`vyoma/kernel-ai`** | Deep Learning & Inference (PyTorch, TensorFlow, vLLM, Ollama) | `hugepagesz=1G`, `iommu=pt`, VFIO PCI direct passthrough, AVX-512 vector acceleration, pinned memory mapping | Min 4 vCPU, 2048 MB RAM |
| **`vyoma/kernel-hardened`** | Zero-Trust & Confidential Computing (Vault, Keycloak, Payment Gateways) | `lockdown=confidentiality`, `init_on_alloc=1`, `slab_nomerge`, AMD SEV-SNP / Intel TDX support, vTPM PCR measurements | Min 1 vCPU, 256 MB RAM |
| **`vyoma/kernel-universal`** | General-purpose microVM execution (Web apps, microservices, CLI tools) | Standard PVH boot, virtio-block, virtio-net, virtio-vsock, console serial | Min 1 vCPU, 256 MB RAM |

### 3.2 Dynamic Resolution & Heuristic Suggestions

When pulling from Docker Hub via the **Hub Bridge** ([`crates/vyoma-image/src/hub_bridge.rs`](file:///home/subesh/Startup_Projects/Vyoma/vyoma/crates/vyoma-image/src/hub_bridge.rs)), Vyoma automatically analyzes the OCI repository name and image configuration:
- Pulling `postgres:16` $\rightarrow$ automatically attaches `vyoma/kernel-db` and database boot parameters.
- Pulling `nginx:alpine` $\rightarrow$ automatically attaches `vyoma/kernel-edge` and network bypass flags.
- Pulling `vllm/vllm-openai` $\rightarrow$ automatically attaches `vyoma/kernel-ai` and large page allocations.

Organizations can override this recommendation or bundle custom, internally compiled kernels directly in their `vyoma.toml`.

---

## 4. Cryptographic Supply Chain & Gatekeeper (Ed25519 Signing)

To prevent supply chain poisoning and malicious tampering, Vyoma enforces a cryptographic trust pipeline similar to mobile app store ecosystems:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Publishing Phase (Publisher / Organization)              │
│    • Generate Ed25519 keypair                               │
│    • Compile rootfs.sqfs + kernel.vmlinuz                   │
│    • Compute SHA-256 digests                                │
│    • Pre-calculate PCR expectations for Measured Boot       │
│    • Svyoma vyoma.toml -> generates vyoma.toml.sig          │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Distribution Phase (Vyoma Hub Registry)                  │
│    • Store signed VMIF appliances                           │
│    • Provide publisher public key and CA certificates       │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. Host Verification Phase (Vyoma Daemon)                   │
│    • Check /etc/vyoma/keys/trusted/ (TrustPolicy)           │
│    • Verify Ed25519 signature of vyoma.toml.sig             │
│    • Verify SHA-256 of rootfs.sqfs matches manifest         │
│    • Verify SHA-256 of kernel.vmlinuz matches manifest      │
│    • FAIL: Abort launch immediately before KVM allocation   │
│    • PASS: Initialize Virtual TPM 2.0 (swtpm)               │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Execution & Attestation Phase (Hardware Boundary)        │
│    • KVM launches Cloud Hypervisor microVM                  │
│    • PCR 0: Kernel hash measured into vTPM                  │
│    • PCR 1: Initramfs agent hash measured into vTPM         │
│    • Remote attestation (vyoma attest) validates identity   │
└─────────────────────────────────────────────────────────────┘
```

### 4.1 Strict Security Enforcement

When `require_signed_manifest = true` is configured in `/etc/vyoma/vyoma.toml`:
1. The daemon checks `~/.vyoma/keys/trusted` or `/etc/vyoma/keys/trusted`.
2. Any image lacking a valid `.sig` file signed by a trusted Ed25519 public key is **rejected before any guest process or hypervisor is initialized**.
3. If an attacker modifies even a single byte of `rootfs.sqfs` or alters a kernel boot parameter, verification fails.

---

## 5. Dual-Mode Operation & Migration Roadmap

### Phase 1: The OCI Compatibility Bridge (Current)
- Support 100% of existing Docker Hub and OCI container images.
- Unpack layers on pull, generate on-the-fly Squashfs, and attach the recommended kernel from the `KernelCatalog`.
- Developers get hardware isolation without learning a new format.

### Phase 2: Native VMIF Distribution on Vyoma Hub (Active Rollout)
- `hub.vyoma.dev` serves pre-built, verified `.vmif` bundles.
- Eliminates on-the-fly Squashfs build latency, delivering instant `<250ms` cold boots directly from cache.
- Official certified appliances for PostgreSQL, NGINX, MongoDB, Redis, and PyTorch.

### Phase 3: The Open Kernel Marketplace
- Community and enterprise organizations can publish certified custom kernels.
- Cloud providers and confidential computing operators can publish specialized kernels with bespoke hardware drivers (CXL memory, NVLink, FPGA accelerators) verified through remote attestation quotes.
