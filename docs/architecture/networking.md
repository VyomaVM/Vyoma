# Networking Subsystem Architecture

Vyoma provides container-like networking for MicroVMs with two distinct operational modes:
1. **Rootless Vsock L4 Stream Forwarding (Default)**: 100% rootless, zero sudo, zero TAP/bridge, microsecond shared-memory transport.
2. **Privileged TAP & Bridge Networking (Optional)**: Traditional Layer 2 network bridging for environments running with root / `CAP_NET_ADMIN`.

---

## 1. Rootless Vsock L4 Architecture (Default)

In modern development workflows (laptops, CI/CD pipelines, Kubernetes worker nodes), developers run without `root` privileges. Vyoma's primary networking engine operates entirely in user space through the Linux kernel's `virtio-vsock` subsystem.

```
Host (Unprivileged User)                          MicroVM Guest (Hardware Isolated)
┌──────────────────────────────────────────┐     ┌──────────────────────────────────────────────┐
│  Client (Browser / curl / API)           │     │                                              │
│         │ (TCP connect)                  │     │                                              │
│         ▼                                │     │                                              │
│  vyomad ProxyManager (0.0.0.0:8088)      │     │                                              │
│         │                                │     │                                              │
│         ▼ (Unix Domain Socket)           │     │                                              │
│  vsock.sock (Cloud-Hypervisor Backend)   │     │                                              │
│         │                                │     │                                              │
│  CONNECT 10000\n                         │     │                                              │
│         │                                │     │                                              │
│         ▼                                │     │                                              │
│  Virtio-Vsock Ring Buffer (Zero-Copy)    │====>│  AF_VSOCK Port 10000 (vyoma-agent-vm)        │
│         │                                │     │        │                                     │
│  Port Payload: "80\n"                    │     │        ▼ (TCP Connect 127.0.0.1:80)          │
│         │                                │     │  Container Application (Nginx :80)           │
│  tokio::io::copy_bidirectional           │<===>│  tokio::io::copy_bidirectional              │
└──────────────────────────────────────────┘     └──────────────────────────────────────────────┘
```

### Components & Lifecycle

1. **Host Unix Domain Socket**:
   - Cloud-Hypervisor exposes a standard host Unix socket (`<vm_dir>/vsock.sock`) configured via `--vsock cid=<CID>,socket=<path>`.
   - Creating, opening, and streaming over this socket requires standard user-level permissions with **zero sudo**.

2. **Host Proxy Manager (`crates/vyoma-core/src/proxy.rs`)**:
   - Listens on published host ports (e.g. `0.0.0.0:8088`).
   - On inbound TCP connection, connects to `<vm_dir>/vsock.sock`.
   - Sends the Cloud-Hypervisor control command: `CONNECT 10000\n`.
   - Receives hypervisor acknowledgement: `OK <assigned_port>\n`.
   - Sends the target container port: `80\n`.
   - Receives in-VM agent confirmation: `OK\n`.
   - Slices host TCP and Vsock stream using Tokio's asynchronous `copy_bidirectional`.

3. **In-VM Agent (`vyoma-agent-vm`)**:
   - Static musl binary (`/sbin/vyoma-agent-vm`) injected into the microVM initramfs.
   - Listens on `AF_VSOCK` at `VMADDR_CID_ANY` on port `10000` (port-forwarding proxy multiplexer).
   - Dynamically routes inbound streams to `127.0.0.1:<target_port>`.
   - Includes a 1.5s active retry loop (30 attempts × 50ms) to ensure container applications finishing startup (e.g. Nginx, Node.js) accept incoming connections cleanly without dropping early packets.

4. **In-VM Introspection Protocol (Port 9999)**:
   - `vyoma-agent-vm` also serves guest introspection (`GetMetrics`, `ProcessList`, `FileRead`, `ExecCommand`) over vsock port `9999`.
   - Allows daemon and CLI to introspect, debug, and monitor workloads even when zero IP interfaces are attached.

---

## 2. Privileged TAP & Bridge Networking (Optional)

For enterprise bare-metal deployments where root or `CAP_NET_ADMIN` is available and VMs require direct Layer 2 broadcast access to a physical LAN:

```
Host Interface (eth0 / wlan0)
           │
     [iptables NAT] (Masquerade)
           │
     vyoma0 Bridge (172.16.0.1/16)
       ├── Embedded DNS Server (Port 53)
       ├── vyoma-tap-1 ────────── VM 1 (172.16.0.2)
       ├── vyoma-tap-2 ────────── VM 2 (172.16.0.3)
       └── vyoma-tap-3 ────────── VM 3 (172.16.0.4)
```

1. **Linux Bridge (`vyoma0`)**: Gateway interface (`172.16.0.1`).
2. **TAP Interfaces**: Provisioned via `ip tuntap add dev tapXXXXXXXX mode tap user vyoma`.
3. **Graceful Fallback**: If `ip tuntap add` fails due to lack of privileges, `vyomad` automatically detects rootless mode and routes all port publishing through the Vsock L4 engine without failure.

---

## 3. Embedded DNS & Service Discovery

`vyomad` includes an embedded UDP DNS resolver bound to `172.16.0.1:53` when running in bridged environments:
- **Authoritative Zone**: Resolves all requests in the `.vyoma` domain (e.g. `web.vyoma`, `db.vyoma`).
- **Compose Stacks**: Resolves service names declared in `vyoma-compose.yml`.
- **Round-Robin DNS**: Automatically distributes traffic across multiple instances when scaled (`vyoma scale web=3`).
- **Upstream Forwarding**: External queries are forwarded to system resolvers (`127.0.0.53` or `8.8.8.8`).

---

## 4. Multi-Host Swarm Overlay

When multiple nodes join a **Vyoma Swarm**, an overlay network is established:

```
 Node 1 (192.168.1.10)                     Node 2 (192.168.1.11)
 Subnet: 10.42.1.0/24                     Subnet: 10.42.2.0/24
 ┌──────────────────────┐                 ┌──────────────────────┐
 │  VM A (10.42.1.5)    │                 │  VM B (10.42.2.8)    │
 └──────────┬───────────┘                 └──────────┬───────────┘
            │                                        │
     [vxlan-overlay]                          [vxlan-overlay]
            │                                        │
     [WireGuard Mesh] ◄══════════════════════► [WireGuard Mesh]
```

- **Deterministic Subnet Allocation**: Node subnets derived from Raft node ID (`10.42.<node_id>.0/24`).
- **VXLAN Encapsulation**: Layer 2 encapsulation over UDP port `4789`.
- **WireGuard Mesh**: Inter-node traffic authenticated and encrypted using ChaCha20-Poly1305.

---

## 5. Performance & Security Comparison

| Metric / Dimension | Traditional Docker (RunC) | Traditional MicroVM (TAP/Bridge) | Vyoma Rootless Vsock L4 |
| :--- | :--- | :--- | :--- |
| **Host Privilege Requirement** | Requires `root` or rootlesskit namespaces | Requires `root` / `CAP_NET_ADMIN` | **100% Rootless (Zero sudo)** |
| **Syscall Isolation** | Shared host kernel (`/proc`, `/sys`) | Hardware KVM barrier | **Hardware KVM barrier** |
| **Port Forwarding Latency** | Userspace proxy / iptables NAT (~150μs) | Kernel TAP / Bridge (~120μs) | **Shared-Memory Virtio-Vsock (~35μs)** |
| **Host Interface Leakage** | Veth pairs left on unclean exit | Orphan TAP interfaces | **Zero host network devices** |
| **CI/CD / Container-in-Container** | Requires `--privileged` docker-in-docker | Fails without nested TAP perms | **Runs in any unprivileged KVM context** |
