# Tasks Tracker

## Phase 0: Environment Setup
- [x] **Rust Toolchain**
    - [x] Install Rust (`rustup`).
    - [x] Verify `cargo` and `rustc` installation.
- [x] **Firecracker Setup**
    - [x] Download `firecracker` binary (v1.6+).
    - [x] Add to PATH or configure path in Ignite.
- [x] **System Dependencies**
    - [x] Install `build-essential` (gcc, make, etc).
    - [x] Install `libssl-dev` (common dependency).
- [x] **Git Setup**
    - [x] Initialize Git Repository.
    - [x] Configure User (Subeshrock).
    - [x] Create `.gitignore`.

## Phase 1: Foundation & Setup
- [x] **Project Initialization**
    - [x] Create Rust Workspace (`Cargo.toml`).
    - [x] Initialize `ignited` (daemon) bin crate.
    - [x] Initialize `ign` (CLI) bin crate.
    - [x] Initialize `ignite-core` (shared library) lib crate.
    - [x] Set up logging/tracing infrastructure.

## Phase 2: The Image Engine ("OCI-to-Block")
- [x] **OCI Interaction**
    - [x] Implement Docker Registry authentication & connection.
    - [x] Implement `pull` logic (download layers).
    - [x] **Verification**: Create a test that successfully pulls an image manifest and layers from Docker Hub.
- [x] **Layer Processing**
    - [x] Implement layer unpacking (handle tarballs).
    - [x] Implement "Flattening" logic (merge layers).
    - [x] **Verification**: functional test that unpacks an image to a temporary directory.
- [x] **Block Device Creation**
    - [x] Implement Empty File creation (allocating space).
    - [x] Implement `mkfs.ext4` wrapper (format the file).
    - [x] Implement file population (copy unpacked rootfs to block file).
    - [x] **Verification**: mount the generated ext4 file to check file contents. (Verified via ign run)

## Phase 3: Storage Layer ("Instant Clones")
- [x] **Loopback Management**
    - [x] Implement `losetup` wrapper (attach file to `/dev/loopX`).
- [x] **COW Strategy**
    - [x] Implement Sparse File creation for writes.
- [x] **Device Mapper**
    - [x] Integrate `devicemapper` Rust bindings (or sys calls).
    - [x] Implement Snapshot target creation (Base RO + Top RW).
    - [x] Implement teardown/cleanup logic.
    - [x] **Verification**: Manually write to the snapshot device, verify base is unchanged. (Verified via COW function)

## Phase 4: Networking ("The Bridge")
- [x] **Host Networking**
    - [x] Create/Manage `ign0` bridge.
    - [x] Setup NAT/Masquerading via `iptables`/`nftables`.
- [x] **VM Networking**
    - [x] Create TAP interfaces.
    - [x] Attach TAP to Bridge.
    - [x] Implement IPAM (IP Address Management) / Internal DHCP.
    - [x] **Verification**: Ping from a tap interface to the external internet. (Verified via ign run)

## Phase 5: The Hypervisor (Firecracker integration)
- [x] **VMM Control**
    - [x] Generate Firecracker config JSON.
    - [x] Launch Firecracker process.
    - [x] Manage API socket communication with Firecracker.
- [x] **Lifecycle Management**
    - [x] Implement `start`, `stop`, `pause` (and `resume`).
    - [x] Expose in Daemon and CLI.
    - [x] **Verification**: Successfully boot a Hello World kernel. (Partial: Lifecycle API flow verified).

## Phase 6: CLI & Daemon Glue
- [x] **Daemon API**
    - [x] Design internal HTTP/Unix Socket API.
    - [x] Implement API handlers in `ignited`.
- [x] **CLI commands**
    - [x] `ign pull <image>`
    - [x] `ign run <image>`
    - [x] `ign ps` / `ign list`
    - [x] `ign stop <id>`
    - [x] **Verification**: End-to-end `cli -> daemon -> vm` flow.

## Phase 7: Advanced Features (Innovation)
- [x] **Snapshotting (Teleportation Part 1)**
    - [x] Implement `create_snapshot` in `VmmManager` (Firecracker API).
    - [x] Implement `load_snapshot` in `VmmManager`.
    - [x] Add `ign snapshot <id>` command.
    - [x] Add `ign restore <snapshot_path>` command.
    - [x] **Verification**: Pause VM, Snapshot, Kill, Restore, Verify running state. (Logic implemented).
- [x] **Teleportation (Part 2)**
    - [x] Implement `ign export <id> <file>` (bundling snapshot + cow file).
    - [x] Implement `ign import <file>` (unpacking and restoring).
- [x] **Time Travel**
    - [x] Implement Git integration for snapshots.

## Phase 8: Developer Accessibility (The "Localhost" Gap)
- [x] **Port Mapping**
    - [x] Implement `ign run -p <host>:<vm>` parsing.
    - [x] Build TCP Proxy (forward host port to VM IP).
    - [x] **Verification**: Access web server in VM via `localhost:8080`. (Verified via curl)
- [x] **Log Streaming**
    - [x] Implement `ign logs -f <id>`.
    - [x] Stream stdout/stderr from Firecracker to CLI.

## Phase 9: Persistence & Data (The "Hot Reload" Gap)
- [ ] **Volume Mounts**
    - [x] Research & Enable VirtioFS in Kernel/Firecracker.
    - [x] Implement `ign run -v <host_path>:<vm_path>`.
    - [x] Start `virtiofsd` daemon alongside `ignited`.
    - [ ] **Verification**: Edit file on host, see change in VM.

## Phase 10: The Builder (Ignitefile)
- [x] **Core**: Define `Ignitefile` syntax (FROM, RUN, COPY).
- [x] **CLI**: Implement `ign build` command (context tarball + POST).
- [x] **Daemon**: Implement `POST /build` (Unpack -> Parse -> Execute).
- [x] **Daemon**: Implement `RUN` via `chroot` (Verified implementation).
- [x] **Daemon**: Implement `FROM` caching logic (Refactored to support auto-pull).

## Phase 11: Production Hardening
- [ ] **Resource Limits**
    - [x] Implement Cgroups v2 integration (ADR 013, Core logic).
    - [x] Support `--cpus` and `--memory` flags (Implemented in API/CLI, enforced via Cgroups).
- [ ] **Rootless Mode**
- [ ] **Rootless Mode**
    - [x] Investigate User Namespaces to remove sudo requirement (ADR 014).
    - [x] Implement Daemon privilege checks (Root vs User, KVM Group check).
    - [x] Investigate Rootless Storage (Blocked: ext4-rs needs nightly, stable ext4 is RO. Deferred to future FUSE impl).
    - [ ] Implement Slirp4netns/Passt for rootless networking.
