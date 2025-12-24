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

## 007. Verification Strategy & Known Gaps
*   **Date**: 2025-12-23
*   **Context**: Development is happening in a **Pure Linux (Ubuntu)** environment. Root/Sudo and KVM permissions should be available.
*   **Decision**:
    1.  Implement logic robustly using best-practice wrappers.
    2.  Write Integration Tests for all privileged operations but mark them as `#[ignore]`.
    3.  Track skipped verifications explicitly.
*   **Known Gaps (requiring manual verification)**:
    *   **Storage Population**: `mount` and `cp` require `sudo`. Logic confirmed via `test_storage_population` but requires manual run.
    *   **Loopback/DM**: `losetup` and `dmsetup` require `sudo`.
    *   **Networking**: `ip link` and `iptables` require `NET_ADMIN`/`sudo`.
    *   **Firecracker Boot**: Requires user to be in `kvm` group or have RW access to `/dev/kvm`.
*   **Risk Mitigation**:
    *   The `ignite-core` library is designed to be modular. If one component fails (e.g., networking), the others (storage) remain testable.
    *   Future CI pipeline MUST run on a bare-metal or nested-virt enabled runner with passwordless sudo to fully validate the `#[ignore]` tests.

## 008. Daemon State Management
*   **Date**: 2025-12-24
*   **Decision**: Store active VM instances in `ignited` memory using `Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<VmmManager>>>>>`.
*   **Reasoning**:
    *   Daemon is the source of truth for running processes.
    *   `VmmManager` owns the `std::process::Child` handle.
    *   `tokio::sync::Mutex` allows locking the VMM handle across async API calls (like pause/resume) which wait for Firecracker's HTTP response.
*   **Consequences**:
    *   Daemon restart loses control of running VMs (orphaned processes). (Future task: State persistence/recovery).

## 009. Port Mapping Strategy (Phase 8)
*   **Date**: 2025-12-24
*   **Decision**: Use userspace TCP proxying (Tokio tasks) instead of `iptables` DNAT.
*   **Reasoning**:
    1.  **Flexibility**: Allows mapping `localhost:8080` to VM `80` without managing complex NAT tables or avoiding port conflicts on the bridge.
    2.  **Safety**: Isolates the port opening logic to the `ignited` process. If the daemon dies, the ports close automatically (unlike iptables rules which persist).
    3.  **Future-Proofing**: Aligns with "Rootless" goals (Phase 11). Userspace proxies don't strictly *need* root (if binding non-privileged ports), whereas `iptables` always does.
*   **Implementation**:
    *   Spawn a `tokio::task` for each mapped port.
    *   Bind `0.0.0.0:HOST_PORT`.
    *   Accept connections and pump bytes to `VM_IP:VM_PORT`.

## 010. Log Streaming Strategy
*   **Date**: 2025-12-24
*   **Decision**: Use `tokio::sync::broadcast` + Server-Sent Events (SSE) for log streaming.
*   **Reasoning**:
    *   `broadcast` generic channel allows multiple consumers (though we currently use one main one, it allows future expansion like "ign logs" + "dashboard" simultaneously).
    *   Firecracker logs (stdout/stderr) are captured via pipes and immediately pushed to the broadcast channel.
    *   SSE (`text/event-stream`) is a standard HTTP protocol for streaming updates, supported natively by browsers and easy to consume in CLI via `reqwest`.
    *   Avoids complex WebSocket setup just for read-only logs.
*   **Consequences**:
    *   Clients must handle SSE parsing (implemented in CLI).
    *   Logs are transient in memory (buffer size 100). If no one is listening, logs are dropped. (Acceptable for "streaming" logs, but means we don't have "history" unless we implement persistent logging).
