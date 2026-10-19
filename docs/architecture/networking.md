# Networking Subsystem Architecture

Vyoma provides container-like networking for Micro-VMs using Linux Bridges, TAP devices, embedded DNS, and multi-host overlays.

---

## 1. Local Network Topology

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

### Components
1. **Linux Bridge (`vyoma0`)**: Serves as the default gateway (`172.16.0.1`) for local VMs.
2. **TAP Interfaces**: Each VM is assigned a dedicated TAP interface (`vyoma-tap-N`) connected directly to the bridge.
3. **NAT Masquerade**: Outbound traffic is masqueraded via `iptables` rules to allow internet access from inside the guest.
4. **Port Forwarding**: Implemented via userspace TCP proxies (Tokio tasks) running in `vyomad`, binding the host port and pumping traffic to `VM_IP:VM_PORT`.

---

## 2. Embedded DNS & Service Discovery

`vyomad` runs an embedded UDP DNS resolver bound to the gateway IP `172.16.0.1:53`:
- **Authoritative Zone**: Resolves all requests in the `.vyoma` domain (e.g. `web.vyoma`, `db.vyoma`).
- **Compose Stacks**: Resolves service names declared in `vyoma-compose.yml`.
- **Round-Robin DNS**: Automatically distributes traffic across multiple instances when scaled (`vyoma scale web=3`).
- **Upstream Forwarding**: All external queries are forwarded to the host's system resolver (e.g., `127.0.0.53` or `8.8.8.8`).

---

## 3. Multi-Host Swarm Overlay

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

- **Deterministic Subnet Allocation**: Node subnets are derived deterministically from the Raft node ID (`10.42.<node_id>.0/24`).
- **VXLAN Encapsulation**: Transparent Layer 2 encapsulation over UDP port `4789`.
- **WireGuard Mesh**: Inter-node traffic is authenticated and encrypted using ChaCha20-Poly1305 via WireGuard.
