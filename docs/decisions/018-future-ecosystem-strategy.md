# 18. Future Ecosystem Strategy (v1.0+)

Date: 2026-01-24

## Status

Proposed

## Context

To ensure **Vyoma v1.0** is a complete "Docker Desktop Alternative" and viable for AI/Cluster workloads, we must integrate these ecosystems BEFORE the stable release.
These features will be delivered in **v0.8 (Workloads)** and **v0.9 (Distribution)**.

## Key Strategic Pillars

### 1. Kubernetes Compatibility
Vyoma should function as a container runtime or have a CRI (Container Runtime Interface) shim.
*   **Goal**: Allow Kubernetes nodes to schedule pods as Vyoma MicroVMs.
*   **Implementation**: A `containerd` shim (`shim-vyoma-v2`) or a direct Virtual Kubelet provider.

### 2. AI & LLM Workloads
Support running Large Language Models (LLMs) efficiently.
*   **Requirements**:
    *   GPU Passthrough (if available).
    *   Optimized AVX512 support for CPU inference.
    *   Pre-packaged "AI Stack" images (Ollama/Llama inside VM).
*   **MCP Support**: Compatibility with Model Context Protocol servers (running agents inside VMs).

### 3. Distribution Strategy
We will split the delivery model based on the user persona:

*   **Developer/Student**:
    *   **Artifact**: All-in-one Installer (MSI/DMG/Deb).
    *   **Experience**: "Desktop Mode". Includes CLI + Daemon + **Web UI** (Dashboard).
    *   **Features**: Auto-updating, easy networking, visual management.

*   **Server/Production**:
    *   **Artifact**: Standard package manager (`apt-get install vyomad`).
    *   **Experience**: Headless Daemon + CLI.
    *   **Features**: Systemd integration, telemetry, swarm mode enabled.

## Decision

We will prioritize **Stability** for v1.0, but architecture the v1.x series to support these ecosystem expansions. 
The **Web UI** will be the bridge for developer adoption.

## Consequences

- Need to design a Web/GUI Client (React/Rust-WASM).
- Need to research `containerd` shim implementation.
