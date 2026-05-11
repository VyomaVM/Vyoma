# 17. Overlay Networking Strategy

Date: 2026-01-24

## Status

Proposed

## Context

We need to enable communication between VMs running on different hosts (Ignite Swarm). 
The solution must support:
1.  Unique IP per VM across the cluster.
2.  Routing between hosts.
3.  **Rootless compatibility** (Ideally).

## Options

### 1. VXLAN (Flannel/Calico)
Standard Kubernetes approach. Encapsulates L2 frames in UDP.
*   **Pros**: Standard, performant (hardware offload).
*   **Cons**: Creating VXLAN interfaces usually requires host Root privileges (`CAP_NET_ADMIN` in init ns). Hard for Rootless.

### 2. User-Mode WireGuard (Tailscale/Netmaker)
Mesh VPN.
*   **Pros**: Secure, works over NAT, works in User Mode (tun device).
*   **Cons**: Performance overhead (encryption), complexity of key management.

### 3. User-Mode IP-over-UDP (Simple Overlay)
Custom tap-to-udp bridge.
*   **Pros**: Simple to implement.
*   **Cons**: Reinventing the wheel, performance.

## Decision

We will first validate **Flannel** for Rootful (Standard) mode.
For Rootless, we will investigate if **Slirp4netns** can integrate with a multi-host backend or if we need a userspace router.

**Plan**:
1.  Implement `ign network create --driver=overlay`.
2.  In Rootful mode, this invokes `flannel` CNI (managed by us or external).
3.  In Rootless mode, we mark as "Experimental/Not Supported" initially, or fallback to a userspace tunnel (to be researched).

## Consequences

- Dependency on `flannel` binary.
- Requires Daemon to manage a subnet lease (ETCD/Consul or Gossip).
