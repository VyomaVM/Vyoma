# 16. Cluster Architecture (Vyoma Swarm)

Date: 2026-01-24

## Status

Proposed

## Context

We want to orchestrate VMs across multiple hosts ("Nodes"). 
We need:
1.  **Node Discovery**: How nodes find each other.
2.  **State Sharing**: How we know which VM is where.
3.  **Scheduling**: Deciding where to run a VM.

## Options

### 1. Centralized (Master/Worker)
Like Kubernetes. One node is Leader.
*   **Pros**: Simple mental model, strong consistency.
*   **Cons**: SPOF (Single Point of Failure), requires HA setup.

### 2. Decentralized (Gossip)
Like Serf/Memberlist.
*   **Pros**: Resilient, no master.
*   **Cons**: Convergence time, eventual consistency.

### 3. Static Mesh
Manual `vyoma node add <ip>`.
*   **Pros**: Trivial MVP.
*   **Cons**: Manual management.

## Decision

We will implement **Option 3 (Static)** for MVP, graduating to **Option 2 (Gossip/Memberlist)**.
Vyoma is designed to be lightweight. A heavy Etcd/Raft consensus is overkill for v0.7.0.

**Architecture**:
- Each Daemon exposes a Cluster API (gRPC or HTTP).
- `vyoma swarm init`: Becomes a "Seed".
- `vyoma swarm join <seed-ip>`: Joins the mesh.
- State (VM List) is broadcasted periodically (Anti-Entropy).

## Data structures
- `NodeList`: Map<NodeID, NodeInfo { IP, Capacity }>.
- `GlobalServiceMap`: Map<ServiceName, List<VMID, NodeID>>.

## Consequences

- We need to handle network partitions.
- We need authentication (Shared Secret or mTLS).
