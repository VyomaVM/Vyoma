# Architectural Decision Records (ADR)

This document tracks significant architectural decisions, their context, consequences, and alternatives considered.

## 001. Project Naming & Branding
*   **Date**: 2025-12-23
*   **Decision**: Rename project from "Generic Micro-VM (MVM)" to **Ignite**.
*   **Visuals**: Daemon = `ignited`, CLI = `ign`.
*   **Context**: User requested a "premium" and energetic brand. "Ignite" aligns with Firecracker (the underlying VMM).
*   **Alternatives**: "Capsule" (discarded for being too generic/safe).

## 002. CLI Wrapper Strategy for System Operations
*   **Date**: 2025-12-23
*   **Decision**: Use `std::process::Command` to wrap standard Linux utilities (`mkfs.ext4`, `losetup`, `mount`, `dmsetup`) instead of linking against native C libraries (`libdevmapper`, `libmount`).
*   **Reasoning**:
    1.  **Portability & Simplicity**: Reduces build-time dependencies (`build-essential` is enough). No need to handle complex C-to-Rust bindings/FFI issues during early prototyping.
    2.  **Debuggability**: It is much easier to print the exact CLI command and reproduce issues manually in the terminal than to debug an ioctl failure code.
    3.  **Stability**: Linux CLI tools have extremely stable interfaces.
*   **Consequences**:
    *   Performance: Slight overhead from spawning processes (negligible for infrequent ops like VM lifecycles).
    *   Error Handling: Must parse stderr/stdout strings instead of typed errors.
*   **Future Considerations**:
    *   For high-scale production usage (thousands of VMs/sec), we should migrate critical paths (like `dmsetup`) to native Rust crates (`devicemapper-rs`) to avoid `fork/exec` overhead.

## 003. OCI Image Handling
*   **Date**: 2025-12-23
*   **Decision**: Implement a custom simple OCI client in `ignite-core` using `reqwest` instead of using the `oci-distribution` crate.
*   **Reasoning**:
    *   The `oci-distribution` crate (v0.9.4) is heavy and had compatibility issues with recent tokio/http versions in our testing.
    *   We need specific control over handling OCI Indexes vs Docker V2 Manifests to force `linux/amd64` resolution.
*   **Consequences**:
    *   We own the OCI parsing logic (maintenance burden).
    *   We can easily customize authentication logic (Docker Hub vs private registries).

## 004. Database / State Management
*   **Status**: Pending
*   **Context**: We need to track running VMs, allocated IPs, and active loop devices.
*   **Current Direction**: Likely file-based JSON/ToML in `~/.ignite/state` for MVP.

## 005. Networking Strategy
*   **Date**: 2025-12-23
*   **Decision**: Use Linux Bridge (`brctl`/`ip link`) + TAP interfaces + `iptables` NAT.
*   **Reasoning**:
    *   Standard Docker-like networking model.
    *   Allows VMs to communicate with each other (via bridge) and internet (via NAT).
*   **Alternatives**:
    *   **MacVTap**: Higher performance, but harder to perform host-to-VM communication (hairpin mode issues).
    *   **User-mode Networking (slirp)**: Safer (no root needed), but much slower and harder to expose ports.
*   **Consequences**:
    *   Requires `NET_ADMIN` capability or Root.
    *   Daemon must run with high privileges.

## 006. Storage Stategy: Device Mapper
*   **Date**: 2025-12-23
*   **Decision**: Use `dm-snapshot` for instant cloning.
*   **Reasoning**:
    *   Allows starting 100 VMs from 1 base image with minimal space overhead.
    *   Standard Linux kernel feature (stable).
*   **Implementation**: Wrapped `dmsetup` CLI.
