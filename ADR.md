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

## 011. Volume Mount Strategy (VirtioFS)
*   **Date**: 2025-12-24
*   **Decision**: Use **VirtioFS** with the Rust-based `virtiofsd` binary for sharing host directories.
*   **Reasoning**:
    *   Standard way to share files with Firecracker.
    *   Performance is near-native for cached reads.
    *   Allows "Hot Reload" workflows (editing code on host, running in VM).
*   **Implementation**:
    *   **Dependency**: Requires `virtiofsd` binary in `bin/` or system PATH.
    *   **Daemon**: Spawns a dedicated `virtiofsd` process for *each* shared volume (or one per VM handling multiple paths if supported, but usually one socket per fs).
    *   **Socket**: `virtiofsd` listens on a Unix socket, Firecracker connects to it.
    *   **Kernel**: Depends on guest kernel having `virtiofs`.

## 012. Builder Strategy (Ignitefile)
*   **Date**: 2025-12-24
*   **Decision**: Implement `ign build` via a Client-Server model where the Daemon performs the build using `chroot` for `RUN` instructions.
*   **Reasoning**:
    *   **Context**: The daemon manages the image store (`~/.ignite/images`), which is often root-owned or privileged.
    *   **Performance**: `RUN` commands are executed via `chroot` on a mounted loopback device of the image. This avoids the overhead of booting a full Firecracker VM for every build step, similar to how Docker builds work (mostly).
    *   **Simplicity**: We mimic Docker's context sending (streaming tarball to daemon).
*   **Directives MVP**:
    *   `FROM <image>`: Starts from a base image.
    *   `RUN <cmd>`: Executes command in chroot.
    *   `COPY <src> <dest>`: Copies files from build context to image.

## 013. Resource Limits Strategy (Cgroups v2)
*   **Date**: 2025-12-24
*   **Decision**: Use Cgroups v2 explicitly to manage VM resource limits (CPU, Memory).
*   **Reasoning**:
    *   **Modern Standard**: Cgroups v2 is the standard on modern Linux (Ubuntu 22.04+).
    *   **Firecracker Integration**: Firecracker supports running inside a Cgroup. We will create a parent cgroup `ignite.slice` and sub-cgroups for each VM `ignite-<id>.scope`.
    *   **Implementation**: We will use direct file system manipulation of `/sys/fs/cgroup/ignite.slice/` for simplicity and control, rather than `systemd-run` for now, unless `systemd` integration is strictly required. Direct FS manipulation is more educational and portable for a "from scratch" project build.

## 014. Rootless Strategy (Future)
*   **Date**: 2025-12-25
*   **Status**: Proposed / In Progress
*   **Context**: Running  as root is a security risk.
*   **Decision**: We will transition to "Rootless" capability using **Slirp4netns** or **Passt** for unprivileged networking.
*   **Challenges**:
    1.  **Networking**: Creating TAP/Bridge requires root. Usage: slirp4netns [OPTION]... PID|PATH [TAPNAME]
User-mode networking for unprivileged network namespaces.

-c, --configure          bring up the interface
-e, --exit-fd=FD         specify the FD for terminating slirp4netns
-r, --ready-fd=FD        specify the FD to write to when the network is configured
-m, --mtu=MTU            specify MTU (default=1500, max=65521)
-6, --enable-ipv6        enable IPv6 (experimental)
-a, --api-socket=PATH    specify API socket path
--cidr=CIDR              specify network address CIDR (default=10.0.2.0/24)
--disable-host-loopback  prohibit connecting to 127.0.0.1:* on the host namespace
--netns-type=TYPE 	 specify network namespace type ([path|pid], default=pid)
--userns-path=PATH	 specify user namespace path
--enable-sandbox         create a new mount namespace (and drop all caps except CAP_NET_BIND_SERVICE if running as the root)
--enable-seccomp         enable seccomp to limit syscalls (experimental)
--outbound-addr=IPv4     sets outbound ipv4 address to bound to (experimental)
--outbound-addr6=IPv6    sets outbound ipv6 address to bound to (experimental)
--disable-dns            disables 10.0.2.3 (or configured internal ip) to host dns redirect (experimental)
--macaddress=MAC         specify the MAC address of the TAP (only valid with -c)
--target-type=TYPE       specify the target type ([netns|bess], default=netns)
-h, --help               show this help and exit
-v, --version            show version and exit runs in userspace.
    2.  **Storage**:  requires root. We must move to user-space mounting (FUSE) or rely on direct file usage (Firecracker supports raw files without mounting).
    3.  **KVM**: Requires user to be in  group.
*   **Phasing**:
    *   Phase 1 is establishing the Rootless Architecture.
    *   Currently, we will focus on investigating these requirements in a separate branch .

## 014. Rootless Strategy (Future)
*   **Date**: 2025-12-25
*   **Status**: Proposed / In Progress
*   **Context**: Running `ignited` as root is a security risk.
*   **Decision**: We will transition to "Rootless" capability using **Slirp4netns** or **Passt** for unprivileged networking.
*   **Challenges**:
    1.  **Networking**: Creating TAP/Bridge requires root. `slirp4netns` runs in userspace.
    2.  **Storage**: `mount -o loop` requires root. We must move to user-space mounting (FUSE) or rely on direct file usage (Firecracker supports raw files without mounting).
    3.  **KVM**: Requires user to be in `kvm` group.
*   **Phasing**:
    *   Phase 1 is establishing the Rootless Architecture.
    *   Currently, we will focus on investigating these requirements in a separate branch `feat/rootless`.

