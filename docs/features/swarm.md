# Vyoma Swarm (Clustering & Raft Consensus)

**Vyoma Swarm** turns a pool of individual physical machines into an encrypted, multi-host Micro-VM cluster.

---

## 1. Cluster Architecture

Swarm uses **Raft Consensus** ([ADR-031](file:///home/subesh/Startup_Projects/Vyoma/vyoma/docs/adrs/adr-031-raft.md)) powered by the `openraft` crate:
- **No Single Point of Failure**: Eliminates reliance on a single seed node.
- **State Replication**: Node registration, VM placement, and service definitions are replicated across the Raft cluster.
- **Automatic Leader Election**: If the leader fails, followers automatically elect a new coordinator.

---

## 2. Setting Up a Cluster

### Step 1: Initialize the Seed Node

On the first machine:

```bash
vyoma swarm init --advertise-addr 192.168.1.10
```

Output:
```text
✓ Swarm cluster initialized. Node ID: 1
✓ Raft consensus engine running.
Join token generated: vyoma-token-xyz...
```

### Step 2: Join Worker Nodes

On other machines in your LAN or cloud network:

```bash
vyoma swarm join 192.168.1.10
```

### Step 3: Inspect Cluster State

```bash
vyoma swarm ls
```

Output:
```text
NODE ID   ADDRESS            ROLE      STATUS    SUBNET          VMS
1         192.168.1.10:7946  Leader    Ready     10.42.2.0/24    4
2         192.168.1.11:7946  Follower  Ready     10.42.3.0/24    2
3         192.168.1.12:7946  Follower  Ready     10.42.4.0/24    3
```

---

## 3. Overlay Networking & WireGuard

Each node automatically sets up:
1. **Subnet Assignment**: Unique, non-overlapping `/24` subnet per node (`10.42.<node_id>.0/24`).
2. **VXLAN Interface**: Virtual interface (`vyoma-vxlan`) encapsulating VM traffic across nodes.
3. **WireGuard Encryption**: Secure tunnels encrypting all inter-node VM traffic over UDP port `51820`.

For migration details from legacy seed setups, refer to the [Swarm Migration Guide](file:///home/subesh/Startup_Projects/Vyoma/vyoma/docs/features/swarm-migration.md).
